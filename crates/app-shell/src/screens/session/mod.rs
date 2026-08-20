//! 项目内 · 会话。**结构照 `hifi/index.html` 的 `renderSession` 排**:三栏
//! 216 / 1fr / 264,左边按 worktree 分组列会话,中间动作条 + 页签 + 正文,
//! 右边文件树 · 改动文件 · git 盒 · MR 卡。
//!
//! 三条如实:
//!
//! 1. **左列只显示「运行中 / 空闲」两态**。「等你输入」这种细粒度状态要靠
//!    claude 的 hook 回传,还没接 —— 唯一真实的信号是 PTY 进程还在不在,
//!    所以只显示这一个,不猜。
//! 2. **终端本体不在这个文件里**。它挂在 `.content` 上(`main.rs`),因为挂在
//!    这里的话人一切面板就整屏卸载,收字节的循环跟着没,agent 那段时间说的话
//!    是真丢的。这个屏在中栏下半格留一个空格子,终端靠 `.content.session-mode`
//!    那套网格落进去 —— 所以中栏是上下两张卡拼的,不是原型里的一整张。
//! 3. **没有「完成」按钮**。会话屏里能做的最远是「推到评审」;点完成在计划屏
//!    或通知屏,那是人看过 diff 之后的动作。

use crate::bridge::{Bridge, Req};
use crate::vm::{ProjectVm, SessionTab, SessionVm};
use bw_v4::command::Command;
use bw_v4::model::Signal as HealthSignal;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    let open = p
        .session_open
        .and_then(|id| p.sessions.iter().find(|s| s.issue_id == id));
    rsx! {
        div { class: "sess-col sess-left",
            div { class: "sess-left-scroll",
                if p.sessions.is_empty() {
                    div { class: "detail-empty",
                        "还没有会话。到计划屏点一张活的 ▶开工,这里就会多一行。"
                    }
                }
                for s in p.sessions.iter() {
                    {session_row(s, p.session_open == Some(s.issue_id), bridge)}
                }
            }
        }
        div { class: "sess-col sess-midhead",
            {action_bar(open, bridge)}
            {tabs(p, bridge)}
        }
        {mid_body(p)}
        div { class: "sess-col sess-right",
            div { class: "sess-right-scroll", {right_column(p, bridge)} }
        }
    }
}

// ── 左栏 ────────────────────────────────────────────

fn session_row(s: &SessionVm, active: bool, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let id = s.issue_id;
    // 只有两态,而且两态都是真的:进程在不在。
    let (sig, state) = if s.live {
        (Some(HealthSignal::Green), "运行中")
    } else {
        (None, "空闲")
    };
    let dot_cls = if s.live { "dot dot-run" } else { "dot" };
    let color = crate::theme::signal_color(sig);
    rsx! {
        div { key: "{s.issue_id:?}",
            // 分组标签 = 这个会话在哪个 worktree、哪条分支上干活。
            div { class: "sess-group-label", title: "{s.workspace_path}", "{s.branch}" }
            div {
                class: if active { "sess-row sel" } else { "sess-row" },
                onclick: move |_| b.send(Req::SelectSession(Some(id))),
                span { class: "{dot_cls}", style: "background:{color};margin-top:4px;" }
                div { class: "body",
                    div { class: "ttl", "#{s.issue_number} {s.issue_title}" }
                    div { class: "sub", "{s.issue_status} · {state}" }
                }
            }
        }
    }
}

// ── 中栏上半:动作条 + 页签 ─────────────────────────

