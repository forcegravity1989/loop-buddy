//! 指标采集 —— 起脚本、传时间窗、读标准输出的 JSON。
//!
//! 设计正本是 [`design.md`](../../../../docs/v4-prototype/design.md) 第 10 章。
//! 这个模块只做那一章 §10.2-§10.4 那三件事,**一件业务判断都不做**:
//!
//! 1. **采集方式只有两种** —— 脚本、手填。没有第三种,尤其**没有「buddy 内建的
//!    git 算子」**:那等于把算法藏进二进制,项目看不见、改不了、也说不清这个数
//!    怎么来的。git 类统计一样走脚本,只不过脚本由 buddy 铺一份现成的。
//! 2. **窗口由 buddy 给,不是脚本自己决定**。面板要四周就传四周,要八周就传
//!    八周 —— 采多少和画多少永远一致。V3 把这个决定权留在脚本里(自带脚本硬编码
//!    30 天)而面板按 8 周画,于是前三四周永远是空的,长期对不上还没人发现。
//! 3. **只认标准输出的一个 JSON 对象**,不是 JSON 就算这次采集失败。V3 只读脚本
//!    写出的文件、不看标准输出,结果**每次采集都报「成功」而指标永远没数**。
//!
//! **失败一律如实,绝不写 0。** 0 是一个真实的数值,「没采到」不是。

use crate::isoweek;
use crate::repo::metrics_file::{self, MetricClass, MetricDef};
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

/// 单个脚本最多跑多久。超时按「没采到」算,不是按 0 算。
const TIMEOUT: Duration = Duration::from_secs(90);

/// 一周一个点。`value` **原样保留脚本给的东西** —— 数也好、`"87%"` 这种给人看的
/// 字符串也好,要画走势时才解析,解析不出来的点跳过、不画、不猜。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricPoint {
    pub week: String,
    pub value: String,
}

/// 一条指标采完之后的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricReadout {
    /// 指标名 —— `.bw/metrics.toml` 里没有单独的 id,名字就是键。
    pub name: String,
    pub class: MetricClass,
    /// 近 N 周,旧的在前。手填与采集失败时是空的。
    pub points: Vec<MetricPoint>,
    /// 现值 = 最后一个点。**空 ≠ 0**。
    pub current: Option<String>,
    /// 这次为什么没采到。空 = 采到了,或者压根不该采(手填)。**不吞错误。**
    pub error: String,
}

/// 采一遍这个项目的全部指标。
///
/// 三类各走各的路:可回溯的现算(传窗口跑脚本);不可回溯的今天**如实说还没
/// 接**(读数流水那条路没落地);手填的不采,界面上打「手填」徽记。
pub async fn collect_all(ws: &Path, weeks: u32) -> Result<Vec<MetricReadout>, String> {
    let file = metrics_file::read(ws)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "这个项目还没有 .bw/metrics.toml".to_string())?;
    let labels = week_labels(weeks);
    let mut out = Vec::new();
    for def in file.all() {
        out.push(collect_one(ws, def, &labels).await);
    }
    Ok(out)
}

async fn collect_one(ws: &Path, def: &MetricDef, labels: &[String]) -> MetricReadout {
    let class = def.collect.class();
    let mut r = MetricReadout {
        name: def.name.clone(),
        class,
        points: Vec::new(),
        current: None,
        error: String::new(),
    };
    match class {
        MetricClass::Manual => return r,
        MetricClass::PointInTime => {
            // 如实:这条路设计好了(读数追加进 `.bw/metrics/readings.jsonl`),
            // 但今天一行没落地。**不假装采过、也不悄悄当成手填。**
            r.error = "这条标着不可回溯,而按时采一次、追加进读数流水的那条路还没接上 —— \
                       今天没有数。要么给它找一个能倒推历史的采法改成可回溯,要么先当手填。"
                .into();
            return r;
        }
        MetricClass::Retro => {}
    }
    if def.collect.run.is_empty() {
        r.error = "这条写的是脚本采集,但 run 是空的 —— 那是一条写了一半的定义,没法采".into();
        return r;
    }
    match run_script(ws, &def.collect.run, labels).await {
        Ok(points) => {
            r.current = points.last().map(|p| p.value.clone());
            r.points = points;
        }
        Err(e) => r.error = e,
    }
    r
}

