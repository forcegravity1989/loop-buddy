//! 编排层的错误类型。目前只包一层存储错误;运行管理器落地后(下一任务)
//! 会长出「这件活已经有一个交付运行在跑」「执行连接器没找到/判定不了」等
//! 编排层自己的判断——那些错误变体到那时候再加,现在不预置用不上的分支。

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] bw_store::StoreError),
}
