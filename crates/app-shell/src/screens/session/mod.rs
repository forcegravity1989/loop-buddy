//! 项目内 · 会话。三栏:左边按活列会话,中间终端 / 文件 / diff 三个页签,
//! 右边文件树 · 改动文件 · git 状态 · MR 卡。
//!
//! 两条如实:
//!
//! 1. **左列只显示「运行中 / 空闲」两态**。「等你输入」这种细粒度状态要靠
//!    claude 的 hook 回传,还没接 —— 唯一真实的信号是 PTY 进程还在不在,
//!    所以只显示这一个,不猜。
//! 2. **终端跨屏不卸载**。切到别的屏再切回来,agent 中间说的话还在:终端
//!    没被焦点隐藏,只是挪到屏外去了,字节照收。

use crate::adapters::terminal_xterm::TerminalWidget;
use crate::bridge::{Bridge, Req};
use crate::theme;
use crate::vm::{ProjectVm, SessionTab, SessionVm};
use bw_v4::command::Command;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    let open = p
        .session_open
        .and_then(|id| p.sessions.iter().find(|s| s.issue_id == id));

    rsx! {
        div {
            style: "display:flex;gap:14px;align-items:stretch;height:calc(100vh - 150px);",
            // ── 左:会话列表 ──
            div {
                style: "width:230px;flex:none;{theme::card()}padding:12px;overflow:auto;",
                div {
                    style: "font-size:12px;color:{theme::INK_3};margin-bottom:10px;",
                    "会话 · 一件活最多一个"
                }
                if p.sessions.is_empty() {
                    div {
                        style: "font-size:12px;color:{theme::INK_4};line-height:1.9;",
                        "还没有会话。到计划屏点一张活的 ▶跑,这里就会多一行。"
                    }
                }
                for s in p.sessions.iter() {
                    {session_row(s, p.session_open == Some(s.issue_id), bridge)}
                }
            }

            // ── 中:终端 / 文件 / diff ──
            div {
                style: "flex:1;min-width:0;display:flex;flex-direction:column;gap:10px;",
                {top_bar(open, bridge)}
                {tabs(p, bridge)}
                {middle(p)}
            }

            // ── 右:文件树 / 改动 / git / MR ──
            div {
                style: "width:260px;flex:none;{theme::card()}padding:12px;overflow:auto;",
                {right_column(p, bridge)}
            }
        }

        // 终端本体挂在这里,**不随页签卸载**:切走只是移到屏外,字节照收。
        // 每个活着的会话各挂一个,焦点那个显示在中栏的槽位里。
        for s in p.sessions.iter() {
            TerminalWidget {
                key: "{s.conversation_id:?}",
                conversation_id: s.conversation_id,
                focused: p.session_open == Some(s.issue_id) && p.workbench.tab == SessionTab::Terminal,
                bridge: bridge.clone(),
            }
        }
    }
}

fn session_row(s: &SessionVm, active: bool, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let id = s.issue_id;
    let bg = if active {
        theme::CARD_ALT
    } else {
        "transparent"
    };
    // 只有两态,而且两态都是真的:进程在不在。
    let (dot_color, state) = if s.live {
        (theme::CLAY, "运行中")
    } else {
        (theme::INK_4, "空闲")
    };
    rsx! {
        div {
            key: "{s.issue_id:?}",
            style: "padding:9px 8px;border-radius:6px;cursor:pointer;background:{bg};margin-bottom:4px;",
            onclick: move |_| b.send(Req::SelectSession(Some(id))),
            div {
                style: "display:flex;align-items:center;gap:6px;margin-bottom:3px;",
                span { style: "{theme::dot(dot_color, 7)}" }
                span {
                    style: "font-family:{theme::MONO};font-size:10px;color:{theme::INK_4};",
                    "#{s.issue_number}"
                }
                span { style: "font-size:10px;color:{theme::INK_4};margin-left:auto;", "{state}" }
            }
            div {
                style: "font-size:12px;color:{theme::INK_2};line-height:1.5;",
                "{s.issue_title}"
            }
            div {
                style: "font-family:{theme::MONO};font-size:10px;color:{theme::INK_4};margin-top:3px;",
                "{s.branch}"
            }
        }
    }
}