/// 脚本吐回来的 JSON。可回溯的给序列;不可回溯的给一个当下的值(今天用不上,
/// 但协议里留着,免得将来接读数流水时再改一次格式)。
#[derive(Deserialize)]
struct ScriptOut {
    #[serde(default)]
    points: Vec<ScriptPoint>,
    #[serde(default)]
    value: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct ScriptPoint {
    week: String,
    value: serde_json::Value,
}

/// 把 JSON 值变成给人看的字符串。**数不带小数点尾巴,字符串原样。**
fn as_text(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

async fn run_script(
    ws: &Path,
    run: &[String],
    labels: &[String],
) -> Result<Vec<MetricPoint>, String> {
    let (since, until) = window_of(labels)?;
    let mut cmd = v4_engine::tokio_cmd(&run[0]);
    cmd.args(&run[1..])
        .arg("--since")
        .arg(&since)
        .arg("--until")
        .arg(&until)
        .arg("--granularity")
        .arg("week")
        .current_dir(ws)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let out = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Err(_) => return Err(format!("采集脚本超过 {} 秒还没跑完", TIMEOUT.as_secs())),
        Ok(Err(e)) => return Err(format!("起不动采集脚本 `{}`:{e}", run.join(" "))),
        Ok(Ok(o)) => o,
    };
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let why = if why.is_empty() {
            format!("退出码 {}", out.status)
        } else {
            why
        };
        return Err(format!("采集脚本失败:{why}"));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let parsed: ScriptOut = serde_json::from_str(text.trim()).map_err(|e| {
        // 把原始输出的头一段带出来 —— 「不是 JSON」这句话本身帮不上人,
        // 看见它到底打了什么才帮得上。
        let head: String = text.trim().chars().take(120).collect();
        format!("采集脚本的输出不是 JSON({e});它打的是:{head}")
    })?;
    if parsed.points.is_empty() {
        if parsed.value.is_some() {
            return Err(
                "这条标着可回溯,但脚本只给了一个当下的值、没有按周的序列 —— \
                        多半是脚本没认 --since/--until,检查一下标对了没有"
                    .into(),
            );
        }
        return Err("采集脚本没给出任何数据点".into());
    }
    // 按 buddy 要的周对齐:脚本多给的周丢掉,少给的周**留空**(断开,不补前值)。
    let mut points = Vec::with_capacity(labels.len());
    for w in labels {
        if let Some(p) = parsed.points.iter().find(|p| p.week.trim() == w.as_str()) {
            if let Some(v) = as_text(&p.value) {
                points.push(MetricPoint {
                    week: w.clone(),
                    value: v,
                });
            }
        }
    }
    if points.is_empty() {
        return Err("采集脚本给的周号和 buddy 要的对不上(要的是 `2026-W34` 这种 ISO 周)".into());
    }
    Ok(points)
}

/// 近 `weeks` 周的 ISO 周号,**旧的在前**。
fn week_labels(weeks: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mut w = isoweek::current_week();
    for _ in 0..weeks.max(1) {
        out.push(w.clone());
        let Some(prev) = isoweek::previous_week(&w) else {
            break;
        };
        w = prev;
    }
    out.reverse();
    out
}

/// 这一串周对应的 `[since, until)`,都是本机时区的日期。**左闭右开** ——
/// `until` 是最后一周的下周一,不是那一周的周日,免得少算一天。
fn window_of(labels: &[String]) -> Result<(String, String), String> {
    let first = labels.first().ok_or("没有要采的周")?;
    let last = labels.last().ok_or("没有要采的周")?;
    let (since, _) = isoweek::week_bounds(first).ok_or(format!("认不出周号 {first}"))?;
    let (_, until) = isoweek::week_bounds(last).ok_or(format!("认不出周号 {last}"))?;
    let fmt = |d: time::Date| format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day());
    Ok((fmt(since), fmt(until)))
}
