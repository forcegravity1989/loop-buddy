//! 本机自测用的假群。内存态,**不落库不进仓**。
//!
//! 发出去的每一条同时往 stderr 打一行 `[BW_CHAT_SENT] …` —— 和 `[BW_OPEN]`
//! 是同一条纪律:结构化 stderr 就是证据,E2E 直接 grep 它,不需要为了验收
//! 去建一张表。
//!
//! 拉历史返回**调用方提前塞好的**消息(`MockChatGroup::with_history`),
//! 不自己编内容。

use super::{ChatError, ChatGroup, ChatMessage};
use std::sync::Mutex;
use time::OffsetDateTime;

pub struct MockChatGroup {
    group_id: String,
    sent: Mutex<Vec<ChatMessage>>,
    history: Vec<ChatMessage>,
}

impl MockChatGroup {
    pub fn new(group_id: &str) -> Self {
        Self {
            group_id: group_id.to_string(),
            sent: Mutex::new(Vec::new()),
            history: Vec::new(),
        }
    }

    pub fn with_history(group_id: &str, history: Vec<ChatMessage>) -> Self {
        Self {
            group_id: group_id.to_string(),
            sent: Mutex::new(Vec::new()),
            history,
        }
    }

    /// 这一轮进程里发出去过哪些。给同进程的验收脚本用。
    pub fn sent(&self) -> Vec<ChatMessage> {
        self.sent.lock().map(|v| v.clone()).unwrap_or_default()
    }
}

#[async_trait::async_trait]
impl ChatGroup for MockChatGroup {
    async fn send(&self, msg: &ChatMessage) -> Result<(), ChatError> {
        // 一行一条,**不去重** —— 同一件事触发两次就打两行。验收要确认的正是
        // 「它确实会重复」,不是反过来验证「不会重复」。
        eprintln!("[BW_CHAT_SENT] group={} text={:?}", self.group_id, msg.text);
        if let Ok(mut v) = self.sent.lock() {
            v.push(msg.clone());
        }
        Ok(())
    }

    async fn fetch_history(
        &self,
        since: OffsetDateTime,
        until: OffsetDateTime,
    ) -> Result<Vec<ChatMessage>, ChatError> {
        Ok(self
            .history
            .iter()
            .filter(|m| match m.time {
                // 没有时间戳的消息一律留着 —— 丢掉不如留着,调用方看得见。
                None => true,
                Some(t) => t >= since && t < until,
            })
            .cloned()
            .collect())
    }

    async fn probe(&self) -> Result<String, ChatError> {
        Ok(format!(
            "假群 {} 可达(本机自测,没有真的连过网)",
            self.group_id
        ))
    }
}
