//! 项目内 · 会话。A 刀先做真实数据的最简列表:按活列出已经有过会话的那些。
//!
//! 内嵌终端(xterm + PTY)、文件树、diff 页签是下一刀的事。**这里不摆假数据**
//! ——没有会话就说没有,不画一个空的终端框假装它能用。

use crate::theme;
use crate::vm::ProjectVm;
use dioxus::prelude::*;

pub fn view(p: &ProjectVm) -> Element {
    rsx! {
        div {
            style: "max-width:900px;margin:0 auto;",
            div { style: "font-family:{theme::SERIF};font-size:20px;margin-bottom:6px;", "会话" }
            div {
                style: "color:{theme::INK_3};font-size:12px;margin-bottom:18px;line-height:1.8;",
                "一件交互式的活最多一个会话。这里列的是库里真实存在的会话行(活 ↔ claude 会话 ↔ worktree ↔ 分支),\
                 重启之后靠它恢复。"
            }
            if p.sessions.is_empty() {
                div {
                    style: "{theme::card()}padding:34px;text-align:center;color:{theme::INK_3};\
                            font-size:13px;line-height:2;",
                    "还没有任何会话。"
                    br {}
                    "去计划屏点一张活的「▶ 开工」,跑起来之后这里会出现它的会话。"
                }
            }
            for s in p.sessions.iter() {
                {session_row(s)}
            }
            div {
                style: "margin-top:24px;{theme::not_built()}",
                "内嵌终端、文件树、diff 页签还没建。这一屏现在只如实显示会话记录,不放占位的终端框。"
            }
        }
    }
}

fn session_row(s: &crate::vm::SessionVm) -> Element {
    let branch = dash_if_blank(&s.branch);
    let ws = dash_if_blank(&s.workspace_path);
    let sid = if s.session_id.is_empty() {
        "—(还没捕获到)"
    } else {
        &s.session_id
    };
    rsx! {
        div {
            key: "{s.issue_id:?}",
            style: "{theme::card()}padding:16px;margin-bottom:12px;",
            div {
                style: "display:flex;gap:8px;align-items:baseline;margin-bottom:8px;",
                span { style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_4};", "#{s.issue_number}" }
                span { style: "font-size:14px;", "{s.issue_title}" }
            }
            div {
                style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_3};line-height:1.9;",
                div { "分支:{branch}" }
                div { "工作区:{ws}" }
                div { "claude 会话 id:{sid}" }
            }
        }
    }
}

fn dash_if_blank(s: &str) -> &str {
    if s.is_empty() {
        "—"
    } else {
        s
    }
}
