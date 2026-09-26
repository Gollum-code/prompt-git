//! 评测门禁：`prompt-git test` 的核心逻辑。

use std::path::Path;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};

use crate::eval::{self, BackendConfig, ChatMessage, ResolvedBackend, TestCase};
use crate::template::{self, PromptSet, PROMPTS_DIR};

/// tests.yaml 顶层结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TestSuite {
    pub backend: BackendConfig,
    pub judge: String,
    pub fail_under: f64,
    pub cases: Vec<TestCase>,
}

impl Default for TestSuite {
    fn default() -> Self {
        TestSuite {
            backend: BackendConfig::default(),
            judge: "keyword".to_string(),
            fail_under: 1.0,
            cases: Vec::new(),
        }
    }
}

pub fn load_suite(text: &str) -> Result<TestSuite> {
    let mut suite: TestSuite = serde_yaml::from_str(text).context("解析 tests.yaml 失败")?;
    if suite.judge.is_empty() {
        suite.judge = "keyword".to_string();
    }
    match suite.judge.as_str() {
        "keyword" | "llm" => {}
        other => bail!("judge 仅支持 keyword / llm，当前为 {other}"),
    }
    if !(0.0..=1.0).contains(&suite.fail_under) {
        bail!("fail_under 必须在 0.0 ~ 1.0 之间");
    }
    Ok(suite)
}

/// 读取工作区 tests.yaml。
pub fn load_suite_from_disk(root: &Path) -> Result<TestSuite> {
    let p = root.join(PROMPTS_DIR).join("tests.yaml");
    if !p.is_file() {
        bail!(
            "未找到 {}，请先运行 `prompt-git init` 或创建测试用例",
            p.display()
        );
    }
    let text = std::fs::read_to_string(&p)
        .with_context(|| format!("读取 {} 失败", p.display()))?;
    load_suite(&text)
}

#[derive(Debug, Clone, Default)]
pub struct GateSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub gate_ok: bool,
}

/// 运行评测门禁。filter 按用例名子串过滤；dry_run 只渲染请求不调用模型。
pub fn run_gate(
    root: &Path,
    filter: Option<&str>,
    dry_run: bool,
) -> Result<GateSummary> {
    let ps = PromptSet::load(root)?;
    let suite = load_suite_from_disk(root)?;

    let cases: Vec<&TestCase> = suite
        .cases
        .iter()
        .filter(|c| c.enabled)
        .filter(|c| filter.is_none_or(|f| c.name.contains(f)))
        .collect();

    if cases.is_empty() {
        eprintln!("{} 没有匹配的测试用例", "警告:".yellow());
        return Ok(GateSummary::default());
    }

    let backend: Option<ResolvedBackend> = if dry_run {
        None
    } else {
        Some(eval::resolve_backend(&suite.backend)?)
    };

    let total = cases.len();
    let mut summary = GateSummary {
        total,
        ..Default::default()
    };

    for case in cases {
        // 合并变量：vars.yaml 默认值 < 用例 vars
        let mut values = ps.default_values();
        values.extend(case.vars.clone());

        let (system, user, unresolved) = template::render_prompt_set(&ps, &values)?;
        if !unresolved.is_empty() {
            eprintln!(
                "{} 用例 [{}] 存在未提供变量: {}（将原样保留在提示中）",
                "警告:".yellow(),
                case.name,
                unresolved.join(", ")
            );
        }
        let messages = template::build_messages(&system, &user);

        if dry_run {
            print_dry_run(&case.name, &messages);
            summary.skipped += 1;
            continue;
        }

        let backend = backend.as_ref().expect("backend resolved");
        let response = eval::chat(backend, &messages);
        match response {
            Ok(res) => {
                let result = if suite.judge == "llm" {
                    eval::judge_llm(backend, case, &res.text)?
                } else {
                    eval::judge_keyword(case, &res.text)
                };
                if result.passed {
                    summary.passed += 1;
                    println!(
                        "{}  {}  {}  {}",
                        "✔".green().bold(),
                        case.name.bright_white().bold(),
                        result
                            .latency
                            .as_millis()
                            .to_string()
                            .cyan()
                            .dimmed()
                            .to_string() + "ms",
                        token_suffix(result.tokens),
                    );
                } else {
                    summary.failed += 1;
                    println!("{}  {}  {}", "✘".red().bold(), case.name.bright_white().bold(), "失败".red());
                    for r in &result.reasons {
                        println!("      - {}", r.yellow());
                    }
                }
                if !result.output_preview.trim().is_empty() {
                    println!("      {}", preview_line(&result.output_preview));
                }
            }
            Err(err) => {
                summary.failed += 1;
                println!("{}  {}  {}", "✘".red().bold(), case.name.bright_white().bold(), "调用失败".red());
                println!("      {}", format!("{err:#}").yellow().dimmed());
            }
        }
    }

    // 汇总
    println!("\n{}", "━━━ 评测汇总 ━━━".bright_cyan().bold());
    let rate = if summary.total > 0 {
        summary.passed as f64 * 100.0 / summary.total as f64
    } else {
        0.0
    };
    let mode_note = if dry_run {
        "[dry-run，未调用模型]".cyan().dimmed().to_string()
    } else {
        String::new()
    };
    println!(
        "通过率: {}/{} ({:.0}{})  {}",
        summary.passed.to_string().green().bold(),
        summary.total,
        rate,
        "%".green().bold(),
        mode_note,
    );
    if !dry_run {
        let ratio = rate / 100.0;
        summary.gate_ok = ratio >= suite.fail_under;
        let verdict = if summary.gate_ok {
            format!("门禁通过（要求 ≥ {:.0}%）", suite.fail_under * 100.0)
                .green()
                .bold()
                .to_string()
        } else {
            format!("门禁未通过（要求 ≥ {:.0}%，低于阈值）", suite.fail_under * 100.0)
                .red()
                .bold()
                .to_string()
        };
        println!("{verdict}");
    }
    Ok(summary)
}

