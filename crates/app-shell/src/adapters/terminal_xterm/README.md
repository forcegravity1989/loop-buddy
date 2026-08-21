# 适配模块 · 内嵌终端(xterm.js + PTY)

## 借自哪个项目 / 文件

- `crates/app-desktop/src/screens/op/terminal_widget.rs` —— 整块搬过来的,是 V3 试用期
  一个坑一个坑填出来的那一份。
- `crates/bw-engine/src/pty_backend.rs` / `terminal_manager.rs` —— PTY 后端与按会话分
  的有界缓冲,原样沿用,一行没改。
- Orca 的 `use-terminal-pane-lifecycle.ts`(见 `docs/archive/v4-prototype/research/orca.md` §2a)
  —— 选区防抖 + 长度上限那一段的思路。
- `docs/v4-prototype/hifi/index.html` 的 `copyTerminal`(约第 1122 行)—— 标题栏那颗
  「复制」按钮:先试 `navigator.clipboard.writeText`,不成退到临时 textarea +
  `execCommand('copy')`,两条都不成就如实告诉人没复制成。按钮的样式类 `.copybtn`
  高保真里就有,样式表里一直带着,这次只是终于有东西用它了。

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
- **标题栏那颗「复制」按钮**,连同它两级写剪贴板、失败如实回执的做法(见上)。
  没选中时它复制整段(含回滚的内容),这也是高保真里的语义。按钮**故意不挂
  Rust 那边的 onclick**:绕一趟 IPC 再 `document::eval` 回来,浏览器认的那一下
  「用户手势」已经过期,`execCommand('copy')` 会被直接拒掉;点击改由 JS 侧用
  事件委托按 id 认领,当场处理完。

**这一段不是借的,是这里自己写的**:认组合键复制粘贴。macOS 上 Cmd+C 本来要靠
宿主应用的「编辑」菜单把系统的 copy: 动作路由给 WebView,WebView 再给网页发一个
copy 事件,xterm 自带的那个监听器才有机会把选区塞进剪贴板;这个壳没有原生菜单,
链在第一步就断了 —— 选中一直是好的,复制永远出不来。现在改成终端在自己身上认键:

- **Cmd+C 与 Ctrl+Shift+C**(两套都认,不嗅平台):有选中就复制选中,并且不把这个键
  当成普通输入吐给 PTY。
- **没选中就完全不碰这个键**,让它按原路走完。**不带 Shift 的 Ctrl+C 压根不在识别
  范围里** —— 那是「中断正在跑的命令」,任何时候都原样送到 PTY,有没有选中都一样。
- **Cmd+V 与 Ctrl+Shift+V** 粘贴,只有 `navigator.clipboard.readText` 一条路
  (`execCommand('paste')` 在 WebKit 里对网页是禁的,不存在第二条);读不到就说读
  不到。文字交给 `term.paste()` 而不是自己拼给 PTY —— 它负责括号粘贴模式的包裹和
  换行归一,自己拼会把多行粘贴变成一串回车。
- 顺带修掉一个原来就有的错:`keyBytes` 会把 Cmd+C / Cmd+V 漏成普通的 `c` / `v` 推给
  PTY,还顺手 `preventDefault` 掉。现在带 Cmd/Super 的组合一律不算终端输入。

## 没借什么

- **不借 Orca 的 `@xterm/headless` 回放机制**。它为了让终端内容能被 agent 读回去,
  在后台另跑一个无头终端做回放;buddy 不需要 —— 会话内容的正本是仓里的改动和 MR,
  不是终端里滚过的那些字。
- **不接 A 刀那一版的"会话内容存库"设想**。终端字节不进库、不进 ViewModel:它是一次
  性的流,进了 ViewModel 每次重渲染都会被重新写进终端一遍。
- **不为了复制去装 macOS 原生菜单**。装菜单确实能把 Cmd+C 接通,但那是**全应用范围**
  的改动(要动 `main.rs`),Windows 上又完全用不着;一个适配模块该自足,所以复制这件
  事就在这个模块里了结。
- **不走 Rust 侧的剪贴板库**。那样最稳(不受网页那层的安全上下文限制),但要给
  `app-shell` 加依赖、加一条命令,不是这个模块能自己收口的。**留一句实话在这**:这个
  壳的页面地址是 `dioxus://index.html`,WebKit 不把自定义协议当安全上下文,
  `navigator.clipboard` 很可能在 macOS 上压根不存在 —— 真正兜底的是那条
  `execCommand('copy')`。哪天它也不灵了,再谈把剪贴板挪到 Rust 侧。

## 换掉的接线(只有这两处)

| 原来(app-desktop) | 现在(app-shell) |
|---|---|
| `use_context::<Kernel>()` + `k.pty_bytes()` | `Bridge` 当 props 传进来 + `bridge.pty` |
| `k.send(Command::Terminal*)` | `bridge.cmd(Command::Terminal*)` |