/// 顶部一行:活标题 + 状态 + 三个动作。
///
/// **没有「完成」按钮**。会话屏里能做的最远是「推到评审」;点完成在通知屏,
/// 那是人看过 diff 之后的动作。
fn top_bar(open: Option<&SessionVm>, bridge: &Bridge) -> Element {
    let Some(s) = open else {
        return rsx! {
            div {
                style: "{theme::card()}padding:12px 14px;font-size:12px;color:{theme::INK_3};",
                "左边选一个会话。"
            }
        };
    };
    let (b_run, b_stop, b_review) = (bridge.clone(), bridge.clone(), bridge.clone());
    let (id_run, id_stop, id_review) = (s.issue_id, s.issue_id, s.issue_id);
    rsx! {
        div {
            style: "{theme::card()}padding:10px 14px;display:flex;align-items:center;gap:10px;",
            span {
                style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_4};",
                "#{s.issue_number}"
            }
            span { style: "font-size:14px;", "{s.issue_title}" }
            span {
                style: "{theme::chip(theme::CARD_ALT, theme::INK_2)}",
                "{s.issue_status}"
            }
            div { style: "margin-left:auto;display:flex;gap:8px;",
                button {
                    style: "{theme::btn_ghost()}",
                    onclick: move |_| b_run.cmd(Command::RunIssue { id: id_run }),
                    "▶ 开工"
                }
                button {
                    style: "{theme::btn_ghost()}",
                    onclick: move |_| b_stop.cmd(Command::CancelRun { id: id_stop }),
                    "■ 停止"
                }
                button {
                    style: "{theme::btn_primary()}",
                    onclick: move |_| b_review.cmd(Command::TransitionIssue {
                        id: id_review,
                        to: bw_v4::model::IssueStatus::InReview,
                    }),
                    "推到评审"
                }
            }
        }
    }
}

fn tabs(p: &ProjectVm, bridge: &Bridge) -> Element {
    let cur = p.workbench.tab;
    let file_label = if p.workbench.open_path.is_empty() {
        "打开的文件".to_string()
    } else {
        p.workbench.open_path.clone()
    };
    rsx! {
        div {
            style: "display:flex;gap:6px;",
            {tab_btn("终端", SessionTab::Terminal, cur, bridge)}
            {tab_btn(&file_label, SessionTab::File, cur, bridge)}
            {tab_btn("改动 diff", SessionTab::Diff, cur, bridge)}
        }
    }
}

fn tab_btn(label: &str, tab: SessionTab, cur: SessionTab, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let active = tab == cur;
    let (bg, fg) = if active {
        (theme::CARD, theme::INK)
    } else {
        ("transparent", theme::INK_3)
    };
    let label = label.to_string();
    rsx! {
        div {
            style: "padding:5px 12px;border-radius:6px 6px 0 0;cursor:pointer;font-size:12px;\
                    background:{bg};color:{fg};border:1px solid {theme::BORDER};border-bottom:none;\
                    max-width:280px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
            onclick: move |_| b.send(Req::SessionTab(tab)),
            "{label}"
        }
    }
}

/// 中栏正文槽。终端页签下留一个空槽 —— 真正的终端挂在组件树末尾,靠 CSS
/// 定位覆盖上来,这样切页签不会把它卸载掉。
fn middle(p: &ProjectVm) -> Element {
    let w = &p.workbench;
    if w.tab == SessionTab::Terminal {
        let hint = if p.session_open.is_none() {
            "选一个会话,终端出现在这里。"
        } else {
            ""
        };
        return rsx! {
            div {
                style: "flex:1;min-height:0;{theme::card()}padding:0;display:flex;\
                        align-items:center;justify-content:center;color:{theme::INK_4};font-size:12px;",
                "{hint}"
            }
        };
    }
    let body = if w.open_path.is_empty() {
        "右边点一个文件。".to_string()
    } else {
        w.open_body.clone()
    };
    rsx! {
        div {
            style: "flex:1;min-height:0;{theme::card()}padding:14px 16px;overflow:auto;",
            pre {
                style: "margin:0;font-family:{theme::MONO};font-size:11px;line-height:1.7;\
                        color:{theme::INK_2};white-space:pre-wrap;word-break:break-all;",
                "{body}"
            }
        }
    }
}

