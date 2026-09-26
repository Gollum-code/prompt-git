use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use colored::Colorize;

use prompt_git::compare::compare_versions;
use prompt_git::diff::{build_file_diffs, render_diffs};
use prompt_git::store;
use prompt_git::template::{self, PromptSet, PROMPTS_DIR};
use prompt_git::test::run_gate;

#[derive(Parser, Debug)]
#[command(
    name = "prompt-git",
    version,
    about = "把提示词当代码管理：git 存储 + 结构化 diff + 每次变更自动跑评测门禁",
    long_about = "prompt-git — 提示词的版本控制工具。\n\n\
常用流程：\n  \
prompt-git init                 # 创建 prompts/ 并纳入 git\n  \
prompt-git commit -m \"优化系统提示\"   # 提交一次提示词变更\n  \
prompt-git diff                  # 结构化查看与上一版的差异（按块高亮）\n  \
prompt-git test                  # 变更后自动跑评测门禁\n  \
prompt-git compare v1 v2         # 同输入下对比两版输出"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// 初始化：创建 prompts/ 目录并纳入 git 管理
    Init(InitArgs),
    /// 查看当前提示词状态（未提交变更、最近提交）
    Status,
    /// 提交一次提示词变更（基于 git）
    Commit(CommitArgs),
    /// 查看提示词版本历史
    Log(LogArgs),
    /// 回滚提示词到某个版本（commit / tag / 分支）
    Checkout(CheckoutArgs),
    /// 结构化查看提示词变更（默认与 HEAD 比较）
    Diff(DiffArgs),
    /// 渲染当前提示词（不调用模型），用于调试变量
    Render(RenderArgs),
    /// 变更后跑评测门禁：通过率低于 fail_under 时非零退出
    Test(TestArgs),
    /// 同一输入下对比两个版本的输出
    Compare(CompareArgs),
}

#[derive(Args, Debug)]
struct InitArgs {
    /// 覆盖已存在的默认文件
    #[arg(long)]
    force: bool,
}

#[derive(Args, Debug)]
struct CommitArgs {
    /// 提交信息
    #[arg(short, long)]
    message: String,
}

#[derive(Args, Debug)]
struct LogArgs {
    /// 显示条数
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,
    /// 精简输出（仅 hash + 标题）
    #[arg(long)]
    oneline: bool,
}

#[derive(Args, Debug)]
struct CheckoutArgs {
    /// 版本（commit hash / tag / 分支 / HEAD~1）
    version: String,
    /// 可选：只恢复某个文件（如 system.md）
    path: Option<String>,
}

#[derive(Args, Debug)]
struct DiffArgs {
    /// 与哪个版本比较（默认 HEAD）
    version: Option<String>,
    /// 关闭颜色
    #[arg(long)]
    no_color: bool,
}

#[derive(Args, Debug)]
struct RenderArgs {
    /// 用户输入（注入 {input} 变量）
    #[arg(long)]
    input: Option<String>,
    /// 覆盖变量，格式 key=value（可多次）
    #[arg(long = "var", value_name = "K=V")]
    vars: Vec<String>,
}

#[derive(Args, Debug)]
struct TestArgs {
    /// 按用例名子串过滤
    #[arg(long)]
    filter: Option<String>,
    /// 只渲染请求，不调用模型（无需 API Key）
    #[arg(long)]
    dry_run: bool,
    /// 关闭颜色
    #[arg(long)]
    no_color: bool,
}

#[derive(Args, Debug)]
struct CompareArgs {
    /// 旧版本
    v1: String,
    /// 新版本
    v2: String,
    /// 用户输入（注入 {input} 变量）
    #[arg(long)]
    input: Option<String>,
    /// 覆盖变量，格式 key=value（可多次）
    #[arg(long = "var", value_name = "K=V")]
    vars: Vec<String>,
    /// 只渲染两版提示，不调用模型（无需 API Key）
    #[arg(long)]
    dry_run: bool,
    /// 关闭颜色
    #[arg(long)]
    no_color: bool,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let root = resolve_root();
    match run(cli, &root) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("{} {:#}", "错误:".red().bold(), err);
            ExitCode::from(1)
        }
    }
}

fn resolve_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    store::find_project_root(&cwd).unwrap_or(cwd)
}

fn run(cli: Cli, root: &Path) -> Result<ExitCode> {
    match cli.cmd {
        Cmd::Init(args) => cmd_init(root, args),
        Cmd::Status => cmd_status(root),
        Cmd::Commit(args) => cmd_commit(root, args),
        Cmd::Log(args) => cmd_log(root, args),
        Cmd::Checkout(args) => cmd_checkout(root, args),
        Cmd::Diff(args) => cmd_diff(root, args),
        Cmd::Render(args) => cmd_render(root, args),
        Cmd::Test(args) => cmd_test(root, args),
        Cmd::Compare(args) => cmd_compare(root, args),
    }
}

// ---------- commands ----------

fn cmd_init(root: &Path, args: InitArgs) -> Result<ExitCode> {
    let created = template::scaffold(root, args.force)?;
    store::git_init(root)?;
    println!();
    println!("{} {}", "已创建提示词项目：".green().bold(), root.display().to_string().bright_white().bold());
    for p in &created {
        println!("  + {}", p.strip_prefix(root).unwrap_or(p).display().to_string().bright_cyan());
    }
    if created.is_empty() {
        println!("  (文件已存在，未改动)");
    }
    println!();
    println!("下一步：");
    println!("  1. 编辑 {}", format!("{PROMPTS_DIR}/system.md 与 user.md").bright_white().bold());
    println!("  2. {}", "prompt-git commit -m \"初始化提示词\"".bright_white().bold());
    println!("  3. {}", "prompt-git diff   # 看结构化 diff".bright_white().bold());
    Ok(ExitCode::SUCCESS)
}

