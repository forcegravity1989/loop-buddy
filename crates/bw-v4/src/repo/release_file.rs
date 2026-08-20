//! `docs/releases.md` —— 发版记录,**唯一正本**。库里不存副本。
//!
//! 一行一个版本:版本号、发版日、说明、包含的活、来源。「包含的活」是活号的
//! 自由文本(如 `#88 #91 #93`),渲染时按号去 `issue` 缓存拿标题展开;号找不到
//! 对应的活就跳过这条关联并记一句警告,不让整份文件解析失败。

use super::{read_to_string, write_file, Result};
use std::path::Path;

pub const REL_PATH: &str = ".bw/releases.md";

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

/// buddy 这张表的表头。**只认这张表** —— 老项目仓里可能已经有一份格式完全
/// 不同的发版记录(列数、列名都不一样),把行往那种表里塞会把值错位到别的
/// 列上。认表头是最简单也最不会出错的判据。
pub const HEADER: [&str; 5] = ["版本号", "发版日", "说明", "包含的活", "来源"];

fn cells_of(line: &str) -> Option<Vec<String>> {
    let t = line.trim();
    if !t.starts_with('|') || !t.ends_with('|') {
        return None;
    }
    Some(
        t.trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect(),
    )
}

fn is_separator(cells: &[String]) -> bool {
    !cells.is_empty()
        && cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
}

fn is_our_header(cells: &[String]) -> bool {
    cells.len() >= 4
        && HEADER
            .iter()
            .take(4)
            .enumerate()
            .all(|(i, h)| cells[i] == *h)
}

/// 只解析 buddy 那张表里的行。文件里别的表格(老项目自己的发版流水)一概
/// 不碰,也不当成解析失败。
pub fn parse(raw: &str) -> Vec<ReleaseRow> {
    let mut rows = Vec::new();
    let mut in_our_table = false;
    for line in raw.lines() {
        let Some(cells) = cells_of(line) else {
            in_our_table = false;
            continue;
        };
        if is_our_header(&cells) {
            in_our_table = true;
            continue;
        }
        if !in_our_table || is_separator(&cells) {
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
    // 新版本排在表格最上面(最新的在前)。找不到 buddy 那张表就另起一段,
    // 绝不往老项目自己那张表里塞行 —— 列对不上会把值错位到别的列上。
    let body = insert_under_our_header(&existing, &line)
        .unwrap_or_else(|| append_our_section(&existing, &line));
    write_file(workspace, REL_PATH, &body)?;
    Ok(true)
}

/// 把新行插进 buddy 那张表的表头之后(最新的在最上面)。找不到那张表就返回
/// `None`,由调用方另起一段,而不是往一张陌生的表里塞行。
fn insert_under_our_header(existing: &str, line: &str) -> Option<String> {
    let mut out = String::new();
    let mut state = 0; // 0=还没找到表头 1=刚过表头等分隔行 2=插完了
    for l in existing.lines() {
        out.push_str(l);
        out.push('\n');
        match state {
            0 => {
                if cells_of(l).is_some_and(|c| is_our_header(&c)) {
                    state = 1;
                }
            }
            1 if cells_of(l).is_some_and(|c| is_separator(&c)) => {
                out.push_str(line);
                state = 2;
            }
            _ => {}
        }
    }
    (state == 2).then_some(out)
}

/// 老项目已经有一份格式不同的发版记录时:另起一段 buddy 自己的表,不动人家
/// 原来那张。MR 说明里会提到这件事,由人决定要不要合并两张表。
fn append_our_section(existing: &str, line: &str) -> String {
    format!(
        "{}\n## buddy 管理的发版记录\n\n\
         > 这个仓原本已经有一份格式不同的发版记录,buddy 不改它,另起一张表。\n\
         > 两张表要不要并成一张,由人决定。\n\n\
         | {} |\n|---|---|---|---|---|\n{}",
        ensure_trailing_newline(existing),
        HEADER.join(" | "),
        line
    )
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
