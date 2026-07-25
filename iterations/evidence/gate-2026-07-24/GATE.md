# plan/15 第 0 阶段三关闸门 · 实测结果(2026-07-24, Fable 亲驾)

载体:dist/BW.app(V1-1 手工骨架包,commit 894dafd)。

## G-1 打包/窗口可见 —— ✓ 通过
- 用 `open --env BW_DB=<临时库> dist/BW.app` 与 nohup 直启二进制两种方式启动。
- MCP computer-use `screenshot` **清晰拍到 BW 窗口**:标题栏「Builders' Workbench」、
  空库首页「我的项目 / 还没有项目 / + 新建项目」全部可读。
- 结论:打包成 .app 后窗口成为 OS 一等公民,**历史"裸 debug 二进制拿不到窗口"暗礁排除**。
  这是相对过去会话的真实进展。

## G-2 点击生效 —— ✗ 受阻(环境,非 BW 侧)
- 现象:对 BW 窗口内**任意**坐标点击((43,89) logo、(482,318) 新建项目卡、(43,259) 导航图标)
  MCP 一律报 `Click at these coordinates would land on "程序坞"(Dock),not in allowed applications`。
- 排查(全部无效):
  1. `request_access` 已授予 BW(tier=full);
  2. `open_application` 把 BW 激活到 frontmost(标题栏高亮确认);
  3. 改用 `open` 以正规 .app 身份启动(非裸进程);
  4. 多个明确位于窗口内、绝不可能是 Dock 的坐标。
- 结论:computer-use 的点击 hit-test 把整个 BW 窗口区域的 ownership **误判归属 Dock** 并拦截。
  根因在本机 computer-use 环境,非 BW/打包侧。与既往 memory「clicks blocked / clicks still blocked」
  跨会话反复复现一致。**截图(人眼证据)可用,点击驱动不可用。**

## G-3 CLI 截图落盘 —— ✗ 权限降级(可由用户一次性授权解决)
- `screencapture -l<窗口ID>` 报「could not create image from window」;
  `screencapture -R<区域>` 与全屏 `screencapture` 落盘成功但**内容是纯墙纸**,
  全屏图菜单栏显示「Claude」= 截图由 Claude 宿主进程执行,该进程缺「屏幕录制」权限,
  只能拍到墙纸层、任何应用窗口都不在(与 memory「wallpaper-only = capture perms」一致)。
- 对照:MCP computer-use `screenshot` 有独立 compositor 级捕获授权,能拍到 BW 真窗口。
- 结论:CLI 落盘不可用;可靠的人眼窗口证据来自 MCP screenshot。
  若要 CLI 落盘,需用户在 系统设置→隐私与安全性→屏幕录制 给 Claude 宿主授权一次。

## 裁决
G-1 过;G-2、G-3 卡在**本机 computer-use 环境**(点击 hit-test 误判 + CLI 截图权限),
均非 BW/代码侧问题。触发 plan/15「闸门卡死:停,拿证据找用户定夺,绝不静默换路线」。
待用户拍板路线后继续。
