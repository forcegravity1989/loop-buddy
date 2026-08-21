# 适配模块 · Claude CLI

## 借自哪个项目 / 文件

- `crates/bw-engine/src/claude_bin.rs` —— 候选路径探测(Windows 上认 `claude.cmd`,
  这条是 V3 试用期真踩出来的)。
- `crates/bw-engine/src/interactive_cli.rs` —— 交互式启动计划(`--append-system-prompt`
  + 位置参数提示词)与 PTY 后端。
- Orca(见 `docs/archive/v4-prototype/research/orca.md` §2(d)、§5(B))—— agent 状态判定
  的思路。

## 借了什么

- **状态判定改成 CLI 主动上报**:靠官方 hooks / statusLine 回传,不去猜终端输出
  里那几行字是什么意思。buddy 本来就有 hook 回收,对齐即可。
- 候选路径探测与 `.cmd` 走 `cmd.exe /c` 的处理,原样沿用。

## 没借什么

- **不抄 Orca 的多 agent 归一化层**。buddy 今天只有一个真实能跑的开工工具,
  为「将来可能有很多种 agent」先建一层抽象,是替还不存在的问题写代码。
- 不抄它的会话面板布局 —— 界面按 V4 自己的六入口走。
