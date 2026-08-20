//! 顶层 · 接入项目。**两步**:先指到一个真实的仓,再说清「这是个什么项目」。
//!
//! 为什么是两步而不是并排两张卡:第二步的四个字段**取决于第一步选了哪个仓** ——
//! 那个仓要是已经被 buddy 接管过,名片是回显出来的,人一个字都不用填。并排摆着
//! 就等于让人对着一堆空格子先填一遍、选完仓再看见它被覆盖。
//!
//! 选仓用的是**一个输入框**:点「刷新」把你账号下的仓拉进来,然后在框里打字即
//! 检索、点一行即选中;也可以完全不理列表,直接把 `owner/repo` 打进去。这跟 V3
//! 的接入流是同一个交互(`app-desktop/src/screens/create.rs` 的 `RepoCombobox`),
//! 别再发明第二种。
//!
//! 列表是**真的** —— `gh repo list` / `codehub-cli` 现问(两个平台的 `list_repos`
//! 在 `bw-engine` 里本来就有,V3 一直在用)。问不出来就把 CLI 的原话摆出来,绝不
//! 拿假数据顶上;高保真里那份 `REPO_LIST` 是工厂造的,不抄。
//!
//! 四个基础字段全部落仓文件(`PROJECT.md` 与 `.bw/project.toml`),库里只记路径
//! 与显示用的名字 —— 名片的正本在仓里,换台机器拉下来就有。

use crate::bridge::{Bridge, Req};
use crate::chrome::light_dot;
use crate::vm::{RepoRowVm, ToolProbeVm, Vm};
use bw_v4::command::{Command, ProjectIntent, RemoteRef};
use bw_v4::Signal as HealthSignal;
use dioxus::prelude::*;

/// 下拉里最多画几行。检索是在**已经拉下来的那批**里过滤,不是再去问平台。
const DROPDOWN_CAP: usize = 30;

