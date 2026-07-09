//! `Hub::Connector` — data source connectors: a 3-column card grid, matching
//! the prototype's Connectors hub. Real store-backed records; this app has no
//! actual live sync mechanism yet (Tier D — `Connector::pull()` producing
//! real observations), so a connector recorded here is a real reference
//! entry, not a live integration.

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::ConnectorId;
use dioxus::prelude::*;

#[component]
pub fn ConnectorHub(hub: HubVm) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.connectors.len();

    let mut creating = use_signal(|| false);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "CONNECTORS" }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin:4px 0 18px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "数据连接器" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 数据源" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 接入数据源" }
                }
            }
            if creating() {
                CreateConnectorForm { on_done: move |_| creating.set(false) }
            }
            if hub.connectors.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有连接器——点「+ 接入数据源」录入第一个。" }
            } else {
                div {
                    style: "display:grid;grid-template-columns:repeat(3,1fr);gap:14px;",
                    for c in hub.connectors.clone() {
                        ConnectorCard { key: "{c.id.uuid()}", c }
                    }
                }
            }
        }
    }
}

#[component]
fn ConnectorCard(c: ui::vm::ConnectorCardVm) -> Element {
    let card = theme::card();
    let ink3 = theme::INK_3;
    let chip = theme::chip("#EFE9DA", theme::INK_2);
    rsx! {
        div {
            style: "{card} padding:16px 18px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:10px;",
                div {
                    style: "width:32px;height:32px;border-radius:8px;background:{theme::CARD_ALT};border:1px solid {theme::BORDER};color:{theme::INK_2};display:flex;align-items:center;justify-content:center;font-family:{theme::SERIF};font-weight:700;font-size:13px;flex:none;",
                    "{c.initial}"
                }
                div { style: "flex:1;min-width:0;",
                    div { style: "font-size:13.5px;font-weight:500;", "{c.name}" }
                    div { style: "font-size:11px;color:{ink3};", "{c.kind}" }
                }
                span { style: "{chip}", "{c.status_label}" }
            }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;font-size:11.5px;color:{ink3};",
                span { "{c.scope}" }
                span { "{c.last_sync}" }
            }
        }
    }
}

#[component]
fn CreateConnectorForm(on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();

    let mut name = use_signal(String::new);
    let mut kind = use_signal(String::new);
    let mut scope = use_signal(String::new);

    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        k.send(Command::CreateConnector {
            id: ConnectorId::new(),
            name: n,
            kind: kind().trim().to_string(),
            scope: scope().trim().to_string(),
        });
        name.set(String::new());
        kind.set(String::new());
        scope.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 Datadog / GitHub / Slack",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "类型" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 可观测性 / 代码仓库 / 通知",
                value: "{kind}",
                oninput: move |e| kind.set(e.value()),
            }
            div { style: "{label}", "作用范围" }
            input {
                style: "{input} margin-bottom:12px;",
                placeholder: "如 全部项目 / 具体项目名",
                value: "{scope}",
                oninput: move |e| scope.set(e.value()),
            }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                onclick: save,
                "保存"
            }
        }
    }
}
