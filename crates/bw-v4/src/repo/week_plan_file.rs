//! `docs/plan/YYYY-Www.md` —— 周计划,V4 的核心正本。
//!
//! 一周一份文件。**没有库内索引表**:计划屏左栏那份周列表靠扫这个目录得到。
//! 回填出来的历史周与人写的本周**是同一份规范、同一套段落结构**,只有 front
//! matter 里的 `origin` 不同(`backfill` vs `human`),界面上一个小徽记区分,
//! 不是两套渲染逻辑。
//!
//! Markdown 不走 serde——这里是手写的段落/表格扫描器,拿不准的一律返回空,
//! 绝不猜一个值出来。

use super::{read_to_string, write_file, Result};
use crate::model::{category_key, Category, StageKind};
use std::path::{Path, PathBuf};

pub const DIR: &str = "docs/plan";

pub fn rel_path(week: &str) -> String {
    format!("{DIR}/{week}.md")
}

/// front matter 的两个键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeekFrontMatter {
    pub week: String,
    /// `human`(人写的)或 `backfill`(回填出来的)。
    pub origin: String,
}

impl WeekFrontMatter {
    pub fn is_backfill(&self) -> bool {
        self.origin == "backfill"
    }
}

/// 「业务活」表格的一行 —— 九个缓存列的正本。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActivityRow {
    pub order: f64,
    pub title: String,
    pub category: Option<Category>,
    pub tool: String,
    pub workflow: String,
    pub metric_key: String,
    /// 远端 issue 号;`0` = 没挂远端,如实留白。
    pub remote_number: u32,
}

/// 「本周指标读数」表格的一行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetricReading {
    pub name: String,
    pub value: String,
    pub source: String,
    pub collected_at: String,
}

/// 「本周运作」表格的一行。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpsRow {
    pub title: String,
    pub status: String,
    pub note: String,
}

/// 一份解析出来的周计划。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeekPlan {
    pub front_matter: Option<WeekFrontMatter>,
    pub goal: Option<String>,
    pub activities: Vec<ActivityRow>,
    pub readings: Vec<MetricReading>,
    pub ops: Vec<OpsRow>,
    pub raw: String,
}

impl WeekPlan {
    /// 有没有真正定过周目标。「(未发现……)」这类回填占位不算。
    pub fn has_goal(&self) -> bool {
        self.goal.as_ref().is_some_and(|g| !is_placeholder(g))
    }

    pub fn has_reading(&self) -> bool {
        !self.readings.is_empty()
    }
}

/// 回填与人写的空段落都用括号包一句话说明「没有」。这类文本不算真数据。
fn is_placeholder(s: &str) -> bool {
    let t = s.trim();
    t.is_empty()
        || (t.starts_with('(') && t.ends_with(')'))
        || (t.starts_with('(') && t.ends_with(')'))
}

pub fn read(workspace: &Path, week: &str) -> Result<Option<WeekPlan>> {
    match read_to_string(workspace, &rel_path(week))? {
        None => Ok(None),
        Some(raw) => Ok(Some(parse(&raw))),
    }
}

pub fn exists(workspace: &Path, week: &str) -> bool {
    workspace.join(rel_path(week)).is_file()
}

/// 扫 `docs/plan/` 出周列表(新的在前)。这就是计划屏左栏周列表的全部来源
/// ——没有索引表可查。
pub fn list_weeks(workspace: &Path) -> Vec<String> {
    let dir: PathBuf = workspace.join(DIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut weeks: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let stem = name.strip_suffix(".md")?;
            crate::isoweek::week_start(stem).map(|_| stem.to_string())
        })
        .collect();
    weeks.sort();
    weeks.reverse();
    weeks
}

pub fn parse(raw: &str) -> WeekPlan {
    WeekPlan {
        front_matter: parse_front_matter(raw),
        goal: section_text(raw, "周目标"),
        activities: parse_activities(raw),
        readings: parse_readings(raw),
        ops: parse_ops(raw),
        raw: raw.to_string(),
    }
}

fn parse_front_matter(raw: &str) -> Option<WeekFrontMatter> {
    let body = raw.strip_prefix("---")?;
    let end = body.find("\n---")?;
    let (mut week, mut origin) = (String::new(), String::from("human"));
    for line in body[..end].lines() {
        if let Some(v) = line.strip_prefix("week:") {
            week = v.trim().to_string();
        } else if let Some(v) = line.strip_prefix("origin:") {
            origin = v.trim().to_string();
        }
    }
    (!week.is_empty()).then_some(WeekFrontMatter { week, origin })
}

