# V3 · Issue 调度接 Cursor Agent CLI（设计，未落地）

> **30 秒导读**：这是预研后的设计记录，不是已实现规格。给以后接手「本机只有 Cursor、没有 Claude」这条执行器的人看。**现在作数；代码未改。** 穿刺事实见文内「证据」节。代号 **V3-cursor-cli**。

看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md)；铁律见 [`../../CLAUDE.md`](../../CLAUDE.md)。Issue 开工时 buddy 提示词怎么注入，仍以 [`../v2-prototype/issue-dispatch-prompt-skill.md`](../v2-prototype/issue-dispatch-prompt-skill.md) 为准。

---

## 1. 今天你在哪配「用 Cursor 还是 Claude」

**没地方配。Issue 点开工，写死走本机 `claude`。**

三层现状，别混：

| 层 | 现在怎样 | 人能不能改 |
|---|---|---|
| **Issue 嵌终端开工**（你日常点 ▶跑） | 代码里写死用 Claude 那张启动表（`run_issue_interactive` 只调 `CLAUDE`） | 不能。设置里没有、Issue 卡上没有、智能体编辑表单也没有这一项 |
| **智能体卡上的「执行引擎」** | Hub → 智能体 → 展开详情，能看见一行「执行引擎: Claude Code」 | **只读**。编辑表单只能改名称 / 角色 / 模型 / 技能 / 常驻指令，存库命令不带执行引擎字段。种子数据一律是 `claude-code` |
| **工作流非交互跑**（旧脚本路径） | 会读智能体卡上的执行引擎字段；不是 `claude-code` 就诚实报「暂不支持」 | 字段在库里，界面改不了；Issue 开工不走这条 |

所以：不是「配了但没接上」，是 **Issue 这条主路径根本没有配置面**。本机装了 `agent`（Cursor Agent CLI）也改变不了开工命令。

仓里有一张 Cursor 占位启动表，`supported = false`，启动命令还写成了 `cursor`（那是打开编辑器，不是 Agent CLI）。落地时必须改成 `agent`。

---

## 2. 落地时配置面（拍板，未实现）

「这台电脑用 Cursor 还是 Claude」是 **本机事实**；「这张队友卡声明自己走哪条 CLI」是 **智能体事实**。两层都要，Issue 卡上不再单独选一次（避免一张活两个真相）。

1. **本机默认**（设置 Hub）  
   这台 Buddy 开工默认用 `claude` 还是 `agent`。对应「用户电脑配的是 Cursor、没配 Claude」。未登录 / 找不到二进制时，空态写清楚要先 `agent login`（或设 `CURSOR_API_KEY`），不假装能跑。

2. **智能体卡「执行引擎」改为可编辑**  
   沿用已有字段，界面从只读改成可选：`claude-code` / `cursor-agent`。Issue **指派了**这张卡，就跟这张卡；没指派，用本机默认。

3. **不在单张 Issue 上再选 CLI**  
   开工命令由「指派的智能体 → 否则本机默认」决定。

账单与网关：`claude` 走用户自己的 Claude 配置（本实践环境是 GLM）；`agent` 默认打 Cursor 云。设置里要写明，不混成「换个二进制名」。

---

## 3. 最小代价接法（预研结论）

本机已装 `agent` / `cursor-agent`（2026.08.04），**不要**用 `cursor.exe`。

| buddy 需要 | 落地怎么做 | 明确砍掉 |
|---|---|---|
| 系统提示词（任何 Issue 必带） | 开工前把现有系统提示词写进该 Issue 工作区的 `AGENTS.md`。官方文档写明 CLI 会读 `AGENTS.md` / `.cursor/rules` / `CLAUDE.md` | 不把隐藏旗标 `--system-prompt`（源码注明团队限定）当正式能力 |
| 首条用户消息 | 位置参数：Issue 标题 + 描述（与现在 Claude 相同） | — |
| 恢复某一场会话 | 先 `agent create-chat` 拿到 id（未登录也能造出 id），再 `--resume <id>` | 不新做一套 hook 去抓会话 id |
| 放开改文件 | `--force`（或 `--yolo`） | — |
| 工具白名单 | 第一版不做 | 隐藏 `--allowed-tools` 是内部协议名，对不齐 Claude 的工具名 |
| 单次花费封顶 | 第一版不做 | `agent` 没有对应旗标；设置里诚实写「Cursor 路径不封顶」 |

第一次开工（伪命令）：

```text
agent --workspace <issue工作区> --force --resume <create-chat的id> "<标题+描述>"
```

工作区里已有 `AGENTS.md`，CLI 自己读。恢复同一场：同一 `--resume <id>`，不重写 `AGENTS.md`（与 V2「恢复不重灌系统提示词」对齐）。

**还要再穿一张才敢写「和 Claude 对等」**：本机 `agent login` 之后，用一份最小 `AGENTS.md` 开一场交互会话，问它能否复述 buddy 铁律。只验证「读到了」，不当日常开工。

---

## 4. 证据（2026-08-14 穿刺）

- 命令：`agent --help`、`agent about`（未登录）、`agent create-chat` 打出 UUID。
- 隐藏旗标：本机 `index.js` 注册了 `--system-prompt <file>`（团队限定、`.hideHelp()`）、`--allowed-tools`（internal）。未登录时 `--system-prompt` 先报要登录，说明解析器认这个旗标。
- 官方文档（cursor.com 规则页）：CLI 读工作区 `AGENTS.md` / `.cursor/rules` / `CLAUDE.md`。
- 源码锚点：`crates/bw-engine/src/interactive_cli.rs` 的 `CLAUDE` / `CURSOR`；`crates/bw-app/src/lib.rs` 的 `run_issue_interactive`；Hub 只读展示在 `crates/app-desktop/src/screens/agent_hub.rs`。

---

## 5. 不做什么

- 不落地代码（本篇只存档）。
- 不把 `cursor.exe` 当执行器。
- 不在 Issue 卡上再加一个 CLI 下拉。
- 不声称与 Claude 对等，直到登录后那张注入验证跑过。
