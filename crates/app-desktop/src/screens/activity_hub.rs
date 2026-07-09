//! `Hub::Activity` — cross-project audit feed: real `handoff` rows (newest
//! first), read-only. Matches the prototype's Activity hub (zero per-row
//! actions — "此处可审计每一次 loop") and this codebase's rule that `handoff`
//! is already the append-only source of truth, so this screen invents
//! nothing, it only reads `HubVm.activity`.

use crate::kernel::HubVm;
use crate::theme;
use dioxus::prelude::*;

#[component]
pub fn ActivityHub(hub: HubVm) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let card = theme::card();
    let chip = theme::chip(theme::ALERT_DEEP, "#FFF");
    let n = hub.activity.len();

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "ACTIVITY" }
            div {
                style: "display:flex;align-items:baseline;gap:10px;margin:4px 0 18px;",
                span { style: "font-family:{serif};font-size:22px;font-weight:600;", "活动" }
                span { style: "font-size:12.5px;color:{ink3};", "最近 {n} 次阶段交接 · 跨项目" }
            }
            if hub.activity.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有交接记录——项目跑完一段、点「交棒」后会出现在这里。" }
            } else {
                for a in hub.activity.clone() {
                    div {
                        key: "{a.project_id.uuid()}-{a.time_label}-{a.from_label}",
                        style: "{card} padding:14px 18px;margin-bottom:8px;display:flex;align-items:center;gap:14px;",
                        div { style: "flex:1;min-width:0;",
                            div { style: "font-size:13.5px;font-weight:500;", "{a.project_name}" }
                            div { style: "font-size:12px;color:{ink2};margin-top:2px;", "{a.from_label} → {a.to_label}" }
                            if !a.note.is_empty() {
                                div { style: "font-size:11.5px;color:{ink3};margin-top:2px;", "{a.note}" }
                            }
                        }
                        if a.risky {
                            span { style: "{chip}", "风险交接" }
                        }
                        span { style: "font-size:11px;color:{ink3};white-space:nowrap;", "{a.time_label}" }
                    }
                }
            }
        }
    }
}