fn token_suffix(tokens: Option<u64>) -> String {
    tokens
        .map(|t| format!("{} tokens", t.to_string().cyan().dimmed()))
        .unwrap_or_default()
}

fn preview_line(s: &str) -> String {
    let one = s.lines().next().unwrap_or(s);
    let one = if one.chars().count() > 100 {
        let t: String = one.chars().take(100).collect();
        format!("{t}…")
    } else {
        one.to_string()
    };
    format!("  ↳ {}", one.dimmed())
}

fn print_dry_run(name: &str, messages: &[ChatMessage]) {
    println!("{} {}", "── 用例:".bright_cyan().bold(), name.bright_yellow().bold());
    for m in messages {
        let role = m.role.as_str();
        let tag = if role == "system" {
            "SYSTEM".cyan().bold()
        } else {
            "USER".green().bold()
        };
        println!("  [{}]", tag);
        for line in m.content.lines().take(12) {
            println!("    {}", line.dimmed());
        }
        if m.content.lines().count() > 12 {
            println!("    …（剩余 {} 行省略）", m.content.lines().count() - 12);
        }
    }
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suite_parses_sample() {
        let text = r#"
backend:
  provider: deepseek
  model: deepseek-chat
judge: llm
fail_under: 0.8
cases:
  - name: a
    vars: { input: "hi" }
    enabled: false
  - name: b
    vars: { input: "hello" }
    expect_contains: [world]
"#;
        let s = load_suite(text).unwrap();
        assert_eq!(s.backend.provider, "deepseek");
        assert_eq!(s.judge, "llm");
        assert_eq!(s.cases.len(), 2);
        assert!(!s.cases[0].enabled);
        assert!(s.cases[1].enabled);
    }

    #[test]
    fn suite_defaults_when_minimal() {
        let s = load_suite("cases:\n  - name: x\n").unwrap();
        assert_eq!(s.backend.model, "gpt-4o-mini");
        assert_eq!(s.judge, "keyword");
        assert_eq!(s.fail_under, 1.0);
    }

    #[test]
    fn invalid_judge_rejected() {
        let text = "judge: fuzzy\ncases: []\n";
        assert!(load_suite(text).is_err());
    }

    #[test]
    fn invalid_fail_under_rejected() {
        assert!(load_suite("fail_under: 1.5\ncases: []\n").is_err());
        assert!(load_suite("fail_under: -0.1\ncases: []\n").is_err());
    }
}