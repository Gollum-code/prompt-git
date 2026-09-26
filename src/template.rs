//! 模板与变量：加载 prompts/ 目录、变量插值渲染。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::store;

pub const PROMPTS_DIR: &str = "prompts";

/// 目录约定的四个文件。
pub const DEFAULT_FILES: [&str; 4] = ["system.md", "user.md", "vars.yaml", "tests.yaml"];

/// 变量定义文件（vars.yaml）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VarsFile {
    #[serde(default)]
    pub variables: BTreeMap<String, VarDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarDef {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl VarDef {
    pub fn default_or_empty(&self) -> String {
        self.default.clone().unwrap_or_default()
    }
}

/// 一次加载的完整 prompt 集。
#[derive(Debug, Clone)]
pub struct PromptSet {
    pub root: PathBuf,
    pub system: String,
    pub user: String,
    pub vars: VarsFile,
}

impl PromptSet {
    /// 从工作区加载（文件缺失视为空/默认）。
    pub fn load(root: &Path) -> Result<PromptSet> {
        let prompts = root.join(PROMPTS_DIR);
        if !prompts.is_dir() {
            bail!(
                "未找到 {} 目录，请先在项目根目录运行 `prompt-git init` 或创建 prompts/",
                prompts.display()
            );
        }
        Self::load_impl(root, None)
    }

    /// 从某个 git 版本加载（`git show <rev>:<path>`）。
    pub fn load_at(root: &Path, rev: &str) -> Result<PromptSet> {
        Self::load_impl(root, Some(rev))
    }

    fn load_impl(root: &Path, rev: Option<&str>) -> Result<PromptSet> {
        let read = |rel: &str| -> Result<Option<String>> {
            match rev {
                Some(r) => store::show_file(root, r, &format!("{PROMPTS_DIR}/{rel}")),
                None => {
                    let p = root.join(PROMPTS_DIR).join(rel);
                    if p.is_file() {
                        Ok(Some(
                            fs::read_to_string(&p)
                                .with_context(|| format!("读取 {} 失败", p.display()))?,
                        ))
                    } else {
                        Ok(None)
                    }
                }
            }
        };

        let system = read("system.md")?.unwrap_or_default();
        let user = read("user.md")?.unwrap_or_default();
        let vars = match read("vars.yaml")? {
            Some(text) => parse_vars(&text)?,
            None => VarsFile::default(),
        };
        Ok(PromptSet {
            root: root.to_path_buf(),
            system,
            user,
            vars,
        })
    }

    /// vars.yaml 默认值（CLI 注入前）。
    pub fn default_values(&self) -> BTreeMap<String, String> {
        self.vars
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), v.default_or_empty()))
            .collect()
    }
}

pub fn parse_vars(text: &str) -> Result<VarsFile> {
    serde_yaml::from_str(text).context("解析 vars.yaml 失败")
}

/// 渲染模板：`{name}` 插值，`{{` 转义为字面 `{`。
/// 未提供的占位符保持原样并记录名字。
pub fn render_template(template: &str, values: &BTreeMap<String, String>) -> (String, Vec<String>) {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::with_capacity(template.len());
    let mut unresolved = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '}' && i + 1 < chars.len() && chars[i + 1] == '}' {
            // }} 转义为字面 }
            out.push('}');
            i += 2;
            continue;
        }
        if c == '{' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                out.push('{');
                i += 2;
                continue;
            }
            // 读取变量名：第一个字符要求字母或下划线，后续允许字母/数字/下划线/点
            let name_start = i + 1;
            let first_ok = chars[name_start].is_ascii_alphabetic() || chars[name_start] == '_';
            let mut j = name_start;
            if first_ok {
                let mut jj = j + 1;
                while jj < chars.len() {
                    let cc = chars[jj];
                    if cc.is_ascii_alphanumeric() || cc == '_' || cc == '.' {
                        jj += 1;
                    } else {
                        break;
                    }
                }
                j = jj;
            }
            if first_ok && j < chars.len() && chars[j] == '}' {
                let name: String = chars[name_start..j].iter().collect();
                if let Some(v) = values.get(&name) {
                    out.push_str(v);
                } else {
                    out.push('{');
                    out.push_str(&name);
                    out.push('}');
                    if !unresolved.contains(&name) {
                        unresolved.push(name);
                    }
                }
                i = j + 1;
                continue;
            }
            // 不是变量，按字面输出
            out.push(c);
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    (out, unresolved)
}

/// 组装 chat messages：[system(如有), user]。
pub fn build_messages(system: &str, user: &str) -> Vec<crate::eval::ChatMessage> {
    let mut messages = Vec::new();
    if !system.trim().is_empty() {
        messages.push(crate::eval::ChatMessage {
            role: "system".to_string(),
            content: system.to_string(),
        });
    }
    messages.push(crate::eval::ChatMessage {
        role: "user".to_string(),
        content: user.to_string(),
    });
    messages
}

/// 用变量集渲染 system + user，返回渲染后的内容。
pub fn render_prompt_set(
    ps: &PromptSet,
    values: &BTreeMap<String, String>,
) -> Result<(String, String, Vec<String>)> {
    let (system, u1) = render_template(&ps.system, values);
    let (user, u2) = render_template(&ps.user, values);
    let mut unresolved = u1;
    unresolved.extend(u2);
    unresolved.dedup();
    Ok((system, user, unresolved))
}

