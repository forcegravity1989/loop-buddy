//! 顶层 · 接入项目。两张卡:先指到一个真实的仓,再说清「这是个什么项目」。
//!
//! 版式照 `docs/v4-prototype/hifi/index.html` 的 `renderOnboard`。**高保真上那份
//! 仓列表是工厂造的假数据**(`REPO_LIST`),真壳里列不出来 —— 列远端的仓要调
//! 平台接口,那条路还没接。所以「已有」那一格给的是一个能直接填的地址输入,
//! 外加一句实话说明为什么没有列表可点。
//!
//! 四个基础字段全部落仓文件(`PROJECT.md` 与 `.bw/project.toml`),库里只记路径
//! 与显示用的名字 —— 名片的正本在仓里,换台机器拉下来就有。

use crate::bridge::Bridge;
use crate::chrome::light_dot;
use crate::vm::{ToolProbeVm, Vm};
use bw_v4::command::{Command, ProjectIntent, RemoteRef};
use bw_v4::Signal as HealthSignal;
use dioxus::prelude::*;

#[component]
pub fn View(vm: Vm, bridge: Bridge, close: EventHandler<MouseEvent>) -> Element {
    let (vm, bridge) = (&vm, &bridge);
    // 「新建」= buddy 只在本机把目录建出来;「已有」= 指到一个已经存在的仓。
    let mut existing_tab = use_signal(|| false);
    let mut github = use_signal(|| true);

    let mut name = use_signal(String::new);
    let mut brief = use_signal(String::new);
    let mut benchmark = use_signal(String::new);
    let mut north_star = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut workspace = use_signal(String::new);
    let mut remote = use_signal(String::new);
    let mut host = use_signal(String::new);

    let b = bridge.clone();
    let root = vm.settings.workspaces_root.clone();
    let submit = move |_| {
        let s = if slug.read().trim().is_empty() {
            slugify(&name.read())
        } else {
            slug.read().trim().to_string()
        };
        if s.is_empty() || name.read().trim().is_empty() {
            return;
        }
        let r = remote.read().trim().to_string();
        let gh = *github.read();
        b.cmd(Command::CreateProject {
            slug: s,
            intent: ProjectIntent {
                name: name.read().trim().to_string(),
                brief: brief.read().trim().to_string(),
                benchmark: benchmark.read().trim().to_string(),
                north_star: north_star.read().trim().to_string(),
            },
            remote: if r.is_empty() {
                RemoteRef::default()
            } else {
                RemoteRef {
                    provider: if gh {
                        "github".into()
                    } else {
                        "codehub".into()
                    },
                    host: if gh {
                        "github.com".into()
                    } else {
                        host.read().trim().to_string()
                    },
                    path: r,
                }
            },
            workspace_path: workspace.read().trim().to_string(),
        });
    };

    rsx! {
        section {
            div { class: "ob-head",
                h1 { style: "font-size:20px;margin:0;", "接入项目" }
                button { class: "btn btn-ghost btn-sm", onclick: close, "← 项目墙" }
            }
            div { class: "ob-cards",

                // ── 卡一:项目地址 ─────────────────────────
                div { class: "card ob-card",
                    h3 { "项目地址" }
                    div { class: "tabrow",
                        button {
                            class: if !*existing_tab.read() { "btn btn-sm active" } else { "btn btn-sm" },
                            onclick: move |_| existing_tab.set(false),
                            "新建"
                        }
                        button {
                            class: if *existing_tab.read() { "btn btn-sm active" } else { "btn btn-sm" },
                            onclick: move |_| existing_tab.set(true),
                            "已有"
                        }
                    }
                    div { class: "pillrow",
                        span {
                            class: if !*github.read() { "pill active" } else { "pill" },
                            onclick: move |_| github.set(false),
                            "codehub"
                        }
                        span {
                            class: if *github.read() { "pill active" } else { "pill" },
                            onclick: move |_| github.set(true),
                            "GitHub"
                        }
                    }

                    if *existing_tab.read() {
                        div { class: "formrow",
                            label { class: "label", "本机仓路径" }
                            input {
                                class: "input mono", value: "{workspace}",
                                placeholder: "/Users/you/projects/loop-buddy",
                                oninput: move |e| workspace.set(e.value()),
                            }
                        }
                        div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin:-4px 0 11px;",
                            "仓里已经有 .bw/project.toml 就以它为准 —— 同事先接过这个项目的话,\
                             你填的名片字段只补空着的,一个字都不覆盖。"
                        }
                        div { class: "repolist",
                            div { style: "padding:14px 10px;border:1px dashed var(--border);border-radius:7px;\
                                          font-size:11.5px;color:var(--ink-3);line-height:1.8;",
                                "列不出你在远端有哪些仓 —— 那要调平台接口,这条路还没接。"
                                br {}
                                "先把地址填在下面那格,或者把本机已经 clone 好的路径填在上面。"
                            }
                        }
                    } else {
                        div { class: "formgrid2",
                            div { class: "formrow",
                                label { class: "label", "目录名(留空按名称自动生成)" }
                                input {
                                    class: "input mono", value: "{slug}",
                                    placeholder: "workflowhub",
                                    oninput: move |e| slug.set(e.value()),
                                }
                            }
                            div { class: "formrow",
                                label { class: "label", "本机仓路径(留空用工作区根目录)" }
                                input {
                                    class: "input mono", value: "{workspace}",
                                    placeholder: "留空即 {root}/<目录名>",
                                    oninput: move |e| workspace.set(e.value()),
                                }
                            }
                        }
                        div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin-bottom:11px;",
                            "「新建」只在本机把目录建出来并写进名片。远端仓要你自己先建好 —— \
                             buddy 还不会替你在平台上开仓,也就没有公开/私有可选。"
                        }
                    }

                    div { class: "formgrid2",
                        div { class: "formrow",
                            label { class: "label",
                                if *github.read() { "远端仓(owner/repo,可留空)" } else { "远端仓(命名空间/仓名,可留空)" }
                            }
                            input {
                                class: "input mono", value: "{remote}",
                                placeholder: "forcegravity1989/loop-buddy",
                                oninput: move |e| remote.set(e.value()),
                            }
                        }
                        if !*github.read() {
                            div { class: "formrow",
                                label { class: "label", "codehub 域名" }
                                input {
                                    class: "input mono", value: "{host}",
                                    placeholder: "内部域名,buddy 不知道,得你填",
                                    oninput: move |e| host.set(e.value()),
                                }
                            }
                        }
                    }
                    div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;",
                        "远端留空也能用 —— 没挂远端的项目一样能建活、能干活,只是没有 MR 可评审。"
                    }

                    {probe_row(&vm.env, *github.read())}
                }

                // ── 卡二:基础信息 ─────────────────────────
                div { class: "card ob-card",
                    h3 { "基础信息" }
                    div { class: "formgrid2",
                        div { class: "formrow",
                            label { class: "label", "项目名称" }
                            input {
                                class: "input", value: "{name}",
                                placeholder: "例如 WorkflowHub",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "formrow",
                            label { class: "label", "最像的对标" }
                            input {
                                class: "input", value: "{benchmark}",
                                placeholder: "Linear",
                                oninput: move |e| benchmark.set(e.value()),
                            }
                        }
                    }
                    div { class: "formrow",
                        label { class: "label", "你想做什么" }
                        textarea {
                            class: "textarea", value: "{brief}",
                            placeholder: "把 agent 会话里长出的工作流沉淀成可复用资产",
                            oninput: move |e| brief.set(e.value()),
                        }
                    }
                    div { class: "formrow",
                        label { class: "label", "三个月长成什么样(北极星)" }
                        textarea {
                            class: "textarea", value: "{north_star}",
                            placeholder: "每月被标准工作流带完成的活数",
                            oninput: move |e| north_star.set(e.value()),
                        }
                    }
                    div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;",
                        "这四个字段会写进仓里的 PROJECT.md 与 .bw/project.toml —— 换台机器拉下来就有,\
                         不是只存在你这台电脑上。"
                    }
                }

                div { class: "ob-actions",
                    button { class: "btn btn-primary", onclick: submit, "完成接入" }
                }

                if !vm.projects.is_empty() {
                    div { style: "color:var(--ink-3);font-size:11.5px;line-height:1.8;",
                        "已接入 {vm.projects.len()} 个项目。接入之后记得在配置屏点「规范铺底」,\
                         把管理体系写进这个仓。"
                    }
                }
            }
        }
    }
}

