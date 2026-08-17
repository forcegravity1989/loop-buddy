# V3-use-fix · 创建流列仓 + Claude 探测

> **30 秒导读**:给同事用 V3 时撞到的事。安装器只认 `bin\claude.exe`，终端里
> `claude` 能用也会被挡；挡过去了，没有 exe 时 Issue 终端仍可能起不来。创建流
> 「↻ 刷新列表」只拉 30 条，成员仓会被截掉。搜索一度做成「搜索框 + 原生下拉」
> 两套控件，不符合常规。程序也没有图标。本篇是这轮改动的设计事实源。纳入时
> 选分支**不在本轮**。

看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md)；代号查 [`../code-schemes.md`](../code-schemes.md)。

## 范围

| 做 | 不做 |
|---|---|
| 安装器认 `claude.exe` **或** `%APPDATA%\npm\claude.cmd`，`BW_CLAUDE_BIN` 写实际找到的那条 | 纳入时选分支 / 一仓两项目（§4.16 已否，要另开设计） |
| 应用启动按同一顺序解析；`.cmd` 开工走 `cmd.exe /c`（ConPTY 不能直接 CreateProcess 脚本） | 手填任意 path 当一等纳入（仍要是 `--mine` 里的仓） |
| 创建流 codehub/github 一次拉最多 999 + **一个可搜索下拉**（打字过滤已加载，弹出最多 30 条，点选） | 按最近活跃重排远端列表（CLI 默认序先不动）；999 以外翻页/远端搜索 |
| 程序图标：clay 方砖 + 纸色环 + 工作台，贴窗口 / exe / 安装器 | 另开一套品牌体系或换正式画师稿 |
| 项目墙「测一下」认绿/内源/黄任一区已登录 | 猜一个默认区；把未登录的区也装成绿 |

动手前提 issue（本 skill 只提醒，不代建）。

## 1. 没有 exe，Issue 还能跑吗

**终端能跑 ≠ buddy 能开工。**

- 终端 `claude` 走的是 npm 垫片 `%APPDATA%\npm\claude.cmd` → node 拉 JS CLI。
- `bin\claude.exe` 是 npm `postinstall` 从可选包拷来的 PE，不是解压自带。同事可以有主包、没有 `bin`。
- V3 开工是 Issue 终端（ConPTY）。`CreateProcess` 直接打 `.cmd` 会 `ERROR_BAD_EXE_FORMAT`（不是合法 Win32 映像）。
- 非交互 `claude -p` 走 `tokio::process::Command`，裸 `.cmd` 也不稳。

所以只改安装器放行、却把 `BW_CLAUDE_BIN` 写成不存在的 exe，或写成 `.cmd` 却不包 `cmd.exe`——安装能过，点跑仍失败。

**决议**:

1. 探测顺序（安装器与应用同一条）：显式/`BW_CLAUDE_BIN`（文件在才算）→ `...\claude-code\bin\claude.exe` → `%APPDATA%\npm\claude.cmd` → 退回 PATH 上的 `claude`。
2. 优先 exe。只有 exe 不在、cmd 在，才写/用 cmd。
3. 路径以 `.cmd`/`.bat` 结尾时，PTY 与 `tokio_cmd` 都改打 `cmd.exe /c <脚本>`，后面再跟原来的参数。
4. 设置里人填的路径照用；填了但不存在，探测如实失败，不偷偷换一条。

## 2. 多拉、少画、搜索已加载

实测：绿区 `--mine` 与 `--membership` 都是 79 个（组继承也算，不是「必须我拥有」）。`aipdu/oh-my-hw-claudecode` 排第 74，被当时的 30 截掉。后来改成 200 仍偏少——人要是有 300、500 个仓，搜索也补不上（搜索只过滤已加载的，不打远端）。

**决议**（两层数字，不是一个）:

- **拉**：`ListCodehubRepos` / `ListGithubRepos` 的 limit 改为 **999**（codehub/github 对称）。
- **画 / 搜**：合成**一个可搜索下拉**（常规 combobox），不要「搜索框 + 原生 `<select>`」两套控件。一个输入框：聚焦或打字时弹出匹配列表（最多 30）；点一条即选中并填入。没打字 = 已加载列表的前 30；打了字 = 匹配结果的前 30。过滤仍只对已加载列表（path / 描述 / 默认分支，不区分大小写），不打远端。
- 文案：已加载 N 个（最多 999）；框里搜索，下拉最多 30 条。仍没有 = 不是 member，或排在 999 以外。
- 不恢复手填 path（V1 纳入设计：列表是 `--mine` 的诚实约束）。999 以外真撞到再加翻页/远端搜索，不在本轮。

## 3. 项目墙「测一下」认三区

旧探针写死 `codehub-cli -H open project list --mine --limit 1`。只登了黄区（或只登了绿区）的人，token 正常也会红，墙上还写「先 `-H open auth login`」。

**决议**:读 `codehub-cli auth status`（一次列出全部 host）。任一区已登录即过，文案写实际登录的区（绿区 / 内源 / 黄区）；三区都没有才 Fail。不猜默认区，不装绿。

## 4. 程序图标

仓库、窗口、安装器原先都没有图稿，任务栏 / 开始菜单 / 桌面快捷方式 / 安装包都是系统默认图标。

**决议**：先用一枚几何标记顶上，总比没有好。正本在 `crates/app-desktop/assets/app.ico`（另有 256 PNG）。画面按 buddy 三个产品事实画，不另起品牌：

- **clay 方砖**（`#C5654A`）：工作台的底色，和界面主色同一块。
- **纸色环**：项目是环不是流水线（五阶段回流）。
- **一条小台面 + 两条腿**：工作台，不是一块看板。

贴三处：exe 资源（任务栏 / 快捷方式 / 卸载项）、Dioxus 窗口（资源 32512）、Inno `SetupIconFile`（安装向导）。正式画师稿以后整链替换这一份即可。

## 5. 纳入选分支

不在本轮。产品身份仍是「一个仓 + 一条远端默认主干」。对方若两条分支当两个项目，应拆仓（或两个 path）。见 PRACTICE §4.16。
