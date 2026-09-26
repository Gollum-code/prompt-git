//! `prompt-git compare v1 v2`：同输入下两版 prompt 的输出对比。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use colored::Colorize;

use crate::diff::similarity_ratio;
use crate::eval::{self, ChatMessage, ResolvedBackend};
use crate::store;
use crate::template::{self, PromptSet};
use crate::test::{load_suite, load_suite_from_disk};

/// 确定 input 变量集合：命令行覆盖 > 首个测试用例 vars > vars.yaml 默认值。
fn resolve_input_values(
    root: &Path,
    ps: &PromptSet,
    overrides: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut values = ps.default_values();

    // 用例变量：优先取工作区 tests.yaml，其次 v2 版本中的 tests.yaml
    let suite = load_suite_from_disk(root).ok().or_else(|| {
        store::show_file(root, "HEAD", "prompts/tests.yaml")
            .ok()
            .flatten()
            .as_deref()
            .and_then(|t| load_suite(t).ok())
    });
    if let Some(s) = suite {
        if let Some(first) = s.cases.first() {
            values.extend(first.vars.clone());
        }
    }

    values.extend(overrides.clone());
    values
}

/// 打印一侧的渲染结果/输出。
fn print_side(
    title: &str,
    subtitle: &str,
    messages_opt: Option<&[ChatMessage]>,
    output_opt: Option<&str>,
) {
    println!("{}", "━━━".bright_cyan());
    println!(
        "{} {} {}",
        "■■".bright_cyan().bold(),
        title.bright_yellow().bold(),
        subtitle.dimmed()
    );
    if let Some(msgs) = messages_opt {
        for m in msgs {
            let tag = if m.role == "system" {
                "SYSTEM".cyan().bold()
            } else {
                "USER".green().bold()
            };
            println!("  [{}] {}", tag, m.content.dimmed());
        }
    }
    if let Some(out) = output_opt {
        println!("  {}", "输出:".bright_white().bold());
        for line in out.lines() {
            println!("  {}", line);
        }
    }
    println!();
}

pub fn compare_versions(
    root: &Path,
    v1: &str,
    v2: &str,
    overrides: &BTreeMap<String, String>,
    dry_run: bool,
) -> Result<()> {
    store::check_rev(root, v1)?;
    store::check_rev(root, v2)?;

    let ps1 = PromptSet::load_at(root, v1)?;
    let ps2 = PromptSet::load_at(root, v2)?;

    let values = resolve_input_values(root, &ps2, overrides);

    let (s1, u1, un1) = template::render_prompt_set(&ps1, &values)?;
    let (s2, u2, un2) = template::render_prompt_set(&ps2, &values)?;
    for (tag, list) in [("v1", &un1), ("v2", &un2)] {
        if !list.is_empty() {
            eprintln!(
                "{} {tag} 存在未提供变量: {}",
                "警告:".yellow(),
                list.join(", ")
            );
        }
    }

    let msgs1 = template::build_messages(&s1, &u1);
    let msgs2 = template::build_messages(&s2, &u2);

    if dry_run {
        println!(
            "{} 渲染结果如下（dry-run，未调用模型）\n",
            "等同输入:".bright_cyan().bold()
        );
        print_side(v1, "", Some(&msgs1), None);
        print_side(v2, "", Some(&msgs2), None);
        return Ok(());
    }

    let suite = load_suite_from_disk(root).unwrap_or_default();
    let backend: ResolvedBackend = eval::resolve_backend(&suite.backend)?;

    println!(
        "{}  相同输入（以下变量）下对比 {v1} 与 {v2} 的输出\n",
        "◆".bright_cyan().bold()
    );
    let mut show_input = true;
    if let Some(input) = values.get("input") {
        if !input.trim().is_empty() {
            println!("  input: {}", input.bright_cyan());
            show_input = false;
        }
    }
    if show_input {
        for (k, v) in &values {
            if !v.is_empty() {
                println!("  {k}: {}", v.dimmed());
            }
        }
    }
    println!();

    let r1 = eval::chat(&backend, &msgs1)?;
    let r2 = eval::chat(&backend, &msgs2)?;

    print_side(
        v1,
        &format!("（{} 毫秒）", r1.latency.as_millis()),
        None,
        Some(&r1.text),
    );
    print_side(
        v2,
        &format!("（{} 毫秒）", r2.latency.as_millis()),
        None,
        Some(&r2.text),
    );

    let ratio = similarity_ratio(&r1.text, &r2.text);
    let similar = if ratio > 0.8 {
        "输出高度相似".to_string()
    } else if ratio > 0.5 {
        "输出部分相似".to_string()
    } else {
        "输出差异较大".to_string()
    };
    println!(
        "{}  {:.0}%   {}",
        "输出相似度:".bright_yellow().bold(),
        ratio * 100.0,
        similar.dimmed(),
    );
    println!(
        "{}",
        "提示：如需把评测门禁跑在两版上，可先 checkout 到某版本再运行 `prompt-git test`".dimmed(),
    );

    Ok(())
}
