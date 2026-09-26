//! 结构化 diff：Myers 行级 diff + 行内词级高亮，按 prompt 块组织输出。

use colored::Colorize;
use std::collections::BTreeMap;

/// 行级编辑操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Equal,
    Delete,
    Insert,
}

#[derive(Debug, Clone)]
pub struct Edit {
    pub op: Op,
    /// 旧文本中的行号（1-based），删除/相等时有效
    pub old: Option<usize>,
    /// 新文本中的行号（1-based），插入/相等时有效
    pub new: Option<usize>,
    pub text: String,
}

/// 两个字符串行列表的 diff（Myers 算法）。
pub fn diff_lines(a: &[String], b: &[String]) -> Vec<Edit> {
    let n = a.len() as isize;
    let m = b.len() as isize;

    if n == 0 {
        return b.iter()
            .enumerate()
            .map(|(i, t)| Edit { op: Op::Insert, old: None, new: Some(i + 1), text: t.clone() })
            .collect();
    }
    if m == 0 {
        return a.iter()
            .enumerate()
            .map(|(i, t)| Edit { op: Op::Delete, old: Some(i + 1), new: None, text: t.clone() })
            .collect();
    }

    let max = (n + m) as usize;
    let size = 2 * max + 1;
    let offset = max as isize;
    let mut v = vec![0isize; size];
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut finished = false;

    for d in 0..=max {
        trace.push(v.clone());
        let kmin = -(d as isize);
        let kmax = d as isize;
        let mut k = kmin;
        while k <= kmax {
            let x = if k == kmin || (k != kmax && v[(offset + k - 1) as usize] < v[(offset + k + 1) as usize])
            {
                v[(offset + k + 1) as usize]
            } else {
                v[(offset + k - 1) as usize] + 1
            };
            let mut x = x;
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(offset + k) as usize] = x;
            if x >= n && y >= m {
                finished = true;
                break;
            }
            k += 2;
        }
        if finished {
            break;
        }
    }

    // 回溯
    let mut ops: Vec<Edit> = Vec::new();
    let mut x = n;
    let mut y = m;
    for d in (0..trace.len()).rev() {
        let vv = &trace[d];
        let k = x - y;
        let kprev = if k == -(d as isize) || (k != d as isize && vv[(offset + k - 1) as usize] < vv[(offset + k + 1) as usize])
        {
            k + 1
        } else {
            k - 1
        };
        let xprev = vv[(offset + kprev) as usize];
        let yprev = xprev - kprev;
        while x > xprev && y > yprev {
            ops.push(Edit {
                op: Op::Equal,
                old: Some(x as usize),
                new: Some(y as usize),
                text: a[(x - 1) as usize].clone(),
            });
            x -= 1;
            y -= 1;
        }
        if d == 0 {
            break;
        }
        if x == xprev {
            ops.push(Edit {
                op: Op::Insert,
                old: None,
                new: Some(y as usize),
                text: b[(y - 1) as usize].clone(),
            });
            y -= 1;
        } else {
            ops.push(Edit {
                op: Op::Delete,
                old: Some(x as usize),
                new: None,
                text: a[(x - 1) as usize].clone(),
            });
            x -= 1;
        }
    }
    ops.reverse();
    ops
}

/// 把文本按空白切成 token（空白也保留为 token），用于词级 diff。
fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut buf = String::new();
    let mut in_ws = false;
    for c in text.chars() {
        let ws = c.is_whitespace();
        if !buf.is_empty() && ws != in_ws {
            tokens.push(std::mem::take(&mut buf));
        }
        buf.push(c);
        in_ws = ws;
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    tokens
}

/// 词级 diff 结果：每行各 token 是否变化。
pub type TokenDiffs = (Vec<(String, bool)>, Vec<(String, bool)>);

/// 行内词级 diff：返回两个 token 数组及每行各 token 是否变化。
pub fn diff_words(old_line: &str, new_line: &str) -> TokenDiffs {
    let a: Vec<String> = tokenize(old_line);
    let b: Vec<String> = tokenize(new_line);
    let edits = diff_lines(&a, &b);
    let mut old_out: Vec<(String, bool)> = Vec::new();
    let mut new_out: Vec<(String, bool)> = Vec::new();
    for e in &edits {
        match e.op {
            Op::Equal => {
                old_out.push((e.text.clone(), false));
                new_out.push((e.text.clone(), false));
            }
            Op::Delete => old_out.push((e.text.clone(), true)),
            Op::Insert => new_out.push((e.text.clone(), true)),
        }
    }
    (old_out, new_out)
}

