# V3 · cowelink 网页旁路内嵌（设计，未落地）

> **30 秒导读**：这是预研后的设计记录，不是已实现规格。理念：**Builders 只有一张工作台，就是 buddy**；WeLink 会话处理是工作台里的一块，不是再弹一个 CoWeLink 窗口。**现在作数；代码未改。** 代号 **V3-cowelink-sidecar**。

看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md)。对照已落地的内嵌先例：[`open-design-embed-spike.md`](../v2-prototype/open-design-embed-spike.md) 与 V3 原型进度。

---

## 1. 为什么要挣扎，以及为什么不能 iframe 现在的安装包

Open Design 能嵌，是因为它自己有本机网页口（管道报出 `http://127.0.0.1:<端口>`），buddy 只负责发现 + iframe + 没接到就空态。

CoWeLink（`D:\software\cowelink\CoWeLink.exe`，源码 `D:\2026\code\cowelink`）是 Electron 窗：

- 安装包用本地 HTML 文件加载界面，**没有**本机 HTTP 首页可发现。
- 开发态 Vite 在 `127.0.0.1:1702`，但页面必须靠 Electron 的预加载脚本才能调 WeLink CLI / 助理。buddy 的 iframe **没有这层桥**，嵌进去是死壳（列表、发送、助理全废）。
- 产品反命题仍有效：buddy 不是群聊/收件箱平台。这里嵌的是「WeLink 处理能力」，不是把 buddy 做成 IM。

外开 CoWeLink 违背「只有一张工作台」。HWND 把别人的窗口塞进 buddy 能做出「窗在格子里」的假象，无框标题栏、两套 Chromium、焦点和缩放都会拧，不当产品路径。

---

## 2. 方案：cowelink 长出网页旁路，buddy 只嵌

和 Open Design 同构。你们有 cowelink 源码，这条主动权 orca 没有（orca 已不嵌整窗，见会话结论）。

1. **cowelink 仓**加本机 HTTP + WebSocket：把现在主进程里的能力（读会话、发送、助理）桥到网页，不再只走 Electron 预加载。
2. 支持 **无窗口**跑（只留旁路，不弹 Electron 窗）。
3. 端口可发现（环境变量或本机状态口），不写死。
4. **buddy** 用与 Open Design 同一套：发现 URL → iframe → 没接到就诚实空态。不另弹窗。
5. buddy **不重做** WeLink 会话 UI，只注入工作台规范（当前项目、可写成 Issue 的入口等）——细节落地时另开篇，本篇只钉「旁路 + 内嵌」。

第一张穿刺应打在 **cowelink 仓**：先露出 `127.0.0.1` 首页 + 一条只读会话列表。buddy 侧等有 URL 再加板块，不先做空按钮假装嵌进去了。

---

## 3. 和 Open Design / orca 的位置

| | Open Design（已嵌） | cowelink（本篇） | orca |
|---|---|---|---|
| 补 buddy 没有的能力 | 原型设计器 | WeLink 会话处理 | 多路 Claude 会话（Issue 终端已做） |
| 嵌法 | 发现本机网页 → iframe | 先让 cowelink 长出网页旁路，再 iframe | 不嵌整窗 |
| 没接到 | 空态 | 空态 | — |

---

## 4. 不做什么

- 不落地代码（本篇只存档）。
- 不外开 CoWeLink.exe 当「集成」。
- 不 HWND 硬塞窗口。
- 不在 buddy 里重写一套 WeLink 界面。
- 不把 WeLink 做成团队收件箱；完成仍由人点，消息不会自动把 Issue 标成完成。
