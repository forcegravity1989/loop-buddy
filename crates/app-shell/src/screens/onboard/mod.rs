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
//!
//! **「完成接入」按下去会发生什么**(母文档 §2 第 0 站):把仓弄到本机(该 clone
//! 就 clone)→ 库里落一行项目 → 读/写仓里的名片 → 自动建一张运作活③「规范铺底」,
//! 它写规范骨架、开一条分支、提一个 MR,**停在评审中等人合** → 进这个项目。
//! 每一步都往那条 broadcast 通道报一行,界面上原地覆盖 —— 这一下要十几秒,不报
//! 进度人会以为没点上、然后猛点。

use crate::bridge::{Bridge, Req};
use crate::chrome::{light_dot, progress_log};
use crate::vm::{RepoProbe, RepoRowVm, ToolProbeVm, Vm};
use bw_v4::app::ProgressLine;
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

    // ── 「完成接入」按下之后,一步一句地把后台在干什么摆出来 ──────────
    // 内核那条队列此刻正卡在 clone 上,ViewModel 一动不动,所以这些行是从
    // 旁边那条 broadcast 直接来的(和内嵌终端同一个路子)。
    let mut log = use_signal(Vec::<ProgressLine>::new);
    // 按下那一刻的回执序号。命令做完(不管成没成)序号会变 —— 用它判断
    // 「还在做吗」,不用再造一套状态。做成了这屏会被自动关掉,看不到。
    let mut submitted_at = use_signal(|| None::<u64>);
    let busy = *submitted_at.read() == Some(vm.note_seq);

    let prog = bridge.progress.clone();
    use_future(move || {
        let mut rx = prog.subscribe();
        async move {
            while let Ok(line) = rx.recv().await {
                // 按步号原地覆盖:「正在 clone…」被「clone 好了」换掉,
                // 而不是堆成两行。
                let mut rows = log.write();
                match rows.iter().position(|r: &ProgressLine| r.step == line.step) {
                    Some(i) => rows[i] = line,
                    None => rows.push(line),
                }
            }
        }
    });

    let b = bridge.clone();
    let b_next = bridge.clone();
    let vm_seq = vm.note_seq;
    let vm_probed = vm.repos.picked.clone();
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
        log.write().clear();
        submitted_at.set(Some(vm_seq));
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
                            onclick: move |_| {
                                // 人可能压根没点列表、直接把地址打进去了 —— 那样
                                // 「这个仓接管过没有」根本没查过。走到第二步之前
                                // 补一次:盯着的地址和已经查过的那个不一样就查。
                                let addr = remote.read().trim().to_string();
                                let asked = vm_probed.clone();
                                if !addr.is_empty() && asked.as_deref() != Some(addr.as_str()) {
                                    b_next.send(Req::PickRepo {
                                        github: *github.read(),
                                        host: host.read().trim().to_string(),
                                        path: addr,
                                        // 手打的地址不知道默认分支,交给引擎兜底。
                                        git_ref: String::new(),
                                    });
                                }
                                step.set(2);
                            },
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
                        {adopted_note(&vm.repos.probe)}
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

                    {progress_log(&log.read())}

                    div { class: "ob-actions",
                        button {
                            class: "btn btn-ghost",
                            disabled: busy,
                            onclick: move |_| step.set(1),
                            "← 上一步"
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: busy,
                            onclick: submit,
                            if busy { "接入中…" } else { "完成接入" }
                        }
                    }
                }

                if !vm.projects.is_empty() {
                    div { class: "ob-note",
                        "已接入 {vm.projects.len()} 个项目。接入会顺手把管理体系写进这个仓 ——"
                        "建一张「规范铺底」的活、开一条分支、提一个 MR,停在评审中等你合。"
                    }
                }
            }
        }
    }
}

