//! 顶层 · 接入项目。两张卡:先指到一个真实的仓,再说清「这是个什么项目」。
//!
//! 版式照 `docs/v4-prototype/hifi/index.html` 的 `renderOnboard`。高保真上那份仓
//! 列表是工厂造的假数据(`REPO_LIST`);这里列的是**真的** —— 点「列出我的仓」
//! 现去问 `gh repo list` / `codehub-cli`(两个平台的能力 `bw-engine` 里本来就有,
//! V3 就在用)。问不出来就把 CLI 的原话摆出来,绝不拿假数据顶上。
//!
//! **点一行仓会自动回显名片**:去读那个仓远端的 `.bw/project.toml`,读得到就说明
//! 这个项目已经被 buddy 接管过(你自己接过、或者同事先接的),四个字段直接填好;
//! 读不到就空着让人填。**人只要动手改过某一格,回显就不再盖它** —— 以人填的为准。
//!
//! 四个基础字段全部落仓文件(`PROJECT.md` 与 `.bw/project.toml`),库里只记路径
//! 与显示用的名字 —— 名片的正本在仓里,换台机器拉下来就有。

use crate::bridge::Bridge;
use crate::chrome::light_dot;
use crate::vm::{ToolProbeVm, Vm};
use bw_v4::command::{Command, ProjectIntent, RemoteRef};
use bw_v4::Signal as HealthSignal;
use dioxus::prelude::*;

/// 「我账号下的仓」列表。**没点过就不去问** —— 每次开接入屏都自动起一次子进程
/// 太吵,而且没装 gh 的人会天天看见一条红。点了才问,问不出来就摆原话。
#[allow(clippy::too_many_arguments)]
fn repo_picker(
    vm: &Vm,
    bridge: &Bridge,
    github: bool,
    host: String,
    remote: Signal<String>,
    workspace: Signal<String>,
    root: String,
) -> Element {
    let r = &vm.repos;
    let b = bridge.clone();
    let (gh2, host2) = (github, host.clone());
    rsx! {
        div { class: "repolist",
            div { style: "display:flex;align-items:center;gap:8px;margin-bottom:8px;",
                button {
                    class: "btn btn-sm",
                    disabled: r.loading,
                    onclick: move |_| b.send(crate::bridge::Req::ListRepos {
                        github: gh2,
                        host: host2.clone(),
                    }),
                    if r.loading { "列着…" } else if r.asked { "重列一次" } else { "列出我的仓" }
                }
                span { style: "font-size:11.5px;color:var(--ink-3);",
                    if github { "现去问 gh repo list" } else { "现去问 codehub-cli" }
                }
            }
            if let Some(e) = &r.error {
                div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin-top:6px;",
                    "列不出来:{e}"
                    br {}
                    if github {
                        "多半是没装 gh 或者没登录(gh auth login)。地址也可以直接填在下面那格。"
                    } else {
                        "多半是没装 codehub-cli、没登录,或者上面的域名填错了。"
                    }
                }
            } else if r.loading {
                div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin-top:6px;", "正在问平台…" }
            } else if r.asked && r.rows.is_empty() {
                div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin-top:6px;", "这个账号下一个仓都没列到。" }
            } else {
                for row in r.rows.iter() {
                    {repo_line(row, r.picked.as_deref(), bridge, github, &host, remote, workspace, &root)}
                }
            }
        }
    }
}

/// 一行仓。点它 = 把地址填进下面那格 + 猜一个本机路径 + 去读它的名片。
#[allow(clippy::too_many_arguments)]
fn repo_line(
    row: &crate::vm::RepoRowVm,
    picked: Option<&str>,
    bridge: &Bridge,
    github: bool,
    host: &str,
    mut remote: Signal<String>,
    mut workspace: Signal<String>,
    root: &str,
) -> Element {
    let b = bridge.clone();
    let (path, host2) = (row.path.clone(), host.to_string());
    // 本机路径只是**猜**一个:工作区根目录 + 仓名。人可以改;这个目录还不存在
    // 的话,接入时 buddy 会把它建出来(clone 得你自己先做)。
    let guess = format!(
        "{}/{}",
        root.trim_end_matches('/'),
        row.path.rsplit('/').next().unwrap_or("")
    );
    let is_picked = picked == Some(row.path.as_str());
    rsx! {
        div {
            key: "{row.path}",
            class: if is_picked { "repo-row sel" } else { "repo-row" },
            onclick: move |_| {
                remote.set(path.clone());
                if workspace.read().trim().is_empty() {
                    workspace.set(guess.clone());
                }
                b.send(crate::bridge::Req::PickRepo {
                    github,
                    host: host2.clone(),
                    path: path.clone(),
                });
            },
            div { style: "display:flex;align-items:center;gap:7px;min-width:0;",
                span { class: "mono", "{row.path}" }
                if row.private { span { class: "chip chip-outline", "私有" } }
                if !row.default_branch.is_empty() {
                    span { class: "chip chip-outline mono", "{row.default_branch}" }
                }
            }
            if !row.description.is_empty() {
                span {
                    style: "font-size:11px;color:var(--ink-3);margin-left:10px;flex:1;min-width:0;\
                            overflow:hidden;text-overflow:ellipsis;white-space:nowrap;text-align:right;",
                    "{row.description}"
                }
            }
        }
    }
}

