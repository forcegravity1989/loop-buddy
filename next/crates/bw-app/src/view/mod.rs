//! 六段总控 + 待人处理的推导(design-s5-hexpanel.md §4.3):**纯函数**——
//! 输入是从存储读出来的行(+ 少数由调用方现采的外部证据)与一个传进来的
//! 时刻,输出是可直接渲染的数据结构。**不读时钟、不查库、不发命令**——
//! 查库在调用它们的用例里做(见 [`crate::App::hex_view`]/
//! [`crate::App::attention_view`])。
//!
//! 住编排层而不是新开一个界面 crate 的理由见 design §4.3——推导要能被指
//! 挥器(`bw-app/examples/hex_readback.rs`)直接调用,验的就是界面真正会
//! 用的那个函数,不是另一份平行发明的核对逻辑。

pub mod hex;

pub use hex::HexView;

// `attention` 模块(待人处理投影)是下一个 commit(切片五D)的事——本片
// (切片五C)只立六段总控这一半。