/// 单个文件的结构化 diff。
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub name: String,
    pub label: &'static str,
    pub old_content: String,
    pub new_content: String,
    pub edits: Vec<Edit>,
}

pub fn block_label(name: &str) -> &'static str {
    match name {
        "system.md" => "系统提示",
        "user.md" => "用户提示模板",
        "vars.yaml" => "变量定义",
        "tests.yaml" => "测试用例",
        _ => "prompt 文件",
    }
}

fn pad(s: &str, w: usize) -> String {
    format!("{s:>w$}")
}

/// 归一化行：空串视为 0 行，去掉末尾换行与 \r（Windows CRLF）。
fn split_lines(content: &str) -> Vec<String> {
    if content.is_empty() {
        return Vec::new();
    }
    let c = content.strip_suffix('\n').unwrap_or(content);
    c.split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_string())
        .collect()
}

/// 生成一组文件的结构化 diff（跳过无变更的文件）。
pub fn build_file_diffs(files: &[(String, String, String)]) -> Vec<FileDiff> {
    let mut out = Vec::new();
    for (name, old, new) in files {
        let a = split_lines(old);
        let b = split_lines(new);
        let edits = diff_lines(&a, &b);
        if edits.iter().all(|e| e.op == Op::Equal) {
            continue;
        }
        out.push(FileDiff {
            name: name.clone(),
            label: block_label(name),
            old_content: old.clone(),
            new_content: new.clone(),
            edits,
        });
    }
    out
}

/// 单文件内可读的 hunk 区间（带上下文）。
fn hunks(edits: &[Edit], context: usize) -> Vec<Vec<Edit>> {
    let change_idx: Vec<usize> = edits
        .iter()
        .enumerate()
        .filter(|(_, e)| e.op != Op::Equal)
        .map(|(i, _)| i)
        .collect();
    if change_idx.is_empty() {
        return Vec::new();
    }
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    for &i in &change_idx {
        let lo = i.saturating_sub(context);
        let hi = (i + 1 + context).min(edits.len());
        if let Some(last) = ranges.last_mut() {
            if lo <= last.1 {
                last.1 = hi;
                continue;
            }
        }
        ranges.push((lo, hi));
    }
    ranges
        .into_iter()
        .map(|(lo, hi)| edits[lo..hi].to_vec())
        .collect()
}