/// 人填了就用人填的,没填就用回显那份。**空白 = 没填**,不是「人特意清空了」——
/// 接入屏这四格本来就没有「特意留空」的用法。
fn pick(typed: &str, fallback: &str) -> String {
    let t = typed.trim();
    if t.is_empty() {
        fallback.trim().to_string()
    } else {
        t.to_string()
    }
}

/// 输入框里该显示什么:同上,但保留人正在打的原样(不 trim,不然打空格就被吃掉)。
fn shown(typed: &str, fallback: &str) -> String {
    if typed.is_empty() {
        fallback.to_string()
    } else {
        typed.to_string()
    }
}

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
    let root2 = root.clone();
    // 从远端仓读回来的名片。**人没动过的格子才用它** —— 见下面 `shown`。
    let pf = vm.repos.prefill.clone().unwrap_or_default();
    let (pf_name, pf_brief, pf_bench, pf_star) = (
        pf.name.clone(),
        pf.brief.clone(),
        pf.benchmark.clone(),
        pf.north_star.clone(),
    );
    let submit = move |_| {
        // 人填的优先,没填就用从远端仓回显的那份。
        let eff_name = pick(&name.read(), &pf_name);
        let s = if slug.read().trim().is_empty() {
            slugify(&eff_name)
        } else {
            slug.read().trim().to_string()
        };
        if s.is_empty() || eff_name.is_empty() {
            return;
        }
        let r = remote.read().trim().to_string();
        let gh = *github.read();
        b.cmd(Command::CreateProject {
            slug: s,
            intent: ProjectIntent {
                name: eff_name,
                brief: pick(&brief.read(), &pf_brief),
                benchmark: pick(&benchmark.read(), &pf_bench),
                north_star: pick(&north_star.read(), &pf_star),
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
                        {repo_picker(vm, bridge, *github.read(), host.read().clone(), remote, workspace, root2.clone())}
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
                    if vm.repos.prefilling {
                        div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin-top:6px;", "正在读这个仓的名片…" }
                    } else if vm.repos.prefill.is_some() {
                        div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin-top:6px;",
                            "这个仓已经被 buddy 接管过,下面四格是从它的 .bw/project.toml 回显的。"
                            br {}
                            "直接改就行 —— 你改过的格子以你为准,接进来时也不会覆盖仓里已有的字。"
                        }
                    } else if vm.repos.picked.is_some() {
                        div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.8;margin-top:6px;",
                            "这个仓还没被 buddy 接管过(远端读不到 .bw/project.toml),四格要你自己填。"
                        }
                    }
                    div { class: "formgrid2",
                        div { class: "formrow",
                            label { class: "label", "项目名称" }
                            input {
                                class: "input", value: "{shown(&name.read(), &pf.name)}",
                                placeholder: "例如 WorkflowHub",
                                oninput: move |e| name.set(e.value()),
                            }
                        }
                        div { class: "formrow",
                            label { class: "label", "最像的对标" }
                            input {
                                class: "input", value: "{shown(&benchmark.read(), &pf.benchmark)}",
                                placeholder: "Linear",
                                oninput: move |e| benchmark.set(e.value()),
                            }
                        }
                    }
                    div { class: "formrow",
                        label { class: "label", "你想做什么" }
                        textarea {
                            class: "textarea", value: "{shown(&brief.read(), &pf.brief)}",
                            placeholder: "把 agent 会话里长出的工作流沉淀成可复用资产",
                            oninput: move |e| brief.set(e.value()),
                        }
                    }
                    div { class: "formrow",
                        label { class: "label", "三个月长成什么样(北极星)" }
                        textarea {
                            class: "textarea", value: "{shown(&north_star.read(), &pf.north_star)}",
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
