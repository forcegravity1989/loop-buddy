//! 项目群适配工厂。
//!
//! 项目在聊天工具里本来就有一个群,buddy 只做两件事:**往群里发一条**、
//! **定计划时把上周群消息拉出来当参考**。不做双向机器人、不做群里点按钮改
//! 活状态(那会绕开「完成永远由人点」)、不做多群路由。
//!
//! # 为什么是 trait + 工厂,不是一个 enum
//!
//! `bw-engine` 里 `Remote::for_project` 是同一条纪律的先例:**认哪个提供方
//! 只在工厂这一处分支,调用点只调方法**。但 `Remote` 两个变体的字段几乎一样
//! (host + path),塞一个 enum 正合适;聊天工具天生更杂——群号 / channel ID /
//! chat_id,认证方式各不相同,而且第一版就要装「未配置」和「本机自测」两个
//! 不是真实提供方的实现。所以这里用 `Box<dyn ChatGroup>`。
//!
//! # 「拉不了历史」是正常返回值,不是异常
//!
//! 六家聊天工具里,钉钉 / 企业微信 / Teams 的群机器人**只能发不能读**。
//! 接口必须把这件事当成诚实的正常状态([`ChatError::HistoryUnsupported`]),
//! 调用方看到它要安静跳过——不重试、不报错、不在界面上闪一个红条。
//!
//! # 发出去就算完,不记账
//!
//! 没有「已发送」账本表,没有去重键,没有失败重试队列。代价如实:极小概率下
//! (进程恰好在发送前后重启)同一件事可能被重复推一条进群。**重发一条能忍**,
//! 为它建一张表不值。失败也只在当次界面上说一次,不假装有一套可靠的恢复机制。

pub mod mock;
pub mod none;
pub mod welink;

use time::OffsetDateTime;

/// 一条群消息。发消息与拉历史共用一个类型。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatMessage {
    pub time: Option<OffsetDateTime>,
    pub sender: Option<String>,
    /// 所有提供方共同的兜底。能发富文本的额外填 [`Self::link`] / [`Self::markdown`]。
    pub text: String,
    /// (标题, URL)。
    pub link: Option<(String, String)>,
    pub markdown: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChatError {
    /// 这个项目没配群。**不算失败**,调用方直接跳过。
    #[error("这个项目没配项目群")]
    NotConfigured,
    /// 这个提供方天生拉不了历史。也不算失败。
    #[error("这个提供方拉不了群历史")]
    HistoryUnsupported,
    /// 凭证/权限问题,原文带上,绝不吞掉伪装成功。
    #[error("群鉴权失败:{0}")]
    Auth(String),
    #[error("发群网络失败:{0}")]
    Network(String),
}

#[async_trait::async_trait]
pub trait ChatGroup: Send + Sync {
    /// 必须实现。
    async fn send(&self, msg: &ChatMessage) -> Result<(), ChatError>;

    /// 尽力实现。做不到就稳定返回 [`ChatError::HistoryUnsupported`],
    /// 不 panic、不抛别的错。
    async fn fetch_history(
        &self,
        since: OffsetDateTime,
        until: OffsetDateTime,
    ) -> Result<Vec<ChatMessage>, ChatError>;

    /// 「测一下这个项目配的群号对不对」。可选——没实现就是没这个能力,
    /// 界面显示「该提供方未提供测活能力」,不崩。
    async fn probe(&self) -> Result<String, ChatError> {
        Err(ChatError::NotConfigured)
    }
}

/// 认提供方**只在这一处**。调用点一律只拿 `Box<dyn ChatGroup>` 调方法。
///
/// 认不出的名字一律落到 [`none`] —— 不猜、不报错,就是「没配群」。
pub fn for_project(provider: &str, group_id: &str) -> Box<dyn ChatGroup> {
    match provider.trim() {
        "welink" => Box::new(welink::WelinkChatGroup::new(group_id)),
        "mock" => Box::new(mock::MockChatGroup::new(group_id)),
        _ => Box::new(none::NoneChatGroup),
    }
}

/// 哪些事默认同步到群。`.bw/project.toml` 的 `[chat] notify` 没写就是这三样。
pub const DEFAULT_NOTIFY: [&str; 3] = ["review", "merged", "release"];

/// 事件类型 → 中文标签。认不出的原样返回,不替它编一个名字。
pub fn event_label(event: &str) -> &str {
    match event {
        "review" => "评审中",
        "merged" => "已合入",
        "release" => "发版",
        other => other,
    }
}

/// 一行文案:`【<事件>】#<活号> <标题> · <MR 状态> · <谁该动> → <深链>`。
///
/// **不 @ 人**:buddy 不建成员,没有身份映射可 @。
/// `number == 0` 表示这条不挂在某张活上(发版就是这样),此时不写活号、也不
/// 写深链——写一个 `#0` 和一条点不开的链接比不写更糟。
pub fn notify_text(event: &str, number: u32, title: &str, mr: &str, who: &str) -> String {
    let mut s = if number == 0 {
        format!("【{}】{title}", event_label(event))
    } else {
        format!("【{}】#{number} {title}", event_label(event))
    };
    if !mr.is_empty() {
        s.push_str(&format!(" · {mr}"));
    }
    if !who.is_empty() {
        s.push_str(&format!(" · {who}"));
    }
    if number > 0 {
        s.push_str(&format!(" → bw://open?issue={number}"));
    }
    s
}
