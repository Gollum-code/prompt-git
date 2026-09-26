//! 端到端集成测试：在临时目录里跑完整 git 工作流。

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use prompt_git::diff::{build_file_diffs, diff_words};
use prompt_git::eval;
use prompt_git::store;
use prompt_git::template;
use prompt_git::test;

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git 应可用");
    assert!(
        out.status.success(),
        "git {} 失败: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn setup_repo(dir: &Path) {
    run_git(dir, &["init"]);
    // 关闭 autocrlf，避免 Windows 上 checkout 时 LF→CRLF 造成内容不一致
    run_git(dir, &["config", "core.autocrlf", "false"]);
    run_git(dir, &["config", "user.email", "test@example.com"]);
    run_git(dir, &["config", "user.name", "测试"]);
}

fn write(dir: &Path, name: &str, content: &str) {
    let prompts = dir.join("prompts");
    std::fs::create_dir_all(&prompts).unwrap();
    std::fs::write(prompts.join(name), content).unwrap();
}

fn read(dir: &Path, name: &str) -> String {
    std::fs::read_to_string(dir.join("prompts").join(name)).unwrap()
}

#[test]
fn full_workflow_init_commit_diff_log_checkout() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    setup_repo(dir);

    // init：创建四个默认文件
    let created = template::scaffold(dir, false).unwrap();
    assert_eq!(created.len(), 4);
    assert!(dir.join("prompts").join("system.md").is_file());
    assert!(dir.join("prompts").join("vars.yaml").is_file());
    assert!(dir.join("prompts").join("tests.yaml").is_file());

    // 首次提交
    assert!(store::commit(dir, "初始化提示词").unwrap());

    // 修改 system.md
    let original = read(dir, "system.md");
    write(
        dir,
        "system.md",
        &format!("{original}新增一行要求：必须给出示例。\n"),
    );

    // diff 与 HEAD 对比应检测到 system.md 有插入
    let files = store::collect_files(dir, Some("HEAD")).unwrap();
    let diffs = build_file_diffs(&files);
    let sys_diff = diffs.iter().find(|d| d.name == "system.md").unwrap();
    assert!(sys_diff
        .edits
        .iter()
        .any(|e| e.op == prompt_git::diff::Op::Insert));
    let rendered = prompt_git::diff::render_diffs(&diffs, true);
    assert!(rendered.contains("system.md"));
    assert!(rendered.contains("新增一行要求"));

    // 二次提交 + 历史
    assert!(store::commit(dir, "强化 system 提示").unwrap());
    let log = store::log(dir, 10).unwrap();
    assert!(log.len() >= 2);
    assert_eq!(log[0].subject, "强化 system 提示");

    // 回滚到首个提交
    store::restore(dir, &log[1].short, None).unwrap();
    assert_eq!(read(dir, "system.md"), original);
}

#[test]
fn diff_pairs_and_word_highlight() {
    let (old, new) = diff_words("使用 Python 实现", "使用 Rust 实现");
    assert!(old.iter().any(|(_, c)| *c));
    assert!(new.iter().any(|(_, c)| *c));
}

#[test]
fn render_injects_vars_and_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    template::scaffold(dir, false).unwrap();

    let ps = template::PromptSet::load(dir).unwrap();
    assert!(ps.default_values().contains_key("role"));
    assert_eq!(ps.default_values()["language"], "中文");

    let mut values = ps.default_values();
    values.insert("input".to_string(), "写一首诗".to_string());
    let (system, user, unresolved) = template::render_prompt_set(&ps, &values).unwrap();
    assert!(system.contains("资深 AI 助手"));
    assert!(user.contains("写一首诗"));
    assert!(unresolved.is_empty());
}

#[test]
fn test_gate_dry_run_skips_all() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    template::scaffold(dir, false).unwrap();

    let summary = test::run_gate(dir, None, true).unwrap();
    assert!(summary.total > 0);
    assert_eq!(summary.skipped, summary.total);
    assert_eq!(summary.passed, 0);
}

#[test]
fn test_gate_without_key_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    template::scaffold(dir, false).unwrap();

    // 移除可能存在的 key
    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("DEEPSEEK_API_KEY");
    let err = test::run_gate(dir, None, false).unwrap_err();
    assert!(err.to_string().contains("API Key"), "{err:#}");
}

#[test]
fn compare_dry_run_two_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    setup_repo(dir);
    template::scaffold(dir, false).unwrap();
    store::commit(dir, "v1").unwrap();

    write(dir, "user.md", "旧版提示：{input}\n");
    store::commit(dir, "v2").unwrap();

    let log = store::log(dir, 10).unwrap();
    let overrides = BTreeMap::new();
    prompt_git::compare::compare_versions(dir, &log[1].short, &log[0].short, &overrides, true)
        .unwrap();
}

#[test]
fn resolve_backend_picks_provider_env() {
    std::env::set_var("OPENAI_API_KEY", "sk-test");
    let cfg = eval::BackendConfig {
        provider: "openai".into(),
        model: "gpt-4o-mini".into(),
        ..Default::default()
    };
    let resolved = eval::resolve_backend(&cfg).unwrap();
    assert_eq!(resolved.base_url, "https://api.openai.com/v1");
    assert_eq!(resolved.api_key, "sk-test");

    std::env::set_var("DEEPSEEK_API_KEY", "sk-deep");
    let cfg2 = eval::BackendConfig {
        provider: "deepseek".into(),
        model: "deepseek-chat".into(),
        ..Default::default()
    };
    let resolved2 = eval::resolve_backend(&cfg2).unwrap();
    assert_eq!(resolved2.base_url, "https://api.deepseek.com/v1");

    std::env::remove_var("OPENAI_API_KEY");
    std::env::remove_var("DEEPSEEK_API_KEY");
}

#[test]
fn custom_provider_requires_base_url() {
    let cfg = eval::BackendConfig {
        provider: "custom".into(),
        model: "m".into(),
        ..Default::default()
    };
    assert!(eval::resolve_backend(&cfg).is_err());
}
