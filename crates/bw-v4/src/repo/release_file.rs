//! `docs/releases.md` —— 发版记录,**唯一正本**。库里不存副本。
//!
//! 一行一个版本:版本号、发版日、说明、包含的活、来源。「包含的活」是活号的
//! 自由文本(如 `#88 #91 #93`),渲染时按号去 `issue` 缓存拿标题展开;号找不到
//! 对应的活就跳过这条关联并记一句警告,不让整份文件解析失败。

use super::{read_to_string, write_file, Result};
use std::path::Path;

pub const REL_PATH: &str = "docs/releases.md";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseRow {
    pub version: String,
    pub released_at: String,
    pub note: String,
    /// 「包含的活」列里解析出的活号。找不到对应活的号在渲染时跳过。
    pub included_numbers: Vec<u32>,
    /// `人发` 或 `回填`。
    pub origin: String,
}

pub fn read(workspace: &Path) -> Result<Option<Vec<ReleaseRow>>> {
    match read_to_string(workspace, REL_PATH)? {
        None => Ok(None),
        Some(raw) => Ok(Some(parse(&raw))),
    }
}

pub fn parse(raw: &str) -> Vec<ReleaseRow> {
    let mut rows = Vec::new();
    let mut seen_header = false;
    for line in raw.lines() {
        let t = line.trim();
        if !t.starts_with('|') || !t.ends_with('|') {
            continue;
        }
        let cells: Vec<String> = t
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        if cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }
        if !seen_header {
            seen_header = true; // 第一行是表头
            continue;
        }
        if cells.len() < 4 || cells[0].is_empty() {
            continue;
        }
        rows.push(ReleaseRow {
            version: cells[0].clone(),
            released_at: cells[1].clone(),
            note: cells[2].clone(),
            included_numbers: parse_numbers(&cells[3]),
            origin: cells.get(4).cloned().unwrap_or_else(|| "人发".into()),
        });
    }
    rows
}

fn parse_numbers(cell: &str) -> Vec<u32> {
    cell.split_whitespace()
        .filter_map(|t| t.trim_start_matches('#').trim_end_matches(',').parse().ok())
        .collect()
}

/// 追加一行发版记录。**按版本号幂等**:已经有这个版本号就原样返回 `false`,
/// 不追加第二行(指挥器重跑、回填重跑都靠这条)。
pub fn append_row(workspace: &Path, row: &ReleaseRow) -> Result<bool> {
    let existing = read_to_string(workspace, REL_PATH)?.unwrap_or_else(default_body);
    if parse(&existing).iter().any(|r| r.version == row.version) {
        return Ok(false);
    }
    let line = format!(
        "| {} | {} | {} | {} | {} |\n",
        row.version,
        row.released_at,
        row.note,
        if row.included_numbers.is_empty() {
            "—".to_string()
        } else {
            row.included_numbers
                .iter()
                .map(|n| format!("#{n}"))
                .collect::<Vec<_>>()
                .join(" ")
        },
        row.origin
    );
    // 新版本排在表格最上面(最新的在前),表头与分隔行之后插入。
    let body = match insert_after_separator(&existing, &line) {
        Some(b) => b,
        None => format!("{}{}", ensure_trailing_newline(&existing), line),
    };
    write_file(workspace, REL_PATH, &body)?;
    Ok(true)
}

fn insert_after_separator(existing: &str, line: &str) -> Option<String> {
    let mut out = String::new();
    let mut inserted = false;
    for l in existing.lines() {
        out.push_str(l);
        out.push('\n');
        if !inserted {
            let t = l.trim();
            if t.starts_with('|')
                && t.trim_matches('|').split('|').all(|c| {
                    !c.trim().is_empty() && c.trim().chars().all(|ch| ch == '-' || ch == ':')
                })
            {
                out.push_str(line);
                inserted = true;
            }
        }
    }
    inserted.then_some(out)
}

fn ensure_trailing_newline(s: &str) -> String {
    if s.ends_with('\n') {
        s.to_string()
    } else {
        format!("{s}\n")
    }
}

/// 还没有这份文件时的空骨架(只有表头,一行数据都不编)。
pub fn default_body() -> String {
    String::from(
        "# 版本登记(出包与运作)\n\
         \n\
         > **30 秒导读**:一行一个已发布或在研的版本——版本号、发版日、这一版是\n\
         > 什么、包含哪些活。**这份文件是版本记录的唯一正本**,本机库里不存副本;\n\
         > 「来源」列区分「人发」与「回填」(回填的行只解释历史,不代表当时真走过\n\
         > 评审流程)。\n\
         \n\
         | 版本号 | 发版日 | 说明 | 包含的活 | 来源 |\n\
         |---|---|---|---|---|\n",
    )
}

pub fn write_default_if_missing(workspace: &Path) -> Result<bool> {
    if read_to_string(workspace, REL_PATH)?.is_some() {
        return Ok(false);
    }
    write_file(workspace, REL_PATH, &default_body())?;
    Ok(true)
}
