//! 静态归类表自证:条数 / 重名 / 五阶段计数 —— 数字从表本身算出,不硬编。
//! 与 `verify_goal.rs` 同族:仓里不留单元测试,可核验的事实靠 example 读回。
//!
//! 跑法:cargo run -p bw-app --example verify_stage_catalog

use bw_core::model::StageKind;
use bw_core::stage_catalog::{ALL_FIVE, SKILL_STAGE_CATALOG};
use std::collections::HashSet;

fn main() {
    let total = SKILL_STAGE_CATALOG.len();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut dups: Vec<&str> = Vec::new();
    for (name, _) in SKILL_STAGE_CATALOG {
        if !seen.insert(name) {
            dups.push(name);
        }
    }

    let universal = SKILL_STAGE_CATALOG
        .iter()
        .filter(|(_, s)| s.len() == ALL_FIVE.len())
        .count();
    let no_stage = SKILL_STAGE_CATALOG
        .iter()
        .filter(|(_, s)| s.is_empty())
        .count();

    println!(
        "静态归类表 条数={total} 重名={dups:?} 全阶段通用={universal} 不属任何阶段={no_stage}"
    );
    for kind in StageKind::ALL {
        let direct = SKILL_STAGE_CATALOG
            .iter()
            .filter(|(_, s)| s.contains(&kind) && s.len() < ALL_FIVE.len())
            .count();
        let candidates = direct + universal;
        println!(
            "  {:<10} 直接挂 {:>2} + 全阶段通用 {} = 候选集 {}",
            kind.label(),
            direct,
            universal,
            candidates
        );
    }
    assert!(dups.is_empty(), "静态归类表有重名:{dups:?}");
}
