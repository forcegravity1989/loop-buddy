//! `Hub::Notify` — the human-in-the-loop feed: read-only, no table of its
//! own. Every row is a real status that already flipped elsewhere in the hub
//! library (see `ui::vm::notify_feed`); there is no "mark as read" because
//! there is nothing hand-authored to dismiss — the row disappears once the
//! underlying status does.

use crate::kernel::HubVm;
use crate::theme;
use dioxus::prelude::*;
use ui::vm::NotifyLevel;

#[component]
pub fn NotifyHub(hub: HubVm) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let card = theme::card();
    let alert_chip = theme::chip(theme::ALERT_DEEP, "#FFF");
    let done_chip = theme::chip("#E7EDE2", "#5F7355");
    let n = hub.notifications.len();

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "NOTIFY" }
            div {
                style: "display:flex;align-items:baseline;gap:10px;margin:4px 0 6px;",
                span { style: "font-family:{serif};font-size:22px;font-weight:600;", "通知" }
                span { style: "font-size:12.5px;color:{ink3};", "{n} 条" }
            }
            p { style: "font-size:12.5px;color:{ink2};max-width:46em;margin:0 0 18px;",
                "human-in-the-loop 的入口：失败的定时任务、异常的连接器、需要留意的阶段交接都汇聚于此。"
            }
            if hub.notifications.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "暂无待办——所有定时任务、连接器与交接都正常。" }
            } else {
                for item in hub.notifications.clone() {
                    div {
                        key: "{item.title}-{item.time_label}",
                        style: "{card} padding:14px 18px;margin-bottom:8px;display:flex;align-items:flex-start;gap:14px;",
                        div {
                            style: if item.level == NotifyLevel::Alert { "{alert_chip}" } else { "{done_chip}" },
                            "{item.level.label()}"
                        }
                        div { style: "flex:1;min-width:0;",
                            div { style: "font-size:13.5px;font-weight:500;", "{item.title}" }
                            div { style: "font-size:12px;color:{ink3};margin-top:3px;", "{item.detail}" }
                        }
                        span { style: "font-size:11px;color:{ink3};white-space:nowrap;", "{item.time_label}" }
                    }
                }
            }
        }
    }
}
