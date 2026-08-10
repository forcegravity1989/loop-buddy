//! `port_readback` — headless 行为读回指挥器(next 切片一C)。
//!
//! 不开界面,直接调用已移植的 `bw-core`/`bw-engine` 公开 API,把四件关键行为
//! 各跑一遍并打印,证明「移植过来的代码不但字节没变,行为也真的活着」:
//!
//! 1. 五阶段元数据(`StageKind`)——方法论文案完整随迁。
//! 2. Issue 状态机全矩阵(`IssueStatus::can_transition_to`)——铁律
//!    「Done 唯一入边是 InReview」断言成立。
//! 3. 信号派生链(`bw_core::derive`)——无观测→Unknown、有新鲜观测→非
//!    Unknown 两个场景,均只经密封类型的合法公开入口构造。
//! 4. `.bw/metrics.toml` 正本解析(`bw_workspace::metrics_file`)——内嵌样例
//!    文本落一份临时文件,过解析器读出北极星/滞后/引领字段。**next 切片
//!    五A**:这个模块换落点到了 `bw-workspace`(design-s5-hexpanel.md
//!    §6.2/§9),`bw-engine` 反过来依赖它复用——本文件这一处 `use` 是这条
//!    复用唯一的调用点,其余三节验的仍是 `bw-core`/`bw-engine` 自己的行为。
//!
//! 跑法:`cd next && cargo run -p bw-engine --example port_readback`
//! 退出码 0 且末行 `PORT_READBACK_OK` = 全部断言通过。
//! 运行输出存档:`examples/port_readback.expected.txt`(文件头注明生成命令)。

use bw_core::derive::{evaluate_metric, measure, parse_target, Measurement};
use bw_core::{Cadence, IssueStatus, Signal, SourceKind, StageKind};
use bw_workspace::metrics_file;
use std::process::ExitCode;
use time::OffsetDateTime;

/// 内嵌 `.bw/metrics.toml` 样例——格式照 `docs/metrics-toml-format.md`:一个
/// 北极星 + 两条滞后 + 两条引领,`collect.kind` 覆盖 connector/manual/github/bw
/// 四种取值,证明正本管道对多样字段的解析能力。
const SAMPLE_METRICS_TOML: &str = r#"
schema_version = 1

[north_star]
name = "周活跃创作者数"
def  = "过去 7 天内至少发布过一篇正式内容(非草稿)的注册用户数"
collect = { kind = "connector", query = "content-analytics" }

[[lagging]]
name   = "月流失率"
def    = "上月活跃、本月零发布的用户占上月活跃用户的比例"
target = "≤8%"
collect = { kind = "connector", query = "content-analytics" }

[[lagging]]
name   = "首月留存率"
def    = "注册后 30 天内仍至少发布一次的用户占比"
target = "≥35%"
collect = { kind = "manual", query = "" }

[[leading]]
name   = "每周合并 PR 数"
def    = "过去 7 天内 merge 进 main 的 PR 数——研发产出的先行信号"
target = "≥5"
collect = { kind = "github", query = "repo:{owner}/{repo} is:pr is:merged merged:>=@{7d}" }

[[leading]]
name   = "队友结算 Issue 数"
def    = "过去 7 天内被人 merge 关闭(settle)的 Issue 数——交付节奏的先行信号"
target = "≥3"
collect = { kind = "bw", query = "issue.settled_at within 7d" }
"#;

