//! 把三件事同步进项目群:活进评审、活合入、发了版。
//!
//! **发出去就算完**:不写库、不记「发过没发过」、不做去重、没有失败重试队列。
//! 极小概率下同一件事会被重推一条进群(比如进程恰好在发送前后重启),这是
//! 已经接受的代价——重发一条能忍,为它建一张表不值。
//!
//! **失败不回滚主干**:合入、结清、发版的账在这条链路之前就记完了,发群只是
//! 末尾一个锦上添花的旁支。发不出去就在当次界面上如实说一句,不假装有一套
//! 可靠的恢复机制,也不会在下一次 tick 时偷偷重发。

use super::{App, Result};
use crate::chat::{self, ChatError, ChatMessage};
use crate::command::Event;
use crate::model::{IssueId, ProjectId};

impl App {
    /// 界面/别的用例都走这一个入口。
    ///
    /// 返回 `None` 有两种情况,都**不是失败**:这个项目没配群,或者这件事不在
    /// `[chat] notify` 勾选的名单里。两种情况都安静跳过,不产生任何记录。
    pub(super) async fn chat_send(
        &self,
        project_id: ProjectId,
        event: &str,
        number: u32,
        text: String,
    ) -> Option<Event> {
        let ws = self.workspace_of(project_id).await.ok()?;
        let file = crate::repo::project_file::read(&ws).ok().flatten()?;
        let cfg = file.chat?;
        let provider = cfg.provider.trim();
        if provider.is_empty() || provider == "none" {
            return None;
        }
        // `notify` 没写就用默认三件事;写了就以写的为准(写了空数组 = 一件都不发)。
        let wanted: Vec<String> = if cfg.notify.is_empty() {
            chat::DEFAULT_NOTIFY.iter().map(|s| s.to_string()).collect()
        } else {
            cfg.notify.clone()
        };
        if !wanted.iter().any(|w| w == event) {
            return None;
        }

        let group = chat::for_project(provider, &cfg.group_id);
        let msg = ChatMessage {
            text,
            ..ChatMessage::default()
        };
        Some(match group.send(&msg).await {
            Ok(()) => Event::ChatNotifySent {
                number,
                event_type: event.to_string(),
                ok: true,
                note: String::new(),
            },
            // 没配群这一条走不到这里(上面已经短路),真走到就当没配处理。
            Err(ChatError::NotConfigured) => return None,
            Err(e) => Event::ChatNotifySent {
                number,
                event_type: event.to_string(),
                ok: false,
                note: e.to_string(),
            },
        })
    }

    /// 一张活的三类事件走这条:文案按模板拼,`who` 与 MR 状态能拿到就带上。
    pub(super) async fn chat_notify_issue(&self, id: IssueId, event: &str) -> Option<Event> {
        let issue = self.issue_or_err(id).await.ok()?;
        let mr = if issue.pr_number > 0 {
            format!("MR !{}", issue.pr_number)
        } else {
            String::new()
        };
        let who = match event {
            "review" => "该你看一眼",
            _ => "",
        };
        let text = chat::notify_text(event, issue.number, &issue.title, &mr, who);
        self.chat_send(issue.project_id, event, issue.number, text)
            .await
    }

    /// 界面显式触发的那条命令。项目没配群 / 这件事没勾选时如实回一条「跳过了」,
    /// 不静默——人是主动点的,得到的回音不能是沉默。
    pub(super) async fn sync_notify_to_chat(
        &mut self,
        issue_id: IssueId,
        event_type: String,
    ) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(issue_id).await?;
        Ok(vec![self
            .chat_notify_issue(issue_id, &event_type)
            .await
            .unwrap_or(Event::ChatNotifySent {
                number: issue.number,
                event_type,
                ok: false,
                note: "这个项目没配项目群,或者这件事没勾选同步 —— 没发,也没记账".into(),
            })])
    }
}
