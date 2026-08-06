//! **render_metrics — 把库里真实的指标状态渲染成一页自包含 HTML。**
//!
//! 配套 skill:`docs/skills/metrics-render/SKILL.md`。skill 管「什么时候渲染、
//! 渲染出来算不算数」,**真实性由这段代码保证**——护栏写在类型和分支里,不
//! 靠每次让模型自觉。plan/21 §6 的六条硬约束逐条对应到这里:
//!
//! 1. **无数据 = Unknown 灰,绝不假绿**:显示值只从 `observation` 真实序列取
//!    (`latest_of`),零观测直接渲染「无数据」——不取 `metric.value_raw` 缓存、
//!    不填 0、不沿用上一次的值、不插值。
//! 2. **来源徽记**:`SourceKind::Manual` 戴「手填」徽;`collect_kind` 是
//!    `bw`/`connector` 的标「v1 未接」(见 `docs/metrics-toml-format.md` 采集器
//!    状态表);`origin` 区分正本(file)与界面手建(manual)。
//! 3. **数字可独立复核**:每条指标自带一行能直接跑的 `sqlite3` 命令,跑出来的
//!    就是它上面显示的那些点。页面自带自证方式,不需要信这段代码。
//! 4. **绿色隐身**:green 不高亮不庆祝;每个项目顶部只列 signal≠green 的项
//!    (`plan/00` §6 的原始业务规则)。
//! 5. **1 个观测点不画趋势线**:画了就是插值。单点显式标「仅 1 个观测点」;
//!    非数值观测(如 `5/5`)同样不画。
//! 6. **只吃两个源**:`metric` 表的定义(经 `SyncMetricsFile` 来自
//!    `.bw/metrics.toml`)+ `observation` 表的值。**命令行不接受任何数字入参**
//!    —— 想让页面上出现一个数,只能先让它成为一条真实观测。
//!
//! 只读:除 `SqliteStore::open` 自身的幂等建表外不写库,不 recompute、不采集。
//!
//! 用法:
//!   cargo run -p bw-app --example render_metrics -- <db-path> <out.html> [项目名 …]

use bw_core::{Signal, SourceKind, StageKind};
use bw_store::{MetricOrigin, MetricRole, MetricSignal, ObservationRow, SqliteStore, Store};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

// ───────────────────────────── helpers ─────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// 护栏 1 的实现点:显示值**只**来自真实观测序列。零观测 → `None` →
/// 页面渲染「无数据」。这里刻意不 fall back 到 `metric.value_raw`。
fn latest_of(series: &[&ObservationRow]) -> Option<String> {
    series.last().map(|o| o.raw.clone())
}

/// 护栏 5:能解析成数的观测才进趋势;`5/5`、`清零` 这类非数值观测不画。
fn as_num(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    cleaned.parse::<f64>().ok()
}

fn signal_class(s: Option<Signal>) -> &'static str {
    match s {
        Some(Signal::Green) => "green",
        Some(Signal::Amber) => "amber",
        Some(Signal::Red) => "red",
        _ => "unknown",
    }
}

fn signal_label(s: Option<Signal>) -> &'static str {
    match s {
        Some(Signal::Green) => "达标",
        Some(Signal::Amber) => "接近",
        Some(Signal::Red) => "未达标",
        _ => "无数据",
    }
}

fn stage_label(k: StageKind) -> &'static str {
    match k {
        StageKind::Prototype => "原型",
        StageKind::Build => "构建",
        StageKind::Optimize => "优化",
        StageKind::Growth => "运营推广",
        StageKind::Ops => "运维",
    }
}

/// 护栏 2:只有 `Manual` 戴手填徽——github/script/codehub/connector 都是机器采,
/// 戴徽会是谎报。`None`(零观测)不戴任何来源徽,因为它根本还没有来源。
fn source_badge(src: Option<SourceKind>) -> String {
    match src {
        Some(SourceKind::Manual) => r#"<span class="badge manual">手填</span>"#.into(),
        Some(k) => format!(r#"<span class="badge">{k:?}</span>"#).to_lowercase(),
        None => String::new(),
    }
}

/// 护栏 2 的另一半:采集器 v1 没接的 kind 明说没接,不让它假装在采。
fn collect_badge(kind: &str) -> String {
    match kind {
        "" => r#"<span class="badge warn">无采集方案</span>"#.into(),
        "bw" | "connector" => format!(
            r#"<span class="badge">{}</span><span class="badge warn">v1 未接</span>"#,
            esc(kind)
        ),
        k => format!(r#"<span class="badge">{}</span>"#, esc(k)),
    }
}

/// 护栏 5:< 2 个可解析为数的点 → 不画线。
fn sparkline(series: &[&ObservationRow]) -> Option<String> {
    let vals: Vec<f64> = series.iter().filter_map(|o| as_num(&o.raw)).collect();
    if vals.len() < 2 {
        return None;
    }
    let (w, h, pad) = (120.0_f64, 28.0_f64, 3.0_f64);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for v in &vals {
        lo = lo.min(*v);
        hi = hi.max(*v);
    }
    let span = if (hi - lo).abs() < f64::EPSILON {
        1.0
    } else {
        hi - lo
    };
    let step = (w - pad * 2.0) / (vals.len() - 1) as f64;
    let pts: Vec<String> = vals
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = pad + step * i as f64;
            let y = h - pad - (v - lo) / span * (h - pad * 2.0);
            format!("{x:.1},{y:.1}")
        })
        .collect();
    let last = pts.last().cloned().unwrap_or_default();
    let (lx, ly) = last.split_once(',').unwrap_or(("0", "0"));
    Some(format!(
        r#"<svg class="spark" viewBox="0 0 {w} {h}" width="{w}" height="{h}" aria-label="趋势,{n} 个真实观测点"><polyline points="{p}"/><circle cx="{lx}" cy="{ly}" r="2.4"/></svg>"#,
        n = vals.len(),
        p = pts.join(" "),
    ))
}