/// 卡一底下那行探活。项目墙的环境条是同一份数据,这里只挑与接入直接相关的
/// 三项:平台 CLI、claude、Open Design。**探不出来的照实说探不出来。**
fn probe_row(env: &[ToolProbeVm], github: bool) -> Element {
    let pick = |name: &str| env.iter().find(|t| t.name == name);
    let platform = if github { "gh" } else { "codehub" };
    let items: Vec<&ToolProbeVm> = [platform, "claude_cli", "open_design"]
        .iter()
        .filter_map(|n| pick(n))
        .collect();
    rsx! {
        div { class: "probe3",
            for t in items {
                span {
                    key: "{t.name}",
                    style: "display:flex;align-items:center;gap:6px;",
                    title: "{t.detail}",
                    {light_dot(match t.ok {
                        Some(true) => Some(HealthSignal::Green),
                        Some(false) => Some(HealthSignal::Red),
                        None => None,
                    }, false)}
                    "{t.label}"
                }
            }
            span { style: "margin-left:auto;" }
            span { title: "这三项跟着整份界面数据一起重算,回项目墙点「测一下」即可",
                "探活在项目墙那条环境条上重测" }
        }
    }
}

/// 名称 → 目录名。中文与空格换成短横,认不出的字符丢掉。
fn slugify(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.chars().all(|c| c == '-') {
        String::new()
    } else {
        s
    }
}