fn cmd_status(root: &Path) -> Result<ExitCode> {
    if !store::is_repo(root) {
        println!("当前目录还不是 git 仓库。运行 {} 开始管理提示词。", "prompt-git init".bright_cyan().bold());
        return Ok(ExitCode::SUCCESS);
    }
    if let Some(c) = store::last_commit(root) {
        println!("最近提交: {} {}  {}  {}", c.short.bright_yellow().bold(), c.date.dimmed(), c.author.dimmed(), c.subject.bright_white());
    } else {
        println!("{} 尚无提交记录", "提示:".yellow());
    }
    let changes = store::status(root)?;
    if changes.is_empty() {
        println!("工作区: {} prompts/ 无未提交变更", "干净".green().bold());
    } else {
        println!("未提交变更:");
        for line in changes {
            println!("  {}", line);
        }
        println!("运行 {} 查看结构化 diff", "prompt-git diff".bright_cyan().bold());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_commit(root: &Path, args: CommitArgs) -> Result<ExitCode> {
    if !store::is_repo(root) {
        bail!("当前目录还不是 git 仓库，请先运行 `prompt-git init`");
    }
    store::commit(root, &args.message)?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_log(root: &Path, args: LogArgs) -> Result<ExitCode> {
    if !store::is_repo(root) {
        bail!("当前目录还不是 git 仓库，请先运行 `prompt-git init`");
    }
    let commits = store::log(root, args.limit)?;
    if commits.is_empty() {
        println!("{} 还没有 prompts/ 的提交历史", "提示:".yellow());
        return Ok(ExitCode::SUCCESS);
    }
    for c in commits {
        if args.oneline {
            println!("{} {}", c.short.bright_yellow().bold(), c.subject);
        } else {
            println!("{} {}  {}  {}", c.short.bright_yellow().bold(), c.date.dimmed(), c.author.dimmed(), c.subject.bright_white());
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_checkout(root: &Path, args: CheckoutArgs) -> Result<ExitCode> {
    if !store::is_repo(root) {
        bail!("当前目录还不是 git 仓库，请先运行 `prompt-git init`");
    }
    store::check_rev(root, &args.version)?;
    store::restore(root, &args.version, args.path.as_deref().map(Path::new))?;
    Ok(ExitCode::SUCCESS)
}

fn cmd_diff(root: &Path, args: DiffArgs) -> Result<ExitCode> {
    if args.no_color {
        colored::control::set_override(false);
    }
    if !store::is_repo(root) {
        bail!("当前目录还不是 git 仓库，请先运行 `prompt-git init`");
    }
    let version: Option<String> = match args.version {
        Some(v) => {
            store::check_rev(root, &v)?;
            Some(v)
        }
        None => {
            if store::has_commits(root) {
                Some("HEAD".to_string())
            } else {
                None
            }
        }
    };
    let files = store::collect_files(root, version.as_deref())?;
    let diffs = build_file_diffs(&files);
    if diffs.is_empty() {
        println!("{} 提示词无变更", "✔".green().bold());
        return Ok(ExitCode::SUCCESS);
    }
    let base = version.unwrap_or_else(|| "(初次添加)".to_string());
    println!("{} {}  →  工作区\n", "对比基准:".bright_cyan().bold(), base.bright_yellow());
    print!("{}", render_diffs(&diffs, args.no_color));
    Ok(ExitCode::SUCCESS)
}

fn parse_overrides(pairs: &[String], input: Option<&str>) -> Result<BTreeMap<String, String>> {
    let mut m = BTreeMap::new();
    if let Some(i) = input {
        m.insert("input".to_string(), i.to_string());
    }
    for p in pairs {
        match p.split_once('=') {
            Some((k, v)) if !k.trim().is_empty() => {
                m.insert(k.trim().to_string(), v.to_string());
            }
            _ => bail!("--var 格式应为 key=value，当前: {p}"),
        }
    }
    Ok(m)
}

fn cmd_render(root: &Path, args: RenderArgs) -> Result<ExitCode> {
    let ps = PromptSet::load(root)?;
    let mut values = ps.default_values();
    values.extend(parse_overrides(&args.vars, args.input.as_deref())?);
    let (system, user, unresolved) = template::render_prompt_set(&ps, &values)?;
    template::warn_unresolved(&unresolved);
    if !system.trim().is_empty() {
        println!("[SYSTEM]");
        println!("{}", system);
    }
    println!("[USER]");
    println!("{}", user);
    Ok(ExitCode::SUCCESS)
}

fn cmd_test(root: &Path, args: TestArgs) -> Result<ExitCode> {
    if args.no_color {
        colored::control::set_override(false);
    }
    let summary = run_gate(root, args.filter.as_deref(), args.dry_run)?;
    if args.dry_run {
        return Ok(ExitCode::SUCCESS);
    }
    if summary.total == 0 {
        return Ok(ExitCode::SUCCESS);
    }
    if summary.gate_ok {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::from(1))
    }
}

fn cmd_compare(root: &Path, args: CompareArgs) -> Result<ExitCode> {
    if args.no_color {
        colored::control::set_override(false);
    }
    let overrides = parse_overrides(&args.vars, args.input.as_deref())?;
    compare_versions(root, &args.v1, &args.v2, &overrides, args.dry_run)?;
    Ok(ExitCode::SUCCESS)
}
