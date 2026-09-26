//! 评测后端：OpenAI 兼容 chat/completions 调用 + keyword/llm 两种判定。

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// tests.yaml 中的 backend 配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BackendConfig {
    pub provider: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub base_url: String,
    pub api_key_env: String,
}

impl Default for BackendConfig {
    fn default() -> Self {
        BackendConfig {
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            temperature: 0.2,
            max_tokens: 512,
            base_url: String::new(),
            api_key_env: String::new(),
        }
    }
}

/// 解析后的实际后端参数（API key 已从环境变量拿到）。
#[derive(Debug, Clone)]
pub struct ResolvedBackend {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
}

impl ResolvedBackend {
    pub fn provider_label(&self) -> String {
        self.base_url.clone()
    }
}

/// 解析 provider + 环境变量，得到可用的后端配置。
pub fn resolve_backend(backend: &BackendConfig) -> Result<ResolvedBackend> {
    let provider = backend.provider.trim().to_ascii_lowercase();
    let mut model = backend.model.trim().to_string();
    if model.is_empty() {
        model = "gpt-4o-mini".to_string();
    }

    let (base_url, default_key_env) = match provider.as_str() {
        "openai" => (
            "https://api.openai.com/v1".to_string(),
            "OPENAI_API_KEY".into(),
        ),
        "deepseek" => (
            "https://api.deepseek.com/v1".to_string(),
            "DEEPSEEK_API_KEY".into(),
        ),
        "custom" | "openai-compatible" => {
            let base = backend.base_url.trim().trim_end_matches('/').to_string();
            if base.is_empty() {
                bail!(
                    "backend.provider 为 custom 时必须配置 backend.base_url（OpenAI 兼容接口地址）"
                );
            }
            (base, "PROMPT_GIT_API_KEY".into())
        }
        other => bail!("未知 backend.provider: {other}（支持 openai / deepseek / custom）"),
    };

    // 环境变量统一覆盖 model
    if let Ok(m) = std::env::var("PROMPT_GIT_MODEL") {
        if !m.trim().is_empty() {
            model = m.trim().to_string();
        }
    }

    let key_env = if backend.api_key_env.trim().is_empty() {
        default_key_env
    } else {
        backend.api_key_env.trim().to_string()
    };
    let api_key = std::env::var(&key_env).map_err(|_| {
        let local_hint = if provider == "custom" {
            "（或在 tests.yaml 用 backend.api_key_env 指定自定义环境变量名）".to_string()
        } else {
            String::new()
        };
        anyhow!("未找到 API Key：请设置环境变量 {key_env}{local_hint}")
    })?;

    // base_url 也可被环境变量覆盖（本地模型 / 代理）
    let base_url = std::env::var("PROMPT_GIT_BASE_URL")
        .ok()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or(base_url);

    Ok(ResolvedBackend {
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        api_key,
        model,
        temperature: backend.temperature,
        max_tokens: backend.max_tokens,
    })
}

#[derive(Debug, Clone)]
pub struct ChatResult {
    pub text: String,
    pub latency: Duration,
    pub total_tokens: Option<u64>,
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let short: String = s.chars().take(n).collect();
        format!("{short}…")
    }
}

/// 调用 chat/completions。
pub fn chat(backend: &ResolvedBackend, messages: &[ChatMessage]) -> Result<ChatResult> {
    let url = format!("{}/chat/completions", backend.base_url);
    let body = serde_json::json!({
        "model": backend.model,
        "messages": messages,
        "temperature": backend.temperature,
        "max_tokens": backend.max_tokens,
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .context("创建 HTTP 客户端失败")?;

    let t0 = Instant::now();
    let resp = client
        .post(&url)
        .bearer_auth(&backend.api_key)
        .json(&body)
        .send()
        .with_context(|| format!("请求 {} 失败（网络或接口不可达）", url))?;
    let latency = t0.elapsed();
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        bail!("API 请求失败 [{status}] {}", truncate(&text, 500));
    }

    let json: serde_json::Value = resp.json().context("解析 API 响应失败")?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| {
            anyhow!(
                "API 响应缺少 choices[0].message.content：{}",
                truncate(&json.to_string(), 300)
            )
        })?;
    let total_tokens = json["usage"]["total_tokens"].as_u64();
    Ok(ChatResult {
        text: content,
        latency,
        total_tokens,
    })
}

// ---------------- 判定 ----------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestCase {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
    #[serde(default)]
    pub expect_contains: Vec<String>,
    #[serde(default)]
    pub expect_not_contains: Vec<String>,
    #[serde(default)]
    pub expect_exact: Option<String>,
    #[serde(default)]
    pub min_length: Option<usize>,
    #[serde(default)]
    pub max_length: Option<usize>,
    #[serde(default)]
    pub judge_prompt: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
