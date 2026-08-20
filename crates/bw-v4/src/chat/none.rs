//! 「没配群」。存在的意义是让**没配群时别的流程不会崩**这件事可验证:
//! 发也好、拉也好,一律返回 [`ChatError::NotConfigured`],调用方安静跳过。

use super::{ChatError, ChatGroup, ChatMessage};
use time::OffsetDateTime;

pub struct NoneChatGroup;

#[async_trait::async_trait]
impl ChatGroup for NoneChatGroup {
    async fn send(&self, _msg: &ChatMessage) -> Result<(), ChatError> {
        Err(ChatError::NotConfigured)
    }

    async fn fetch_history(
        &self,
        _since: OffsetDateTime,
        _until: OffsetDateTime,
    ) -> Result<Vec<ChatMessage>, ChatError> {
        Err(ChatError::NotConfigured)
    }
}
