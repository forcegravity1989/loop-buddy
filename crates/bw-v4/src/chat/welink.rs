//! WeLink 群。**buddy 这边只留位,三个函数的真实实现由内部同事补**。
//!
//! # 给接手的同事
//!
//! - **要实现哪三个**:`send` 必须实现;`fetch_history` 尽力(WeLink 开放平台
//!   如果读不了群历史,就稳定返回 [`ChatError::HistoryUnsupported`],**不要**
//!   抛别的错、不要 panic);`probe` 可选但建议做——项目群配置卡片上那颗
//!   「测一下」按钮就靠它回答「这个项目配的群号对不对」。
//! - **错误怎么带**:WeLink SDK / HTTP 的原始错误文本原样塞进
//!   [`ChatError::Auth`] / [`ChatError::Network`]。**绝不吞掉错误伪装成功** ——
//!   这是全仓「读回为证」纪律在这一层的落点。
//! - **登录态不归 buddy 管**:用户在本机用官方渠道自己登好(一次基本永久),
//!   buddy 不碰、不存这份凭证,设置屏也没有「聊天工具登录」这颗按钮。机器上
//!   登没登好由项目墙那条「本机环境 · 测一下」里的 welink-cli 探活回答;群号
//!   对不对由这里的 `probe` 回答。两件互补的事,都不需要登录按钮。
//! - **不用真群怎么自测**:先对着 [`super::mock`] 把 buddy 这边的调用链跑通,
//!   再换真实凭证验证 `send` / `fetch_history` 本身。
//! - **鉴权方式、群标识形态、已知限制**:实现时补写在这段模块文档里,写法参照
//!   `bw-engine` 的 `codehub.rs` 顶部那段。今天还没有一手信息,**空着,不编**。

use super::{ChatError, ChatGroup, ChatMessage};
use time::OffsetDateTime;

pub struct WelinkChatGroup {
    #[allow(dead_code)] // 真实实现补上后就用得上;今天留着是为了让签名先定下来。
    group_id: String,
}

impl WelinkChatGroup {
    pub fn new(group_id: &str) -> Self {
        Self {
            group_id: group_id.to_string(),
        }
    }
}

/// 三个函数一律如实说「还没接」。**不静默成功**——假装发出去了比发不出去更坏。
const NOT_IMPLEMENTED: &str = "WeLink 群还没接上(crates/bw-v4/src/chat/welink.rs 待内部同事实现)";

#[async_trait::async_trait]
impl ChatGroup for WelinkChatGroup {
    async fn send(&self, _msg: &ChatMessage) -> Result<(), ChatError> {
        Err(ChatError::Network(NOT_IMPLEMENTED.into()))
    }

    async fn fetch_history(
        &self,
        _since: OffsetDateTime,
        _until: OffsetDateTime,
    ) -> Result<Vec<ChatMessage>, ChatError> {
        Err(ChatError::HistoryUnsupported)
    }
}