// ───────────────────────────── rendering ─────────────────────────────

fn metric_row(m: &MetricSignal, series: &[&ObservationRow], db: &str) -> String {
    let latest = latest_of(series);
    let sig = signal_class(m.signal);
    let role = match m.role {
        MetricRole::Leading => "引领",
        MetricRole::Lagging => "滞后",
    };
    let origin = match m.origin {
        MetricOrigin::File => r#"<span class="badge file">正本</span>"#,
        MetricOrigin::Manual => r#"<span class="badge">界面手建</span>"#,
    };

    // 护栏 1:零观测就是零观测——写「无数据」,不写 0,不写别的。
    let value_cell = match &latest {
        Some(v) => format!(r#"<span class="val">{}</span>"#, esc(v)),
        None => r#"<span class="val none">无数据</span>"#.into(),
    };

    // 护栏 5:够点才画线;只有一个点就说只有一个点。
    let trend = match sparkline(series) {
        Some(svg) => svg,
        None if series.len() == 1 => r#"<span class="note">仅 1 个观测点,不画趋势</span>"#.into(),
        None if series.len() > 1 => r#"<span class="note">观测非数值,不画趋势</span>"#.into(),
        None => String::new(),
    };

    let obs_note = format!(
        r#"<span class="note">{} 个观测点{}</span>"#,
        series.len(),
        series
            .last()
            .map(|o| format!(" · 最近 {}", o.ts.date()))
            .unwrap_or_default()
    );

    // 护栏 3:这一行就是它自己的证明方式。
    let verify = format!(
        "sqlite3 '{db}' \"SELECT datetime(ts,'unixepoch') AS ts, raw, source_kind \
         FROM observation WHERE metric_id='{}' ORDER BY ts;\"",
        // MetricId 是 newtype 包 Uuid、不实现 Display;取内层 uuid,这样复核
        // 命令能原样粘进终端跑。
        m.id.uuid()
    );

    format!(
        r#"<tr class="sig-{sig}">
  <td class="cell-name"><span class="role">{role}</span> {name}{origin}
    <div class="def">{def}</div>
    <details><summary>读回这条</summary><pre>{verify}</pre></details>
  </td>
  <td class="cell-val">{value_cell}<div class="chip {sig}">{siglabel}</div></td>
  <td class="cell-target">{target}</td>
  <td class="cell-trend">{trend}<div>{obs_note}</div></td>
  <td class="cell-src">{srcb}{collectb}</td>
</tr>"#,
        name = esc(&m.name),
        def = esc(&m.def),
        target = if m.target_raw.is_empty() {
            "—".into()
        } else {
            esc(&m.target_raw)
        },
        siglabel = signal_label(m.signal),
        srcb = source_badge(m.source),
        collectb = collect_badge(&m.collect_kind),
        verify = esc(&verify),
    )
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!(
            "用法: cargo run -p bw-app --example render_metrics -- <db-path> <out.html> [项目名 …]"
        );
        eprintln!("注意:命令行不接受任何指标数值入参——页面上的每个数都必须先是一条真实观测。");
        std::process::exit(2);
    }
    let (db_path, out_path) = (args[0].clone(), args[1].clone());
    let only: Vec<String> = args[2..].to_vec();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let projects = store.list_projects().await.expect("list_projects");

    let mut body = String::new();
    let (mut n_metrics, mut n_obs, mut n_nodata) = (0usize, 0usize, 0usize);

    for p in &projects {
        if !only.is_empty() && !only.contains(&p.name) {
            continue;
        }
        let sigs = store.persisted_signals(p.id).await.expect("signals");
        let obs = store.list_observations(p.id).await.expect("observations");
        let mut by_metric: HashMap<_, Vec<&ObservationRow>> = HashMap::new();
        for o in &obs {
            by_metric.entry(o.metric_id).or_default().push(o);
        }

        let (proj_level, stage_level): (Vec<_>, Vec<_>) =
            sigs.metrics.iter().partition(|m| m.stage_kind.is_none());

        // 护栏 4「绿色隐身」:顶部只浮出 signal≠green 的项。green 不进这个区。
        let loud: Vec<&&MetricSignal> = proj_level
            .iter()
            .filter(|m| m.signal != Some(Signal::Green))
            .collect();

        let ns_series = String::new(); // 北极星的值不在 metric 表里,只显定义
        let _ = ns_series;

        let _ = write!(
            body,
            r#"<section class="project">
<div class="phead">
  <h2>{name}</h2>
  <span class="chip {psig}">{pslabel}</span>
  <span class="pill">当前阶段 · {stage}</span>
  <span class="pill">{ws}</span>
</div>
<div class="ns">
  <div class="ns-tag">北极星</div>
  <div class="ns-name">{ns}</div>
  <div class="ns-def">{nsdef}</div>
  <div class="ns-collect">{nscollect}</div>
</div>"#,
            name = esc(&p.name),
            psig = signal_class(p.signal),
            pslabel = signal_label(p.signal),
            stage = stage_label(p.active_stage),
            ws = if p.workspace_path.is_empty() {
                "未挂工作区".to_string()
            } else {
                esc(&p.workspace_path)
            },
            ns = if p.north_star.is_empty() {
                "(未定)".into()
            } else {
                esc(&p.north_star)
            },
            nsdef = esc(&p.ns_def),
            nscollect = collect_badge(&p.north_star_collect_kind),
        );

        // 出声区
        if loud.is_empty() {
            let _ = write!(
                body,
                r#"<p class="quiet">项目级业务指标里没有需要出声的项(绿色隐身)。</p>"#
            );
        } else {
            let items: String = loud
                .iter()
                .map(|m| {
                    format!(
                        r#"<li class="sig-{c}"><span class="chip {c}">{l}</span> {n}</li>"#,
                        c = signal_class(m.signal),
                        l = signal_label(m.signal),
                        n = esc(&m.name)
                    )
                })
                .collect();
            let _ = write!(
                body,
                r#"<div class="loud"><h3>需要出声的 {k} 项</h3><ul>{items}</ul></div>"#,
                k = loud.len()
            );
        }

        // 项目级业务指标(层 A)
        let rows: String = proj_level
            .iter()
            .map(|m| {
                let empty: Vec<&ObservationRow> = Vec::new();
                let s = by_metric.get(&m.id).unwrap_or(&empty);
                n_metrics += 1;
                n_obs += s.len();
                if s.is_empty() {
                    n_nodata += 1;
                }
                metric_row(m, s, &db_path)
            })
            .collect();
        if proj_level.is_empty() {
            let _ = write!(
                body,
                r#"<p class="quiet">这个项目还没有项目级业务指标——正本 <code>.bw/metrics.toml</code> 尚未同步。</p>"#
            );
        } else {
            let _ = write!(
                body,
                r#"<h3 class="sec">项目级业务指标</h3>
<div class="scroll"><table>
<thead><tr><th>指标</th><th>最新值</th><th>目标</th><th>真实观测</th><th>来源</th></tr></thead>
<tbody>{rows}</tbody></table></div>"#
            );
        }

        // 阶段指标(层 B,plan/18 §1.1:只当现状数,不掺业务链路)
        if !stage_level.is_empty() {
            let items: String = stage_level
                .iter()
                .map(|m| {
                    let empty: Vec<&ObservationRow> = Vec::new();
                    let s = by_metric.get(&m.id).unwrap_or(&empty);
                    format!(
                        r#"<li><span class="pill">{stage}</span> {n} · {v} <span class="note">{c} 个观测点</span></li>"#,
                        stage = m.stage_kind.map(stage_label).unwrap_or("—"),
                        n = esc(&m.name),
                        v = latest_of(s).map(|v| esc(&v)).unwrap_or_else(|| "无数据".into()),
                        c = s.len()
                    )
                })
                .collect();
            let _ = write!(
                body,
                r#"<details class="layerb"><summary>阶段指标 · {k} 条(层 B 通用项目管理指标,只当现状数,不进业务北极星链路)</summary><ul>{items}</ul></details>"#,
                k = stage_level.len()
            );
        }

        let _ = write!(body, "</section>");
    }

    // 模板用 HTML 注释占位而不是 `format!` 的 `{}` —— 模板里有整段 CSS,
    // `format!` 会把每个 `{` 当成占位符。
    let html = include_str!("render_metrics_template.html")
        .replace("<!--BODY-->", &body)
        .replace("<!--DB-->", &esc(&db_path))
        .replace("<!--N_METRICS-->", &n_metrics.to_string())
        .replace("<!--N_OBS-->", &n_obs.to_string())
        .replace("<!--N_NODATA-->", &n_nodata.to_string())
        .replace(
            "<!--GENERATED-->",
            &time::OffsetDateTime::now_utc().date().to_string(),
        );
    std::fs::write(&out_path, html).expect("write html");

    println!("✓ 已渲染 → {out_path}");
    println!(
        "  项目级业务指标 {n_metrics} 条 · 真实观测 {n_obs} 个点 · 其中 {n_nodata} 条零观测(渲染为「无数据」,不假绿)"
    );
    println!("  数据源:{db_path}(只读);页面每条指标自带 sqlite3 复核命令");
}