fn right_column(p: &ProjectVm, bridge: &Bridge) -> Element {
    let w = &p.workbench;
    if p.session_open.is_none() {
        return rsx! {
            div {
                style: "font-size:12px;color:{theme::INK_4};line-height:1.9;",
                "选中一个会话之后,这里显示它那个工作区的文件树、改动、分支状态和 MR。"
            }
        };
    }
    let ab = match w.ahead_behind {
        // 问不出来就说问不出来。显示 0 会让人以为「和主干一样」。
        None => "—".to_string(),
        Some((a, b)) => format!("领先 {a} · 落后 {b}"),
    };
    let dirty = if w.dirty {
        "有未提交的改动"
    } else {
        "干净"
    };
    let pr = if w.pr_number == 0 {
        "还没有 MR".to_string()
    } else {
        format!("MR #{}", w.pr_number)
    };
    rsx! {
        div {
            style: "font-size:11px;color:{theme::INK_3};line-height:1.9;margin-bottom:12px;\
                    font-family:{theme::MONO};",
            div { "分支 {w.branch}" }
            div { "{ab} · {dirty}" }
            div { "{pr}" }
        }

        div { style: "font-size:12px;color:{theme::INK_3};margin:14px 0 6px;", "改动的文件" }
        if w.changed.is_empty() {
            div { style: "font-size:11px;color:{theme::INK_4};", "没有改动。" }
        }
        for c in w.changed.iter() {
            {changed_row(c, bridge)}
        }

        div { style: "font-size:12px;color:{theme::INK_3};margin:16px 0 6px;", "文件树" }
        {tree_level(p, "", 0, bridge)}
    }
}

fn changed_row(c: &crate::vm::ChangedFileVm, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let path = c.path.clone();
    rsx! {
        div {
            key: "{c.path}",
            style: "display:flex;gap:6px;align-items:baseline;padding:3px 4px;border-radius:4px;\
                    cursor:pointer;font-size:11px;",
            onclick: move |_| b.send(Req::OpenFile { path: path.clone(), diff: true }),
            span { style: "color:{theme::CLAY};flex:none;", "{c.label}" }
            span {
                style: "font-family:{theme::MONO};color:{theme::INK_2};word-break:break-all;",
                "{c.path}"
            }
        }
    }
}

/// 一层目录。展开着的目录才往下递归 —— 没展开的那些根本没读过盘。
fn tree_level(p: &ProjectVm, dir: &str, depth: usize, bridge: &Bridge) -> Element {
    let Some((_, entries)) = p.workbench.tree.iter().find(|(d, _)| d == dir) else {
        return rsx! {};
    };
    let pad = 8 + depth * 10;
    rsx! {
        for e in entries.iter() {
            {tree_row(p, e, pad, depth, bridge)}
        }
    }
}

fn tree_row(
    p: &ProjectVm,
    e: &crate::vm::TreeEntryVm,
    pad: usize,
    depth: usize,
    bridge: &Bridge,
) -> Element {
    let b = bridge.clone();
    let rel = e.rel.clone();
    let is_dir = e.is_dir;
    let expanded = is_dir && p.workbench.expanded.contains(&e.rel);
    let mark = if !is_dir {
        " "
    } else if expanded {
        "▾"
    } else {
        "▸"
    };
    rsx! {
        div {
            key: "{e.rel}",
            div {
                style: "padding:2px 4px 2px {pad}px;border-radius:4px;cursor:pointer;font-size:11px;\
                        font-family:{theme::MONO};color:{theme::INK_2};word-break:break-all;",
                onclick: move |_| {
                    if is_dir {
                        b.send(Req::ToggleDir(rel.clone()));
                    } else {
                        b.send(Req::OpenFile { path: rel.clone(), diff: false });
                    }
                },
                "{mark} {e.name}"
            }
            if expanded {
                {tree_level(p, &e.rel, depth + 1, bridge)}
            }
        }
    }
}
