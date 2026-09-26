//! git 接入：初始化、暂存/提交、日志、恢复、按版本读取文件。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::template::{read_worktree_file, worktree_files, PROMPTS_DIR};

fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("无法执行 git，请先安装 git 并加入 PATH")?;
    if !out.status.success() {
        bail!(
            "git {} 失败: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// 当前目录是否是 git 仓库工作区。
pub fn is_repo(root: &Path) -> bool {
    run_git(root, &["rev-parse", "--is-inside-work-tree"])
        .map(|o| o.trim() == "true")
        .unwrap_or(false)
}

/// 在 root 初始化 git 仓库。
pub fn git_init(root: &Path) -> Result<()> {
    if is_repo(root) {
        eprintln!("已是 git 仓库，跳过 git init");
    } else {
        run_git(root, &["init"])?;
    }
    Ok(())
}

/// 向上查找包含 prompts/ 目录的项目根。
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let mut cur = Some(start.to_path_buf());
    while let Some(d) = cur {
        if d.join(PROMPTS_DIR).is_dir() {
            return Some(d);
        }
        cur = d.parent().map(|p| p.to_path_buf());
    }
    None
}

/// 是否有至少一个提交。
pub fn has_commits(root: &Path) -> bool {
    run_git(root, &["rev-parse", "--verify", "--quiet", "HEAD"])
        .map(|_| true)
        .unwrap_or(false)
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub short: String,
    pub author: String,
    pub date: String,
    pub subject: String,
}

/// 查看 prompts/ 的 git 状态（--short 格式）。
pub fn status(root: &Path) -> Result<Vec<String>> {
    let out = run_git(root, &["status", "--short", "--", "prompts"])?;
    Ok(out.lines().map(|l| l.to_string()).collect())
}

/// 最近一次提交信息（若有）。
pub fn last_commit(root: &Path) -> Option<CommitInfo> {
    log(root, 1).ok().and_then(|mut v| v.pop())
}

/// 提交 prompts/ 目录。无变更时返回 created=false。
pub fn commit(root: &Path, message: &str) -> Result<bool> {
    run_git(root, &["add", "--", "prompts"])?;
    let status = run_git(root, &["status", "--porcelain", "--", "prompts"])?;
    if status.trim().is_empty() {
        eprintln!("没有需要提交的变更");
        return Ok(false);
    }
    let out = run_git(root, &["commit", "-m", message])?;
    let short = run_git(root, &["rev-parse", "--short", "HEAD"])?;
    println!("{}", out.trim());
    println!("已提交: {}", short.trim());
    Ok(true)
}

/// 查看 prompts/ 目录的提交历史。
pub fn log(root: &Path, limit: usize) -> Result<Vec<CommitInfo>> {
    let out = run_git(
        root,
        &[
            "log",
            "--pretty=format:%h\u{1f}%an\u{1f}%ad\u{1f}%s",
            "--date=short",
            &format!("-n{limit}"),
            "--",
            "prompts",
        ],
    )?;
    let mut commits = Vec::new();
    for line in out.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split('\u{1f}').collect();
        if parts.len() >= 4 {
            commits.push(CommitInfo {
                short: parts[0].to_string(),
                author: parts[1].to_string(),
                date: parts[2].to_string(),
                subject: parts[3].to_string(),
            });
        }
    }
    Ok(commits)
}

/// 把某个版本（commit/tag/branch）的 prompts/ 恢复到工作区。
pub fn restore(root: &Path, rev: &str, path: Option<&Path>) -> Result<()> {
    match path {
        Some(p) => {
            let rel = format!("{PROMPTS_DIR}/{}", p.display());
            run_git(root, &["checkout", rev, "--", &rel])?;
            println!("已从 {rev} 恢复 {rel}（变更已暂存）");
        }
        None => {
            run_git(root, &["checkout", rev, "--", PROMPTS_DIR])?;
            println!(
                "已从 {rev} 恢复整个 prompts/（变更已暂存）。如需提交：prompt-git commit -m \"回滚到 {rev}\""
            );
        }
    }
    Ok(())
}

/// 列出某版本下 prompts/ 内的文件（相对路径，如 prompts/system.md）。
pub fn tree_files(root: &Path, rev: &str) -> Result<Vec<String>> {
    let out = run_git(root, &["ls-tree", "-r", "--name-only", rev])?;
    let mut files = Vec::new();
    for line in out.lines() {
        let name = line.trim();
        if name.starts_with(PROMPTS_DIR) {
            files.push(name.to_string());
        }
    }
    Ok(files)
}

/// 读取某版本下的文件内容。文件不存在返回 Ok(None)；版本不存在返回 Err。
pub fn show_file(root: &Path, rev: &str, rel_path: &str) -> Result<Option<String>> {
    let rel = rel_path.replace('\\', "/");
    let files = tree_files(root, rev)?;
    if !files.iter().any(|f| f == &rel) {
        return Ok(None);
    }
    let out = run_git(root, &["show", &format!("{rev}:{rel}")])?;
    Ok(Some(out))
}

/// 版本号是否有效（commit/tag/branch 均可）。
pub fn valid_rev(root: &Path, rev: &str) -> bool {
    run_git(root, &["rev-parse", "--verify", "--quiet", rev]).is_ok()
}

/// 校验版本号，无效时报错并给出提示。
pub fn check_rev(root: &Path, rev: &str) -> Result<()> {
    if !valid_rev(root, rev) {
        bail!("无效的版本号: {rev}（可用 commit hash / tag / 分支名，如 HEAD~1）");
    }
    Ok(())
}

/// 收集工作区与某版本（rev=None 表示只用工作区）的 prompts 文件。
/// 返回 (文件名, 版本侧内容, 工作区侧内容)，两者至少有一侧非空。
pub fn collect_files(
    root: &Path,
    rev: Option<&str>,
) -> Result<Vec<(String, String, String)>> {
    let mut names: Vec<String> = Vec::new();

    if let Some(r) = rev {
        for f in tree_files(root, r)? {
            let name = f.strip_prefix(PROMPTS_DIR).unwrap_or(&f).to_string();
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    for name in worktree_files(root)? {
        if !names.contains(&name) {
            names.push(name);
        }
    }

    let mut out = Vec::new();
    for name in names {
        let old = match rev {
            Some(r) => show_file(root, r, &format!("{PROMPTS_DIR}/{name}"))?.unwrap_or_default(),
            None => String::new(),
        };
        let new = read_worktree_file(root, &name)?;
        out.push((name, old, new));
    }

    // 按模板约定的顺序排序：system / user / vars / tests，其余按字典序
    out.sort_by_key(|(name, _, _)| order_key(name));
    Ok(out)
}

fn order_key(name: &str) -> (usize, String) {
    let rank = match name {
        "system.md" => 0,
        "user.md" => 1,
        "vars.yaml" => 2,
        "tests.yaml" => 3,
        _ => 4,
    };
    (rank, name.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn split_commit_line_format() {
        let line = "abc123\u{1f}作者\u{1f}2026-09-26\u{1f}优化 system 提示";
        let parts: Vec<&str> = line.split('\u{1f}').collect();
        assert_eq!(parts.len(), 4);
    }
}