#[component]
pub fn View(vm: Vm, bridge: Bridge, close: EventHandler<MouseEvent>) -> Element {
    let (vm, bridge) = (&vm, &bridge);
    // 第 1 步 = 项目地址,第 2 步 = 基础信息。
    let mut step = use_signal(|| 1u8);
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
    // 从远端仓读回来的名片。**人没动过的格子才用它** —— 见 `shown` / `pick`。
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

    // 第一步至少要指到一个仓:「已有」得有本机路径或远端地址,「新建」随时可走
    // (目录名留空会按项目名生成,而项目名是第二步的事)。
    let can_next = !*existing_tab.read()
        || !workspace.read().trim().is_empty()
        || !remote.read().trim().is_empty();
    let at_addr = *step.read() == 1;

    rsx! {
        section {
            div { class: "ob-head",
                h1 { style: "font-size:20px;margin:0;", "接入项目" }
                button { class: "btn btn-ghost btn-sm", onclick: close, "← 项目墙" }
            }

            div { class: "ob-steps",
                span { class: if at_addr { "ob-step active" } else { "ob-step done" }, "① 项目地址" }
                span { class: "ob-step-line" }
                span { class: if at_addr { "ob-step" } else { "ob-step active" }, "② 基础信息" }
            }

            div { class: "ob-wrap",
                if at_addr {
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

                        if *existing_tab.read() {
                            div { class: "formrow",
                                label { class: "label",
                                    if *github.read() { "远端仓(打字即检索,也可以直接填 owner/repo)" }
                                    else { "远端仓(打字即检索,也可以直接填 命名空间/仓名)" }
                                }
                                RepoField {
                                    vm: vm.clone(),
                                    bridge: bridge.clone(),
                                    github: *github.read(),
                                    host: host.read().clone(),
                                    root: root.clone(),
                                    remote,
                                    workspace,
                                }
                            }
                            div { class: "formrow",
                                label { class: "label", "本机仓路径" }
                                input {
                                    class: "input mono", value: "{workspace}",
                                    placeholder: "/Users/you/projects/loop-buddy",
                                    oninput: move |e| workspace.set(e.value()),
                                }
                            }
                            div { class: "ob-note",
                                "选中一个仓会顺手猜一个本机路径,不对就改。"
                                br {}
                                "仓里已经有 .bw/project.toml 就以它为准 —— 同事先接过这个项目的话,"
                                "你填的名片字段只补空着的,一个字都不覆盖。"
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
                            div { class: "ob-note",
                                "「新建」只在本机把目录建出来并写进名片。远端仓要你自己先建好 —— "
                                "buddy 还不会替你在平台上开仓,也就没有公开/私有可选。"
                                br {}
                                "远端留空也能用 —— 没挂远端的项目一样能建活、能干活,只是没有 MR 可评审。"
                            }
                        }

                        {probe_row(&vm.env, *github.read())}
                    }

                    div { class: "ob-actions",
                        button {
                            class: "btn btn-primary",
                            disabled: !can_next,
                            title: if can_next { "" } else { "先选一个仓,或者把本机仓路径填上" },
                            onclick: move |_| step.set(2),
                            "下一步:基础信息 →"
                        }
                    }
                } else {
                    div { class: "card ob-card",
                        h3 { "基础信息" }
                        div { class: "ob-note",
                            "要接的仓:"
                            span { class: "mono",
                                if remote.read().trim().is_empty() { "(没挂远端)" } else { "{remote}" }
                            }
                            " · 本机 "
                            span { class: "mono",
                                if workspace.read().trim().is_empty() { "(按工作区根目录放)" } else { "{workspace}" }
                            }
                        }
                        if vm.repos.prefilling {
                            div { class: "ob-note", "正在读这个仓的名片…" }
                        } else if vm.repos.prefill.is_some() {
                            div { class: "ob-note",
                                "这个仓已经被 buddy 接管过,下面四格是从它的 .bw/project.toml 回显的。"
                                br {}
                                "直接改就行 —— 你改过的格子以你为准,接进来时也不会覆盖仓里已有的字。"
                            }
                        } else if vm.repos.picked.is_some() {
                            div { class: "ob-note",
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
                        div { class: "ob-note",
                            "这四个字段会写进仓里的 PROJECT.md 与 .bw/project.toml —— 换台机器拉下来就有,"
                            "不是只存在你这台电脑上。"
                        }
                    }

                    div { class: "ob-actions",
                        button {
                            class: "btn btn-ghost",
                            onclick: move |_| step.set(1),
                            "← 上一步"
                        }
                        button { class: "btn btn-primary", onclick: submit, "完成接入" }
                    }
                }

                if !vm.projects.is_empty() {
                    div { class: "ob-note",
                        "已接入 {vm.projects.len()} 个项目。接入之后记得在配置屏点「规范铺底」,"
                        "把管理体系写进这个仓。"
                    }
                }
            }
        }
    }
}

/// 选仓的那一格:**一个输入框**。点「刷新」把你账号下的仓拉进来,打字即在拉下来
/// 的那批里检索,点一行即选中;不想理列表就直接把地址打进去,一样算数。
///
/// 交互照 V3 的 `RepoCombobox`(`app-desktop/src/screens/create.rs`)—— 原生
/// `<select>` 做不了「打字过滤」,而「搜索框 + 下拉框」两个控件是把一件事拆成两下。
#[component]
fn RepoField(
    vm: Vm,
    bridge: Bridge,
    github: bool,
    host: String,
    root: String,
    remote: Signal<String>,
    workspace: Signal<String>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut open = use_signal(|| false);
    let r = &vm.repos;

    let q = query.read().trim().to_lowercase();
    let shown_rows: Vec<RepoRowVm> = r
        .rows
        .iter()
        .filter(|row| {
            q.is_empty()
                || row.path.to_lowercase().contains(&q)
                || row.description.to_lowercase().contains(&q)
        })
        .take(DROPDOWN_CAP)
        .cloned()
        .collect();

    let b_list = bridge.clone();
    let (gh_l, host_l) = (github, host.clone());
    rsx! {
        div { style: "position:relative;",
            div { style: "display:flex;gap:8px;align-items:center;",
                input {
                    class: "input mono",
                    style: "flex:1;min-width:0;",
                    value: "{remote}",
                    placeholder: "forcegravity1989/loop-buddy",
                    onfocus: move |_| open.set(true),
                    oninput: move |e| {
                        // 打的字既是检索词,也是最终的地址 —— 不想用列表的人
                        // 直接打完就走,不用先点开什么。
                        remote.set(e.value());
                        query.set(e.value());
                        open.set(true);
                    },
                    onblur: move |_| open.set(false),
                }
                button {
                    class: "btn btn-sm",
                    disabled: r.loading,
                    title: if github { "去问 gh repo list" } else { "去问 codehub-cli" },
                    onclick: move |_| {
                        open.set(true);
                        b_list.send(Req::ListRepos { github: gh_l, host: host_l.clone() });
                    },
                    if r.loading { "刷新中…" } else { "刷新" }
                }
            }

            if let Some(e) = &r.error {
                div { class: "ob-note",
                    "列不出来:{e}"
                    br {}
                    if github { "多半是没装 gh 或者没登录(gh auth login)。地址直接打进上面那格也一样能用。" }
                    else { "多半是没装 codehub-cli、没登录,或者域名填错了。地址直接打进上面那格也一样能用。" }
                }
            } else if r.asked && !r.loading && r.rows.is_empty() {
                div { class: "ob-note", "这个账号下一个仓都没列到。" }
            } else if !r.asked {
                div { class: "ob-note", "点「刷新」把你账号下的仓拉进来,然后在上面那格打字挑。" }
            }

            if *open.read() && !r.rows.is_empty() {
                div { class: "repo-pop",
                    if shown_rows.is_empty() {
                        div { class: "ob-note", style: "padding:10px 12px;margin:0;",
                            "拉到了 {r.rows.len()} 个仓,没有匹配「{query}」的。"
                        }
                    } else {
                        for row in shown_rows.iter() {
                            {repo_option(row, &bridge, github, &host, &root, remote, workspace)}
                        }
                    }
                }
            }
        }
    }
}

/// 下拉里的一行。用 `onmousedown` 而不是 `onclick`:`onblur` 会先一步把下拉关掉,
/// 用 `onclick` 的话这一下永远点不中。
#[allow(clippy::too_many_arguments)]
fn repo_option(
    row: &RepoRowVm,
    bridge: &Bridge,
    github: bool,
    host: &str,
    root: &str,
    mut remote: Signal<String>,
    mut workspace: Signal<String>,
) -> Element {
    let b = bridge.clone();
    let (path, host2) = (row.path.clone(), host.to_string());
    // 本机路径只是**猜**一个:工作区根目录 + 仓名。人可以改。
    let guess = format!(
        "{}/{}",
        root.trim_end_matches('/'),
        row.path.rsplit('/').next().unwrap_or("")
    );
    rsx! {
        div {
            key: "{row.path}",
            class: "repo-opt",
            onmousedown: move |e| {
                e.prevent_default();
                remote.set(path.clone());
                if workspace.read().trim().is_empty() {
                    workspace.set(guess.clone());
                }
                b.send(Req::PickRepo { github, host: host2.clone(), path: path.clone() });
            },
            div { style: "display:flex;align-items:center;gap:7px;min-width:0;",
                span { class: "mono", "{row.path}" }
                if row.private { span { class: "chip chip-outline", "私有" } }
                if !row.default_branch.is_empty() {
                    span { class: "chip chip-outline mono", "{row.default_branch}" }
                }
            }
            if !row.description.is_empty() {
                div { class: "repo-opt-desc", "{row.description}" }
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