/// 取 `## <标题>` 之后、下一个 `##` 之前的第一段非空正文(跳过引用块与注释)。
fn section_text(raw: &str, heading: &str) -> Option<String> {
    let body = section_body(raw, heading)?;
    for para in body.split("\n\n") {
        let t: Vec<&str> = para
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('>') && !l.starts_with("<!--"))
            .collect();
        if !t.is_empty() {
            return Some(t.join("\n"));
        }
    }
    None
}

fn section_body<'a>(raw: &'a str, heading: &str) -> Option<&'a str> {
    let needle = format!("\n## {heading}");
    let start = raw.find(&needle)? + needle.len();
    let rest = &raw[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    Some(&rest[..end])
}

/// 扫一个 Markdown 表格,返回每行的单元格(已去掉表头与分隔行)。
fn table_rows(body: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if !t.starts_with('|') || !t.ends_with('|') {
            continue;
        }
        let cells: Vec<String> = t
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_string())
            .collect();
        // 分隔行(---|---|---)不是数据。
        if cells
            .iter()
            .all(|c| !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':'))
        {
            continue;
        }
        rows.push(cells);
    }
    // 第一行是表头。
    if !rows.is_empty() {
        rows.remove(0);
    }
    rows
}

/// 类别列既接受显示名(原型/构建/…)也接受机读键(prototype/build/…)。
pub fn parse_category(cell: &str) -> Option<Category> {
    let t = cell.trim();
    if let Some(c) = crate::model::category_from_key(t) {
        return Some(c);
    }
    StageKind::ALL.iter().copied().find(|s| s.label() == t)
}

fn parse_activities(raw: &str) -> Vec<ActivityRow> {
    let Some(body) = section_body(raw, "业务活") else {
        return Vec::new();
    };
    table_rows(body)
        .into_iter()
        .filter(|cells| cells.len() >= 7 && !cells[1].is_empty())
        .enumerate()
        .map(|(i, cells)| ActivityRow {
            order: cells[0].parse().unwrap_or((i + 1) as f64),
            title: cells[1].clone(),
            category: parse_category(&cells[2]),
            tool: normalize_tool(&cells[3]),
            workflow: dash_to_empty(&cells[4]),
            metric_key: dash_to_empty(&cells[5]),
            remote_number: cells[6].trim_start_matches('#').parse().unwrap_or(0),
        })
        .collect()
}

fn parse_readings(raw: &str) -> Vec<MetricReading> {
    let Some(body) = section_body(raw, "本周指标读数") else {
        return Vec::new();
    };
    table_rows(body)
        .into_iter()
        .filter(|c| c.len() >= 4 && !c[0].is_empty())
        .map(|c| MetricReading {
            name: c[0].clone(),
            value: c[1].clone(),
            source: c[2].clone(),
            collected_at: c[3].clone(),
        })
        .collect()
}

fn parse_ops(raw: &str) -> Vec<OpsRow> {
    let Some(body) = section_body(raw, "本周运作") else {
        return Vec::new();
    };
    table_rows(body)
        .into_iter()
        .filter(|c| c.len() >= 3 && !c[0].is_empty())
        .map(|c| OpsRow {
            title: c[0].clone(),
            status: c[1].clone(),
            note: c[2].clone(),
        })
        .collect()
}

/// 工具列显示名 → 机读键。认不出就原样留着,不猜。
pub fn normalize_tool(cell: &str) -> String {
    match cell.trim() {
        "Claude CLI" | "claude_cli" => "claude_cli".into(),
        "Cursor" | "cursor" => "cursor".into(),
        "Open Design" | "open_design" => "open_design".into(),
        "—" | "-" | "" => String::new(),
        other => other.to_string(),
    }
}

pub fn tool_label(tool: &str) -> &str {
    match tool {
        "claude_cli" => "Claude CLI",
        "cursor" => "Cursor",
        "open_design" => "Open Design",
        "" => "—",
        other => other,
    }
}

fn dash_to_empty(s: &str) -> String {
    match s.trim() {
        "—" | "-" => String::new(),
        other => other.to_string(),
    }
}

pub fn category_label(c: Option<Category>) -> &'static str {
    c.map(|c| c.label()).unwrap_or("—")
}

/// 机读键,写文件时留给以后要机器解析的场合(今天文件里写的是显示名)。
pub fn category_machine_key(c: Option<Category>) -> &'static str {
    c.map(category_key).unwrap_or("")
}