pub struct CaseResult {
    pub name: String,
    pub passed: bool,
    pub reasons: Vec<String>,
    pub latency: Duration,
    pub output_preview: String,
    pub tokens: Option<u64>,
}

const PREVIEW_CHARS: usize = 220;

/// keyword 判定：不消耗 token，完全离线。
pub fn judge_keyword(case: &TestCase, output: &str) -> CaseResult {
    let mut reasons: Vec<String> = Vec::new();
    let lower = output.to_lowercase();
    for kw in &case.expect_contains {
        if !lower.contains(&kw.to_lowercase()) {
            reasons.push(format!("缺少关键词：{kw}"));
        }
    }
    for kw in &case.expect_not_contains {
        if lower.contains(&kw.to_lowercase()) {
            reasons.push(format!("出现禁用词：{kw}"));
        }
    }
    if let Some(exact) = &case.expect_exact {
        if output.trim() != exact.trim() {
            reasons.push("与 expect_exact 不完全一致".to_string());
        }
    }
    let len = output.chars().count();
    if let Some(min) = case.min_length {
        if len < min {
            reasons.push(format!("输出过短：{len} 字符 < {min}"));
        }
    }
    if let Some(max) = case.max_length {
        if len > max {
            reasons.push(format!("输出过长：{len} 字符 > {max}"));
        }
    }
    CaseResult {
        name: case.name.clone(),
        passed: reasons.is_empty(),
        reasons,
        latency: Duration::ZERO,
        output_preview: truncate(output, PREVIEW_CHARS),
        tokens: None,
    }
}

/// llm 判定：让模型判断输出是否达标（返回 PASS/FAIL）。
pub fn judge_llm(backend: &ResolvedBackend, case: &TestCase, output: &str) -> Result<CaseResult> {
    let instructions = case
        .judge_prompt
        .clone()
        .unwrap_or_else(|| "判断下面的模型输出是否符合用户要求。只回复 PASS 或 FAIL。".to_string());
    let user = format!("{instructions}\n\n【模型输出】\n{output}");
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: "你是 prompt 评测裁判。只回复 PASS 或 FAIL，不要输出其他任何内容。"
                .to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: user,
        },
    ];
    let res = chat(backend, &messages)?;
    let verdict = res.text.trim().to_uppercase();
    let passed = verdict.contains("PASS") && !verdict.contains("FAIL");
    let mut reasons = Vec::new();
    if !passed {
        reasons.push(format!("judge 判为失败：{}", truncate(&res.text, 120)));
    }
    Ok(CaseResult {
        name: case.name.clone(),
        passed,
        reasons,
        latency: res.latency,
        output_preview: truncate(output, PREVIEW_CHARS),
        tokens: res.total_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_judge_pass() {
        let c = TestCase {
            name: "t".into(),
            vars: BTreeMap::new(),
            expect_contains: vec!["你好".into()],
            expect_not_contains: vec!["无法".into()],
            expect_exact: None,
            min_length: Some(2),
            max_length: None,
            judge_prompt: None,
            enabled: true,
        };
        let r = judge_keyword(&c, "你好，欢迎！");
        assert!(r.passed, "{:?}", r.reasons);
    }

    #[test]
    fn keyword_judge_fail_reasons() {
        let c = TestCase {
            name: "t".into(),
            vars: BTreeMap::new(),
            expect_contains: vec!["NOTHERE".into()],
            expect_not_contains: vec!["BAD".into()],
            expect_exact: Some("exact".into()),
            min_length: Some(100),
            max_length: Some(3),
            judge_prompt: None,
            enabled: true,
        };
        let r = judge_keyword(&c, "BAD output");
        assert!(!r.passed);
        assert!(r.reasons.iter().any(|s| s.contains("NOTHERE")));
        assert!(r.reasons.iter().any(|s| s.contains("BAD")));
        assert!(r.reasons.iter().any(|s| s.contains("expect_exact")));
        assert!(r.reasons.iter().any(|s| s.contains("短")));
        assert!(r.reasons.iter().any(|s| s.contains("长")));
    }

    #[test]
    fn keyword_case_insensitive() {
        let c = TestCase {
            name: "t".into(),
            vars: BTreeMap::new(),
            expect_contains: vec!["HELLO".into()],
            expect_not_contains: vec![],
            expect_exact: None,
            min_length: None,
            max_length: None,
            judge_prompt: None,
            enabled: true,
        };
        let r = judge_keyword(&c, "hello world");
        assert!(r.passed);
    }
}
