//! `app-desktop` 的库面——**目前只导出一样东西**:[`escape_hatch`]。
//!
//! 为什么要有一个 `lib.rs`(壳原本可以是纯 `[[bin]]`):
//! `examples/seed_shell_demo.rs`(深链验证的造数脚本)要证明逃生舱的续
//! 接命令字符串是真的由 [`escape_hatch::build`] 算出来的,不是脚本自己
//! 另抄一遍格式化逻辑——两份格式化代码分开写,迟早会在某次改动里悄悄
//! 分叉(其中一处改了参数顺序,另一处忘了跟),那正是「同一条规则,第
//! 二本账」这个坑。示例(`examples/`)只能看见 crate 的**公开库面**,看
//! 不见 `main.rs` 里的私有模块——所以把 [`escape_hatch`] 单独摆成这个库
//! 面唯一的公开面,`main.rs`(`[[bin]] bw-next`)与
//! `examples/seed_shell_demo.rs` 两边都经这一条路径调用同一份代码。
//! `kernel`/`screens`/`theme` 三个模块仍然只在 `main.rs` 里 `mod`,不经
//! 这个库面——它们没有第二个调用方需要复用它们。

pub mod escape_hatch;