/// 打印未解析占位符警告。
pub fn warn_unresolved(unresolved: &[String]) {
    for name in unresolved {
        eprintln!("警告: 模板中存在未提供的变量 {{{name}}}，已原样保留");
    }
}

// ---------- init / scaffold ----------

pub const DEFAULT_SYSTEM_MD: &str =
    "你是 {role}。\n请始终使用 {language} 回答，语气保持 {tone}。\n";

pub const DEFAULT_USER_MD: &str =
    "用户的问题是：\n\n{input}\n\n请根据以上问题，给出清晰、完整的回答。\n";

pub const DEFAULT_VARS_YAML: &str = r#"# 变量定义与默认值：模板中通过 {name} 插值。
variables:
  role:
    default: "一位资深 AI 助手"
    description: "系统提示中扮演的角色"
  language:
    default: "中文"
    description: "回答使用的语言"
  tone:
    default: "专业"
    description: "回答的语气"
  input:
    default: ""
    description: "用户输入（通常由测试用例或命令行 --input 注入）"
"#;

pub const DEFAULT_TESTS_YAML: &str = r#"# 评测门禁：每次 prompt 变更后运行 `prompt-git test`。
# backend 决定调用哪个模型；judge 决定判定方式。
backend:
  provider: openai          # openai | deepseek | custom（OpenAI 兼容接口）
  model: gpt-4o-mini
  temperature: 0.2
  max_tokens: 512
  base_url: ""              # custom 时必填，例如 https://api.moonshot.cn/v1
  api_key_env: ""           # 自定义 key 的环境变量名，留空则用 PROMPT_GIT_API_KEY

judge: keyword              # keyword：离线关键词判定（默认，不消耗 token）；llm：调用模型判定
fail_under: 1.0             # 通过率门禁：低于该值 `prompt-git test` 以非零码退出

cases:
  - name: 礼貌回复
    vars:
      input: 你好，请介绍一下你自己。
      language: 中文
    expect_contains: [你好]
    expect_not_contains: [无法, 抱歉]

  - name: 代码示例
    vars:
      input: 用 Python 写一个计算斐波那契数列的函数，并解释。
    expect_contains: [def, return]
"#;

pub fn default_files() -> Vec<(String, &'static str)> {
    vec![
        ("system.md".to_string(), DEFAULT_SYSTEM_MD),
        ("user.md".to_string(), DEFAULT_USER_MD),
        ("vars.yaml".to_string(), DEFAULT_VARS_YAML),
        ("tests.yaml".to_string(), DEFAULT_TESTS_YAML),
    ]
}

/// 创建 prompts/ 目录与四个默认文件。`force=true` 覆盖已有文件。
pub fn scaffold(root: &Path, force: bool) -> Result<Vec<PathBuf>> {
    let prompts = root.join(PROMPTS_DIR);
    fs::create_dir_all(&prompts).with_context(|| format!("创建 {} 失败", prompts.display()))?;
    let mut created = Vec::new();
    for (name, content) in default_files() {
        let p = prompts.join(name);
        if p.exists() && !force {
            eprintln!("已存在 {}，跳过（--force 可覆盖）", p.display());
            continue;
        }
        fs::write(&p, content).with_context(|| format!("写入 {} 失败", p.display()))?;
        created.push(p);
    }
    Ok(created)
}

/// 列出工作区 prompts/ 下实际存在的文件。
pub fn worktree_files(root: &Path) -> Result<Vec<String>> {
    let prompts = root.join(PROMPTS_DIR);
    let mut out = Vec::new();
    for entry in
        fs::read_dir(&prompts).with_context(|| format!("读取 {} 失败", prompts.display()))?
    {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                out.push(name);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// 读取工作区某文件内容（不存在返回空字符串）。
pub fn read_worktree_file(root: &Path, name: &str) -> Result<String> {
    let p = root.join(PROMPTS_DIR).join(name);
    if p.is_file() {
        Ok(fs::read_to_string(&p).with_context(|| format!("读取 {} 失败", p.display()))?)
    } else {
        Ok(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn render_substitutes_and_keeps_unknown() {
        let mut v = BTreeMap::new();
        v.insert("input".to_string(), "你好".to_string());
        let (out, unresolved) = render_template("问题：{input}；{missing}；{{ok}}", &v);
        assert_eq!(out, "问题：你好；{missing}；{ok}");
        assert_eq!(unresolved, vec!["missing"]);
    }

    #[test]
    fn render_handles_empty_and_edge() {
        let v = BTreeMap::new();
        let (out, unresolved) = render_template("", &v);
        assert_eq!(out, "");
        assert!(unresolved.is_empty());
        let (out2, _) = render_template("a{b}c", &BTreeMap::new());
        assert_eq!(out2, "a{b}c");
    }

    #[test]
    fn render_multiline_var() {
        let mut v = BTreeMap::new();
        v.insert("input".to_string(), "多\n行".to_string());
        let (out, _) = render_template("[{input}]", &v);
        assert_eq!(out, "[多\n行]");
    }
}
