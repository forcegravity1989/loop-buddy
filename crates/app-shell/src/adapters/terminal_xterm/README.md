# 适配模块 · 内嵌终端(xterm.js + PTY)

## 借自哪个项目 / 文件

- `crates/app-desktop/src/screens/op/terminal_widget.rs` —— 整块搬过来的,是 V3 试用期
  一个坑一个坑填出来的那一份。
- `crates/bw-engine/src/pty_backend.rs` / `terminal_manager.rs` —— PTY 后端与按会话分
  的有界缓冲,原样沿用,一行没改。
- Orca 的 `use-terminal-pane-lifecycle.ts`(见 `docs/archive/v4-prototype/research/orca.md` §2a)
  —— 选区防抖 + 长度上限那一段的思路。

## 借了什么

- **xterm 资产随二进制走**。CDN 拉不下来的那一次,用户看到的是一整块空白。
- **跨屏切换不丢字节**。终端不用 `display:none` 藏(FitAddon 在 0×0 的框上 open 会
  「成功」但画布一直是空的),而是挪到屏外的固定尺寸框里,字节照收;切回来再
  re-home + re-fit + refresh。
- **UTF-8 断字处理**。PTY 字节流按约 100ms 切批,一个三字节汉字经常横跨两批;单独
  解一批会把两半都变成 U+FFFD。这里只取完整的前缀,尾巴留给下一批。
- **一次 drain 两个队列**。键盘输入和 resize 合并成一次 `document::eval` —— 每次
  eval 都是一整趟 IPC 往返。
- 尺寸同步链:`ResizeObserver` + 窗口 resize + 重新聚焦都触发 re-fit,`onResize` 把
  新尺寸攒起来,Rust 侧 30ms 取一次发 `TerminalResize`,PTY 那头跟着变。

## 没借什么

- **不借 Orca 的 `@xterm/headless` 回放机制**。它为了让终端内容能被 agent 读回去,
  在后台另跑一个无头终端做回放;buddy 不需要 —— 会话内容的正本是仓里的改动和 MR,
  不是终端里滚过的那些字。
- **不接 A 刀那一版的"会话内容存库"设想**。终端字节不进库、不进 ViewModel:它是一次
  性的流,进了 ViewModel 每次重渲染都会被重新写进终端一遍。

## 换掉的接线(只有这两处)

| 原来(app-desktop) | 现在(app-shell) |
|---|---|
| `use_context::<Kernel>()` + `k.pty_bytes()` | `Bridge` 当 props 传进来 + `bridge.pty` |
| `k.send(Command::Terminal*)` | `bridge.cmd(Command::Terminal*)` |