fn main() -> ExitCode {
    let mut all_ok = true;

    all_ok &= section_stage_catalog();
    all_ok &= section_state_machine();
    all_ok &= section_signal_derivation();
    all_ok &= section_metrics_file();

    if all_ok {
        println!("PORT_READBACK_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("PORT_READBACK_FAILED");
        ExitCode::FAILURE
    }
}

/// 五阶段元数据(第 1 件事)——遍历 `StageKind::ALL`,打印
/// index/label/role/core_question 首行,证明方法论文案(五阶段的问法、角色
/// 称谓)完整随迁。
fn section_stage_catalog() -> bool {
    println!("== 五阶段元数据 ==");
    let mut count = 0u8;
    for stage in StageKind::ALL {
        count += 1;
        let core_question_first_line = stage.core_question().lines().next().unwrap_or("");
        println!(
            "{}. {} · {} · {core_question_first_line}",
            stage.index(),
            stage.label(),
            stage.role(),
        );
    }
    println!("五阶段总数: {count}");
    println!();

    if count != 5 {
        eprintln!("ASSERT FAILED: StageKind::ALL 应恰好 5 个,实得 {count}");
        return false;
    }
    true
}

/// Issue 状态机全矩阵(第 2 件事)——7×7 全枚举 `can_transition_to`,打印合
/// 法边清单,断言合法边总数(固定 20)与「Done 唯一入边是 InReview」这条铁律。
fn section_state_machine() -> bool {
    println!("== 状态机全矩阵 ==");
    let mut legal_edges = Vec::new();
    for from in IssueStatus::ALL {
        for to in IssueStatus::ALL {
            if from.can_transition_to(to) {
                legal_edges.push((from, to));
                println!("{from:?} -> {to:?}");
            }
        }
    }

    let total = legal_edges.len();
    println!("合法边总数: {total}");

    let done_inbound: Vec<IssueStatus> = legal_edges
        .iter()
        .filter(|(_, to)| *to == IssueStatus::Done)
        .map(|(from, _)| *from)
        .collect();
    println!("Done 唯一入边应为 InReview,实得入边: {done_inbound:?}");
    println!();

    let mut ok = true;
    const EXPECTED_LEGAL_EDGES: usize = 20;
    if total != EXPECTED_LEGAL_EDGES {
        eprintln!("ASSERT FAILED: 合法边总数应为 {EXPECTED_LEGAL_EDGES},实得 {total}");
        ok = false;
    }
    if done_inbound != [IssueStatus::InReview] {
        eprintln!("ASSERT FAILED: Done 唯一入边应为 [InReview],实得 {done_inbound:?}");
        ok = false;
    }
    ok
}

/// 信号派生(第 3 件事)——只用 `bw_core::derive` 的公开入口(`measure` /
/// `parse_target` / `evaluate_metric`)构造两个场景,`Derived<Signal>` 全程
/// 经这条链密封产出,指挥器本身不碰任何私有构造。
fn section_signal_derivation() -> bool {
    println!("== 信号派生 ==");
    let mut ok = true;

    let target = match parse_target("≥90%") {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ASSERT FAILED: target \"≥90%\" 应可解析,实得错误: {e}");
            return false;
        }
    };

    // 场景 a:无观测 → 派生 Unknown(没数据就是灰,绝不假装绿)。
    let missing = Measurement::Missing;
    let eval_a = evaluate_metric(&missing, &target, &[]);
    println!("场景a 输入: Measurement::Missing, target=\"≥90%\"");
    println!("场景a 派生输出: {:?} (hit={})", eval_a.signal(), eval_a.hit);
    if eval_a.signal() != Signal::Unknown {
        eprintln!(
            "ASSERT FAILED: 无观测应派生 Unknown,实得 {:?}",
            eval_a.signal()
        );
        ok = false;
    }

    // 场景 b:一条新鲜观测(as_of == now,不过期)→ 派生非 Unknown 信号。
    // `now` 用固定参考时刻而非真实挂钟,让本档案的输出确定性可复现。
    let fixed_now = OffsetDateTime::from_unix_timestamp(1_735_689_600)
        .expect("固定参考时刻应可构造(2025-01-01T00:00:00Z)");
    let measurement = measure(
        "95%",
        fixed_now,
        SourceKind::Manual,
        &Cadence::Daily,
        fixed_now,
    );
    let eval_b = evaluate_metric(&measurement, &target, &[]);
    println!(
        "场景b 输入: raw_value=\"95%\", as_of=now, source=Manual, cadence=Daily, target=\"≥90%\""
    );
    println!("场景b 派生输出: {:?} (hit={})", eval_b.signal(), eval_b.hit);
    if eval_b.signal() == Signal::Unknown {
        eprintln!(
            "ASSERT FAILED: 新鲜观测不应派生 Unknown,实得 {:?}",
            eval_b.signal()
        );
        ok = false;
    }

    println!();
    ok
}

/// `.bw/metrics.toml` 正本解析(第 4 件事)——把内嵌样例落一份临时工作区文
/// 件,过 `bw_workspace::metrics_file::read` 真实解析(next 切片五A 换落点
/// 前是 `bw_engine::metrics_file::read`),打印北极星/滞后/引领各条的
/// name/kind/collect 字段,证明正本管道随迁可用。
fn section_metrics_file() -> bool {
    println!("== 指标正本解析 ==");

    let workspace = std::env::temp_dir().join(format!("bw-port-readback-{}", std::process::id()));
    let bw_dir = workspace.join(".bw");
    if let Err(e) = std::fs::create_dir_all(&bw_dir) {
        eprintln!(
            "ASSERT FAILED: 无法建临时工作区目录 {}: {e}",
            bw_dir.display()
        );
        return false;
    }
    let metrics_path = bw_dir.join("metrics.toml");
    if let Err(e) = std::fs::write(&metrics_path, SAMPLE_METRICS_TOML) {
        eprintln!(
            "ASSERT FAILED: 无法写临时 metrics.toml {}: {e}",
            metrics_path.display()
        );
        let _ = std::fs::remove_dir_all(&workspace);
        return false;
    }

    let workspace_str = workspace.to_string_lossy().to_string();
    let parsed = match metrics_file::read(&workspace_str) {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!("ASSERT FAILED: 期望解析出 metrics.toml,实得 None(文件未找到?)");
            let _ = std::fs::remove_dir_all(&workspace);
            return false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: metrics.toml 解析失败: {e}");
            let _ = std::fs::remove_dir_all(&workspace);
            return false;
        }
    };

    println!(
        "north_star: name={:?} kind={} collect_query={:?}",
        parsed.north_star.name,
        parsed.north_star.collect.kind.as_str(),
        parsed.north_star.collect.query
    );
    for m in &parsed.lagging {
        println!(
            "lagging: name={:?} kind={} collect_query={:?}",
            m.name,
            m.collect.kind.as_str(),
            m.collect.query
        );
    }
    for m in &parsed.leading {
        println!(
            "leading: name={:?} kind={} collect_query={:?}",
            m.name,
            m.collect.kind.as_str(),
            m.collect.query
        );
    }

    let total = 1 + parsed.lagging.len() + parsed.leading.len();
    println!("北极星+滞后+引领 合计条数: {total}");
    println!();

    let _ = std::fs::remove_dir_all(&workspace);

    const MIN_METRICS: usize = 3;
    if total < MIN_METRICS {
        eprintln!("ASSERT FAILED: 样例应至少含 {MIN_METRICS} 条指标,实得 {total}");
        return false;
    }
    true
}