fn action_bar(open: Option<&SessionVm>, bridge: &Bridge) -> Element {
    let Some(s) = open else {
        return rsx! {
            div { class: "sess-actionbar",
                span { class: "ttl", "左边选一个会话" }
            }
        };
    };
    let (b_run, b_stop, b_review, b_submit) = (
        bridge.clone(),
        bridge.clone(),
        bridge.clone(),
        bridge.clone(),
    );
    let id = s.issue_id;
    rsx! {
        div { class: "sess-actionbar",
            span { class: "ttl", title: "#{s.issue_number} {s.issue_title}",
                "#{s.issue_number} {s.issue_title}"
            }
            span { class: "chip", "{s.issue_status}" }
            button {
                class: "btn btn-sm",
                disabled: s.live,
                onclick: move |_| b_run.cmd(Command::RunIssue { id }),
                "▶ 开工"
            }
            button {
                class: "btn btn-sm",
                disabled: !s.live,
                onclick: move |_| b_stop.cmd(Command::CancelRun { id }),
                "■ 停止"
            }
            button {
                class: "btn btn-sm btn-primary",
                title: "把这棵树里 agent 干出来的改动提交、推分支、开 MR,然后把活推到「评审中」。\
                        「完成」还是你评审完之后再点一次。",
                onclick: move |_| b_submit.cmd(Command::SubmitIssueWork { id }),
                "提交并开 MR"
            }
            button {
                class: "btn btn-sm",
                title: "只改状态,不碰仓 —— 给「改动不在这棵树里」或者「远端还没挂上」的情况留的。",
                onclick: move |_| b_review.cmd(Command::TransitionIssue {
                    id,
                    to: bw_v4::model::IssueStatus::InReview,
                }),
                "只推到评审"
            }
            // 高保真上这里还有「蒸馏」与「在 Cursor 中打开」。前者 V4 还没有
            // 这条命令,后者要按活的开工工具决定露不露 —— 都还没接,做成灰态。
            button { class: "btn btn-sm", disabled: true, title: "V4 还没接蒸馏", "蒸馏" }
        }
    }
}

fn tabs(p: &ProjectVm, bridge: &Bridge) -> Element {
    let cur = p.workbench.tab;
    let has_file = !p.workbench.open_path.is_empty();
    let file_label = if has_file {
        p.workbench.open_path.clone()
    } else {
        "文件".to_string()
    };
    rsx! {
        div { class: "sess-tabs",
            {tab_btn("终端", SessionTab::Terminal, cur, false, bridge)}
            {tab_btn(&file_label, SessionTab::File, cur, has_file, bridge)}
            {tab_btn("改动 diff", SessionTab::Diff, cur, has_file, bridge)}
        }
    }
}

fn tab_btn(
    label: &str,
    tab: SessionTab,
    cur: SessionTab,
    closable: bool,
    bridge: &Bridge,
) -> Element {
    let b = bridge.clone();
    let b_close = bridge.clone();
    let label = label.to_string();
    rsx! {
        div {
            class: if tab == cur { "sess-tab active" } else { "sess-tab" },
            onclick: move |_| b.send(Req::SessionTab(tab)),
            span {
                style: "max-width:220px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                "{label}"
            }
            if closable {
                span {
                    class: "x",
                    onclick: move |e| {
                        e.stop_propagation();
                        // 关掉 = 不再开着任何文件,回终端。
                        b_close.send(Req::OpenFile { path: String::new(), diff: false });
                        b_close.send(Req::SessionTab(SessionTab::Terminal));
                    },
                    "✕"
                }
            }
        }
    }
}

// ── 中栏下半:正文 ──────────────────────────────────

/// 终端页签下**什么都不渲染** —— 那个格子留给挂在 `.content` 上的终端本体。
/// 没选中会话时才摆一句话,否则会盖住终端。
fn mid_body(p: &ProjectVm) -> Element {
    let w = &p.workbench;
    if w.tab == SessionTab::Terminal {
        if p.session_open.is_some() {
            return rsx! {};
        }
        return rsx! {
            div { class: "sess-col sess-midbody",
                div { class: "detail-empty", style: "margin:auto;", "选一个会话,终端出现在这里。" }
            }
        };
    }
    if w.open_path.is_empty() {
        return rsx! {
            div { class: "sess-col sess-midbody",
                div { class: "detail-empty", style: "margin:auto;", "右边点一个文件。" }
            }
        };
    }
    if w.tab == SessionTab::Diff {
        return rsx! {
            div { class: "sess-col sess-midbody",
                div { class: "codeview-head", "{w.open_path} · diff" }
                div { class: "diffview", style: "border:none;border-radius:0;",
                    for (i, line) in w.open_body.lines().enumerate() {
                        div { key: "{i}", class: "diffline {diff_cls(line)}", "{line}" }
                    }
                }
            }
        };
    }
    rsx! {
        div { class: "sess-col sess-midbody",
            div { class: "codeview-head", "{w.open_path} · 只读" }
            div { class: "codeview-body",
                for (i, line) in w.open_body.lines().enumerate() {
                    div { key: "{i}", class: "codeline",
                        span { class: "ln", "{i + 1}" }
                        span { class: "src", "{line}" }
                    }
                }
            }
        }
    }
}

