//! 项目群工厂。
//!
//! 核心只有两个动作:**发一条消息到群**、**拉一段时间的群历史**。提供方今天
//! 只有两个:内部 WeLink(同事在做,还没接)与「没配群」。
//!
//! **不做发送去重账本**:调用即完成,不写库、不查重。极小概率下同一条消息会被
//! 重发一次 —— 这是知情接受的代价,不是 bug。

/// 发一条消息到群。
pub trait ChatGroup: Send + Sync {
    /// 提供方名字,和 `.bw/project.toml` 的 `[chat] provider` 对得上。
    fn provider(&self) -> &'static str;

    /// 发一条。返回 `Err` 就是真失败 —— 不重试、不记录、不阻塞别的通知继续发。
    fn send(&self, group_id: &str, text: &str) -> Result<(), String>;
}

/// 没配群。所有发送调用直接跳过,流程正常走完,不报错。
pub struct NoChatGroup;

impl ChatGroup for NoChatGroup {
    fn provider(&self) -> &'static str {
        "none"
    }

    fn send(&self, _group_id: &str, _text: &str) -> Result<(), String> {
        // 没配群就是没配群,跳过发送不是失败。
        Ok(())
    }
}

/// 按 `.bw/project.toml` 里的提供方挑一个实现。
///
/// WeLink 的真实现还没接 —— 这里如实退回「没配群」,不假装发出去了。
pub fn factory(provider: &str) -> Box<dyn ChatGroup> {
    match provider {
        "welink" => {
            eprintln!("[chat_group] welink 的真实现还没接,这次不发送");
            Box::new(NoChatGroup)
        }
        _ => Box::new(NoChatGroup),
    }
}