/// 渲染单个文件的 diff，返回字符串。
pub fn render_file_diff(fd: &FileDiff, no_color: bool) -> String {
    if no_color {
        colored::control::set_override(false);
    }
    let mut s = String::new();
    let total_old = fd.old_content.lines().count();
    let total_new = fd.new_content.lines().count();
    s.push_str(
        &format!(
            "{} {} · {}\n",
            "━━━".bright_cyan(),
            fd.name.bright_yellow().bold(),
            fd.label.bright_cyan(),
        ),
    );
    s.push_str(&format!(
        "{} {} → {} 行\n",
        "文件规模:".dimmed(),
        total_old.to_string().bright_yellow(),
        total_new.to_string().bright_green(),
    ));

    let hunks = hunks(&fd.edits, 3);
    if hunks.is_empty() {
        s.push_str(&format!("  {}（内容无变化）\n", "✔".green()));
        return s;
    }

    for hunk in hunks {
        let old_start = hunk.iter().find_map(|e| e.old).unwrap_or(0).max(1);
        let new_start = hunk.iter().find_map(|e| e.new).unwrap_or(0).max(1);
        let old_len = hunk.iter().filter(|e| e.op != Op::Insert).count();
        let new_len = hunk.iter().filter(|e| e.op != Op::Delete).count();
        s.push_str(&format!(
            "  {} -{},{} +{},{} {}\n",
            "@@".magenta().bold(),
            old_start.to_string().cyan(),
            old_len.to_string().cyan(),
            new_start.to_string().cyan(),
            new_len.to_string().cyan(),
            "@@".magenta().bold(),
        ));

        let mut i = 0;
        while i < hunk.len() {
            let e = &hunk[i];
            match e.op {
                Op::Equal => {
                    let old = e.old.map(|n| n.to_string()).unwrap_or_default();
                    let new = e.new.map(|n| n.to_string()).unwrap_or_default();
                    s.push_str(&format!(
                        "  {} {} │ {}\n",
                        pad(&old, 4).dimmed(),
                        pad(&new, 4).dimmed(),
                        e.text.dimmed(),
                    ));
                    i += 1;
                }
                Op::Delete => {
                    let old = e.old.map(|n| n.to_string()).unwrap_or_default();
                    if i + 1 < hunk.len() && hunk[i + 1].op == Op::Insert {
                        // 修改对：行内词级高亮，未变部分弱化、变化部分加粗下划线
                        let (old_tokens, new_tokens) = diff_words(&e.text, &hunk[i + 1].text);
                        let old_line: String = old_tokens
                            .iter()
                            .map(|(t, changed)| {
                                if *changed {
                                    t.red().bold().underline().to_string()
                                } else {
                                    t.dimmed().to_string()
                                }
                            })
                            .collect();
                        let new = hunk[i + 1]
                            .new
                            .map(|n| n.to_string())
                            .unwrap_or_default();
                        let new_line: String = new_tokens
                            .iter()
                            .map(|(t, changed)| {
                                if *changed {
                                    t.green().bold().underline().to_string()
                                } else {
                                    t.dimmed().to_string()
                                }
                            })
                            .collect();
                        s.push_str(&format!(
                            "  {} {} │ -{}\n",
                            pad(&old, 4).red(),
                            pad("", 4),
                            old_line
                        ));
                        s.push_str(&format!(
                            "  {} {} │ +{}\n",
                            pad("", 4),
                            pad(&new, 4).green(),
                            new_line
                        ));
                        i += 2;
                        continue;
                    }
                    s.push_str(&format!(
                        "  {} {} │ -{}\n",
                        pad(&old, 4).red(),
                        pad("", 4),
                        e.text.red()
                    ));
                    i += 1;
                }
                Op::Insert => {
                    let new = e.new.map(|n| n.to_string()).unwrap_or_default();
                    s.push_str(&format!(
                        "  {} {} │ +{}\n",
                        pad("", 4),
                        pad(&new, 4).green(),
                        e.text.green()
                    ));
                    i += 1;
                }
            }
        }
    }
    s
}

/// 渲染整组文件的 diff，含统计与后续建议。
pub fn render_diffs(files: &[FileDiff], no_color: bool) -> String {
    let mut s = String::new();
    let mut added = 0usize;
    let mut removed = 0usize;
    for fd in files {
        added += fd.edits.iter().filter(|e| e.op == Op::Insert).count();
        removed += fd.edits.iter().filter(|e| e.op == Op::Delete).count();
        s.push_str(&render_file_diff(fd, no_color));
        s.push('\n');
    }
    s.push_str(&format!(
        "{} 个文件变更，{}{}，{}{}\n",
        files.len().to_string().bright_yellow().bold(),
        "+".green().bold(),
        added.to_string().green().bold(),
        "-".red().bold(),
        removed.to_string().red().bold(),
    ));
    if files.iter().any(|f| f.name == "tests.yaml" || f.name == "vars.yaml") {
        s.push_str(
            &format!(
                "{} 变量/用例有变动，建议运行 `{}` 验证评测门禁\n",
                "⚠".yellow().bold(),
                "prompt-git test".bright_cyan().bold()
            ),
        );
    } else {
        s.push_str(
            &format!(
                "{} 每次变更后建议运行 `{}` 确认回归\n",
                "→".bright_cyan(),
                "prompt-git test".bright_cyan().bold()
            ),
        );
    }
    s
}

