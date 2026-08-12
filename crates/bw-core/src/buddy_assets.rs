//! buddy 系统提示词与规范的打包正本。`include_str!` 在编译期读取,
//! 保持 bw-core 的零 IO/wasm32 约束——与 `bw_library` 的技能正文打包同构。
//!
//! `SYSTEM_PROMPT_MD` 是公共提示词静态段(身份 + 铁律 + 规范索引),
//! 动态项目上下文由 buddy 代码在运行时追加。`STANDARDS_*` 是规范正文,
//! 由 `buddy_materialize` 物化到工作区 `.claude/buddy/standards/` 供 Claude 按需 Read。

/// 公共系统提示词正本。所有 Issue 首次启动注入,恢复不重灌(§7.1)。
pub const SYSTEM_PROMPT_MD: &str = include_str!("../../../docs/buddy/system-prompt.md");

/// 指标格式规范正本。物化到 `.claude/buddy/standards/metrics.md`。
pub const STANDARDS_METRICS_MD: &str = include_str!("../../../docs/buddy/standards/metrics.md");

/// 连接器格式规范正本。物化到 `.claude/buddy/standards/connectors.md`。
pub const STANDARDS_CONNECTORS_MD: &str =
    include_str!("../../../docs/buddy/standards/connectors.md");

/// 所有需要物化到工作区的 buddy 规范文件。每项 = (相对 `.claude/buddy/` 的路径, 正文)。
pub fn standards_files() -> [(&'static str, &'static str); 2] {
    [
        ("standards/metrics.md", STANDARDS_METRICS_MD),
        ("standards/connectors.md", STANDARDS_CONNECTORS_MD),
    ]
}

/// 所有规范正文拼接后的指纹,用作 `.bw-managed` 标记里的版本。
/// 内容不变则指纹不变,整条跳过不重写;buddy 升级后内容变化则指纹改变,原位更新。
pub fn standards_fingerprint() -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (_, content) in standards_files() {
        content.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}