pub fn write(workspace: &Path, week: &str, body: &str) -> Result<PathBuf> {
    write_file(workspace, &rel_path(week), body)
}

/// 一份周计划文件要写的全部内容。渲染是纯格式化 —— 数字从哪来是调用方的事,
/// 这里一个值都不发明。
#[derive(Debug, Clone, Default)]
pub struct WeekPlanDraft {
    pub week: String,
    /// `human` 或 `backfill`。
    pub origin: String,
    pub goal: Option<String>,
    pub activities: Vec<ActivityRow>,
    pub readings: Vec<MetricReading>,
    pub ops: Vec<OpsRow>,
    /// 「上周完成情况」段的正文行。空 = 没查到,写一句留白。
    pub last_week_lines: Vec<String>,
}

/// 空段落统一写成括号包着的一句话 —— 界面据此认出「这里是留白,不是 0」。
const EMPTY_GOAL: &str = "(未填——本周还没定目标)";

pub fn render(d: &WeekPlanDraft) -> String {
    let last_week = crate::isoweek::previous_week(&d.week).unwrap_or_default();
    let origin = if d.origin.is_empty() {
        "human"
    } else {
        &d.origin
    };
    let mut s = format!(
        "---\nweek: {}\norigin: {}\n---\n\n# {} 周计划\n\n",
        d.week, origin, d.week
    );
    s.push_str(
        "> 正本文件。buddy 读它驱动计划屏与总览的「本周计划进度」块;**没有库内\n\
         > 索引表** —— 计划屏左栏的周列表靠扫 `docs/plan/` 目录得到。库里活的排期、\n\
         > 工具、workflow 那几列只是缓存,与这份文件不一致时以这份文件为准。\n\n",
    );

    s.push_str("## 周目标\n\n");
    s.push_str(d.goal.as_deref().unwrap_or(EMPTY_GOAL));
    s.push_str("\n\n");

    s.push_str("## 业务活\n\n");
    s.push_str("| 顺序 | 标题 | 类别 | 工具 | workflow | 预期推动的指标 | 远端 issue |\n");
    s.push_str("|---|---|---|---|---|---|---|\n");
    if d.activities.is_empty() {
        s.push_str("<!-- 还没有排进本周的业务活。排一张活进来这里就会多一行。 -->\n");
    }
    for a in &d.activities {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            trim_order(a.order),
            a.title,
            category_label(a.category),
            tool_label(&a.tool),
            dash_if_empty(&a.workflow),
            dash_if_empty(&a.metric_key),
            if a.remote_number == 0 {
                "—".to_string()
            } else {
                format!("#{}", a.remote_number)
            }
        ));
    }
    s.push('\n');

    s.push_str("## 本周指标读数\n\n");
    s.push_str(
        "<!-- 更新指标那一步把刚读到的现状抄一份进来,随 MR 进仓——多台机器\n\
         \x20    看到同一份数字靠的是这一段,不是同步数据库。 -->\n",
    );
    s.push_str("| 指标 | 数值 | 来源 | 采集时间 |\n|---|---|---|---|\n");
    if d.readings.is_empty() {
        s.push_str("<!-- 本周还没有记过指标读数。没有读数 = 健康灯的第二条判据不成立。 -->\n");
    }
    for r in &d.readings {
        s.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            r.name, r.value, r.source, r.collected_at
        ));
    }
    s.push('\n');

    s.push_str("## 本周运作\n\n| 活 | 状态 | 说明 |\n|---|---|---|\n");
    if d.ops.is_empty() {
        s.push_str("<!-- 本周还没有运作活。 -->\n");
    }
    for o in &d.ops {
        s.push_str(&format!("| {} | {} | {} |\n", o.title, o.status, o.note));
    }
    s.push('\n');

    s.push_str(&format!("## 上周完成情况({last_week})\n\n"));
    if d.last_week_lines.is_empty() {
        s.push_str("(未发现——上周没有可读回的合入或发版记录)\n");
    } else {
        for l in &d.last_week_lines {
            s.push_str(&format!("- {l}\n"));
        }
    }
    s
}

fn trim_order(n: f64) -> String {
    if (n - n.round()).abs() < f64::EPSILON {
        format!("{}", n.round() as i64)
    } else {
        format!("{n}")
    }
}

fn dash_if_empty(s: &str) -> &str {
    if s.trim().is_empty() {
        "—"
    } else {
        s
    }
}