/// diff 每一行按前缀染色。**只认 git 自己的前缀**,不猜。
fn diff_cls(line: &str) -> &'static str {
    match line.as_bytes().first() {
        Some(b'+') => "add",
        Some(b'-') => "del",
        _ => "ctx",
    }
}

// ── 右栏 ────────────────────────────────────────────

fn right_column(p: &ProjectVm, bridge: &Bridge) -> Element {
    let w = &p.workbench;
    if p.session_open.is_none() {
        return rsx! {
            div { class: "detail-empty",
                "选中一个会话之后,这里显示它那个工作区的文件树、改动、分支状态和 MR。"
            }
        };
    }
    let ab = match w.ahead_behind {
        // 问不出来就说问不出来。显示 0 会让人以为「和主干一样」。
        None => "领先 — · 落后 —".to_string(),
        Some((a, b)) => format!("领先 {a} · 落后 {b}"),
    };
    let dirty = if w.dirty {
        "有未提交的改动"
    } else {
        "干净"
    };
    rsx! {
        div { class: "sr-h", "文件树" }
        {tree_level(p, "", 0, bridge)}

        div { class: "sr-h", "改动文件" }
        if w.changed.is_empty() {
            div { class: "detail-empty", style: "padding:6px 2px;", "无改动" }
        }
        for c in w.changed.iter() {
            {changed_row(c, bridge)}
        }

        div { class: "sr-h", "git" }
        div { class: "git-box",
            div { "分支 {w.branch}" }
            div { "{ab}" }
            div { "{dirty}" }
        }

        div { class: "sr-h", "MR" }
        if w.pr_number == 0 {
            div { class: "detail-empty", style: "padding:6px 2px;", "还没有 MR" }
        } else {
            div { class: "mr-card",
                div { class: "row",
                    span { class: "mono", "!{w.pr_number}" }
                    span { class: "chip chip-outline", "开着" }
                }
                div { style: "font-size:11px;color:var(--ink-3);",
                    "合入走通知屏的「合入并完成」——那一下才结清。"
                }
            }
        }
    }
}

fn changed_row(c: &crate::vm::ChangedFileVm, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let path = c.path.clone();
    rsx! {
        div {
            key: "{c.path}",
            class: "changed-row",
            onclick: move |_| b.send(Req::OpenFile { path: path.clone(), diff: true }),
            span { style: "word-break:break-all;", "{c.path}" }
            span { class: "diffnum", "{c.label}" }
        }
    }
}

/// 一层目录。展开着的目录才往下递归 —— 没展开的那些根本没读过盘。
fn tree_level(p: &ProjectVm, dir: &str, depth: usize, bridge: &Bridge) -> Element {
    let Some((_, entries)) = p.workbench.tree.iter().find(|(d, _)| d == dir) else {
        return rsx! {};
    };
    rsx! {
        for e in entries.iter() {
            {tree_row(p, e, depth, bridge)}
        }
    }
}

fn tree_row(p: &ProjectVm, e: &crate::vm::TreeEntryVm, depth: usize, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let rel = e.rel.clone();
    let is_dir = e.is_dir;
    let expanded = is_dir && p.workbench.expanded.contains(&e.rel);
    let changed = p.workbench.changed.iter().any(|c| c.path == e.rel);
    let pad = depth * 12 + if is_dir { 0 } else { 11 };
    let cls = if is_dir {
        "tree-node dir"
    } else if changed {
        "tree-node file changed"
    } else {
        "tree-node file"
    };
    rsx! {
        div { key: "{e.rel}",
            div {
                class: "{cls}",
                style: "padding-left:{pad}px;",
                onclick: move |_| {
                    if is_dir {
                        b.send(Req::ToggleDir(rel.clone()));
                    } else {
                        b.send(Req::OpenFile { path: rel.clone(), diff: false });
                    }
                },
                if is_dir {
                    span { class: "tree-arrow", {if expanded { "▾" } else { "▸" }} }
                    "{e.name}/"
                } else {
                    "{e.name}"
                    if changed { " ✱" }
                }
            }
            if expanded {
                {tree_level(p, &e.rel, depth + 1, bridge)}
            }
        }
    }
}