/// 选仓的那一格:**一个输入框**。光标点进去就自动去列你账号下的仓,打字即在拉下来
/// 的那批里检索,点一行即选中;不想理列表就直接把地址打进去,一样算数。想重新拉一次,
/// 点框里右边那个 ⟳。
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
    let b_focus = bridge.clone();
    let (gh_l, host_l) = (github, host.clone());
    let (gh_f, host_f) = (github, host.clone());
    // 还没问过、也不在问的路上 —— 光标一进这格就替人去问一次,不用先点什么。
    let need_first_fetch = !r.asked && !r.loading;
    rsx! {
        div { style: "position:relative;",
            div { class: "repo-field",
                input {
                    class: "input mono",
                    value: "{remote}",
                    placeholder: "forcegravity1989/loop-buddy",
                    onfocus: move |_| {
                        open.set(true);
                        if need_first_fetch {
                            b_focus.send(Req::ListRepos { github: gh_f, host: host_f.clone() });
                        }
                    },
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
                    class: if r.loading { "repo-refresh spinning" } else { "repo-refresh" },
                    disabled: r.loading,
                    title: if github { "重新问一次 gh repo list" } else { "重新问一次 codehub-cli" },
                    // 按下去不能让上面那格失焦 —— 一失焦下拉就收了,人会以为
                    // 刷新反而把列表刷没了。
                    onmousedown: move |e| {
                        e.prevent_default();
                        open.set(true);
                        b_list.send(Req::ListRepos { github: gh_l, host: host_l.clone() });
                    },
                    "⟳"
                }
            }

            if let Some(e) = &r.error {
                div { class: "ob-note",
                    "列不出来:{e}"
                    br {}
                    if github { "多半是没装 gh 或者没登录(gh auth login)。地址直接打进上面那格也一样能用。" }
                    else { "多半是没装 codehub-cli、没登录,或者域名填错了。地址直接打进上面那格也一样能用。" }
                }
            } else if r.loading {
                div { class: "ob-note",
                    if github { "正在问 gh repo list…" } else { "正在问 codehub-cli…" }
                }
            } else if r.asked && r.rows.is_empty() {
                div { class: "ob-note", "这个账号下一个仓都没列到。地址直接打进上面那格也一样能用。" }
            } else if !r.asked {
                div { class: "ob-note", "光标点进上面那格,就去列你账号下的仓;也可以直接把地址打进去。" }
            }

            if *open.read() && !r.rows.is_empty() {
                div { class: "repo-pop",
                    if shown_rows.is_empty() {
                        div { class: "ob-note", style: "padding:10px 12px;margin:0;",
                            "拉到了 {r.rows.len()} 个仓,没有匹配「{query}」的。"
                        }
                    } else {
                        for row in shown_rows.iter() {
                            {repo_option(row, &bridge, github, &host, &root, remote, workspace, query, open)}
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
    mut query: Signal<String>,
    mut open: Signal<bool>,
) -> Element {
    let b = bridge.clone();
    let (path, host2) = (row.path.clone(), host.to_string());
    // 列表这一行知道它的默认分支,带上 —— 默认分支不叫 main 的仓不带就查不到,
    // 结果会把一个接管过的仓报成「没接管过」。
    let git_ref = row.default_branch.clone();
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
                // 选完就收起来。这里按的是 mousedown 且拦了默认行为(不拦的话
                // 上面那格会先失焦、这一下就点空了),所以「失焦收起」那条走不到,
                // 必须自己收 —— 少这一句,选完下拉就关不上。
                open.set(false);
                // 检索词清掉:下次光标再进那格,看到的是整张列表,而不是被
                // 上一次选中的仓名筛得只剩一行。
                query.set(String::new());
                if workspace.read().trim().is_empty() {
                    workspace.set(guess.clone());
                }
                b.send(Req::PickRepo {
                    github,
                    host: host2.clone(),
                    path: path.clone(),
                    git_ref: git_ref.clone(),
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
                div { class: "repo-opt-desc", "{row.description}" }
            }
        }
    }
}

/// 「这个仓被 buddy 接管过没有」的几种结局,一句都不能混。
///
/// 判据是**去远端读 `.bw/project.toml` 读不读得到**(GitHub 走
/// `gh api .../contents/.bw/project.toml`,codehub 走 codehub-cli)。读到 = 接管过。
/// 麻烦在于**读不到有三种,平台回的都是 404**,得一层层剥开:
///
/// - 仓不在(地址敲错、私有仓没权限)→ 再问一次 `gh repo view` 就露馅 → 「找不到这个仓」
/// - 分支不在(默认分支不叫 main)→ 平台正文里那句「No commit found for the ref」→ 「没查成」
/// - 仓在、分支在、就是没这份文件 → 才是「没接管过」
///
/// 其余(没登录、网断了、文件写坏了)一律「没查成」。把任何一种说成「没接管过」,
/// 人就会照着空格子填一遍,接进去反而盖掉仓里真正的名片。
fn adopted_note(probe: &RepoProbe) -> Element {
    rsx! {
        div { class: "ob-note",
            match probe {
                RepoProbe::NotAsked => rsx! {
                    "没查这个仓被 buddy 接管过没有 —— 上一步没选/没填远端地址。四格要你自己填。"
                },
                RepoProbe::Loading => rsx! { "正在读这个仓的 .bw/project.toml…" },
                RepoProbe::Adopted => rsx! {
                    "这个仓"
                    strong { "已经被 buddy 接管过" }
                    ",下面四格是从它远端的 .bw/project.toml 回显的。"
                    br {}
                    "直接改就行 —— 你改过的格子以你为准,接进来时也不会覆盖仓里已有的字。"
                },
                RepoProbe::Absent => rsx! {
                    "这个仓"
                    strong { "还没被 buddy 接管过" }
                    "(平台明确回「没有 .bw/project.toml」),四格要你自己填。"
                },
                RepoProbe::NoRepo(e) => rsx! {
                    strong { "这个地址上找不到仓" }
                    " —— 原话:{e}"
                    br {}
                    "常见原因:地址敲错了、这是个私有仓而当前账号看不见、gh/codehub-cli 没登录。"
                    br {}
                    "先把地址改对再往下走 —— 接一个不存在的仓,后面每一步都会失败。"
                },
                RepoProbe::Failed(e) => rsx! {
                    strong { "没查成,所以不知道这个仓接管过没有" }
                    " —— 原话:{e}"
                    br {}
                    "常见原因:没登录、网不通、这个仓的默认分支不叫 main。"
                    br {}
                    "照样能接:仓里真有 .bw/project.toml 的话,接入时以仓里的为准,你填的只补空着的字段,一个字都不会被覆盖。"
                },
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