/// 计算两段文本的相似度（字符 bigram Jaccard），0~1。
pub fn similarity_ratio(a: &str, b: &str) -> f64 {
    let bigrams = |s: &str| -> BTreeMap<String, usize> {
        let mut m = BTreeMap::new();
        let chars: Vec<char> = s.chars().collect();
        for w in chars.windows(2) {
            let k: String = w.iter().collect();
            *m.entry(k).or_insert(0) += 1;
        }
        m
    };
    let ma = bigrams(a);
    let mb = bigrams(b);
    if ma.is_empty() && mb.is_empty() {
        return 1.0;
    }
    let mut inter = 0usize;
    let mut union = 0usize;
    let mut keys: std::collections::BTreeSet<&String> = ma.keys().collect();
    keys.extend(mb.keys());
    for k in keys {
        let ca = ma.get(k).copied().unwrap_or(0);
        let cb = mb.get(k).copied().unwrap_or(0);
        inter += ca.min(cb);
        union += ca.max(cb);
    }
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_identical() {
        let a = vec!["a".to_string(), "b".to_string()];
        let edits = diff_lines(&a, &a);
        assert!(edits.iter().all(|e| e.op == Op::Equal));
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn diff_all_insert() {
        let a: Vec<String> = vec![];
        let b = vec!["x".to_string(), "y".to_string()];
        let edits = diff_lines(&a, &b);
        assert!(edits.iter().all(|e| e.op == Op::Insert));
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn diff_all_delete() {
        let a = vec!["x".to_string(), "y".to_string()];
        let b: Vec<String> = vec![];
        let edits = diff_lines(&a, &b);
        assert!(edits.iter().all(|e| e.op == Op::Delete));
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn diff_single_line_change_is_delete_insert() {
        // 行级 diff 以行为最小单位：单行改写表现为 Delete+Insert，
        // 行内差异由 diff_words（词级）负责高亮。
        let a = vec!["你好，世界".to_string()];
        let b = vec!["你好，世界呀".to_string()];
        let edits = diff_lines(&a, &b);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].op, Op::Delete);
        assert_eq!(edits[1].op, Op::Insert);
        assert_eq!(edits[0].text, "你好，世界");
        assert_eq!(edits[1].text, "你好，世界呀");
    }

    #[test]
    fn diff_insert_keeps_surrounding_context() {
        let a = vec![
            "第一行".to_string(),
            "第二行".to_string(),
            "第三行".to_string(),
        ];
        let b = vec![
            "第一行".to_string(),
            "第二行".to_string(),
            "新增行".to_string(),
            "第三行".to_string(),
        ];
        let edits = diff_lines(&a, &b);
        assert_eq!(edits.len(), 4);
        assert_eq!(edits[0].op, Op::Equal);
        assert_eq!(edits[1].op, Op::Equal);
        assert_eq!(edits[2].op, Op::Insert);
        assert_eq!(edits[2].text, "新增行");
        assert_eq!(edits[3].op, Op::Equal);
    }

    #[test]
    fn diff_replacement() {
        let a = vec!["old line".to_string()];
        let b = vec!["new line".to_string()];
        let edits = diff_lines(&a, &b);
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].op, Op::Delete);
        assert_eq!(edits[1].op, Op::Insert);
    }

    #[test]
    fn diff_larger_than_deletion() {
        // 经典用例：插入更多行
        let a = vec!["1".to_string(), "4".to_string()];
        let b = vec!["1".to_string(), "2".to_string(), "3".to_string(), "4".to_string()];
        let edits = diff_lines(&a, &b);
        let inserts = edits.iter().filter(|e| e.op == Op::Insert).count();
        let deletes = edits.iter().filter(|e| e.op == Op::Delete).count();
        assert_eq!(inserts, 2);
        assert_eq!(deletes, 0);
    }

    #[test]
    fn word_diff_marks_changed() {
        let (old, new) = diff_words("color red", "colour red");
        let old_str: String = old.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>().join("");
        let new_str: String = new.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>().join("");
        assert_eq!(old_str, "color red");
        assert_eq!(new_str, "colour red");
        // 至少各有一个 token 被标记变化
        assert!(old.iter().any(|(_, c)| *c));
        assert!(new.iter().any(|(_, c)| *c));
    }

    #[test]
    fn similarity_perfect_and_partial() {
        assert_eq!(similarity_ratio("abc", "abc"), 1.0);
        let r = similarity_ratio("abcdef", "abcxyz");
        assert!(r > 0.0 && r < 1.0);
        assert_eq!(similarity_ratio("", ""), 1.0);
    }

    #[test]
    fn build_file_diffs_skips_empty() {
        let files = vec![("a.md".to_string(), String::new(), String::new())];
        let diffs = build_file_diffs(&files);
        assert!(diffs.is_empty());
    }
}
