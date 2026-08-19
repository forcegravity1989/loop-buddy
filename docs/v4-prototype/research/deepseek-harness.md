# DeepSeek Harness(dsh)源码穿刺:给 BW V4「一张工作台缝合成熟组件」的参考

> **30 秒导读**:这是对 GitHub 项目 `deepseek-ai/deepseek-harness`(简称 DSH,npm 包 `@deepseek-ai/dsh`)的源码级预研,不是转述 README。方法:`gh repo clone --depth 1` 拉真实源码(约 81M,157k+ star),逐包读 `docs/subsystems/*.md`(与源码逐行核对生成)、`docs/user/develop/*`、`packages/*/README.md`,并额外拉取 `nexu-io/open-design`(BW 已嵌入的原型工具)的 README 核实两者关系。**状态:预研,未拍板**——不改 BW 任何设计文档,只为 [mvp-blueprint-draft.md](../mvp-blueprint-draft.md) §3 第 4 站「开工工具」的选型提供依据。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md)。

---

## 1 · DSH 是什么

**技术栈**:TypeScript + Node.js(`engines.node: ^22.19.0 || >=24.0.0`)monorepo,pnpm workspace,近 90 个 `packages/*` 子包 + `apps/cli` + `apps/web`(Vite 前端)。核心插件框架 **Cordis**(`vendor/cordis`,DeepSeek 把它连同 `cosmokit`、`schemastery` 一起**源码 vendor 进仓**并以 `@deepseek-ai/*` 重新发布,而不是从 npm 依赖——见 `THIRD_PARTY_NOTICES.md:12-16`)。

**许可证**:MIT(`LICENSE:1-3`,Copyright DeepSeek 2026),vendored 的 Cordis/cosmokit/schemastery 也都是 MIT。没有额外商标或专利限制条款。仓库自称 **developer preview**:「THERE WILL BE COMPATIBILITY-BREAKING CHANGES」(`README.md`),`package.json:3` 版本号 `0.1.0-rc.7`,npm 上目前只有 rc 版本(`0.0.1-rc.1` → `0.1.0-rc.7`),还没出过 1.0。

**发布形态**:不是桌面 App,是 **npm CLI + 本机跑起的 Web UI**:`npx @deepseek-ai/dsh web` 起一个 Node 服务,浏览器打开 `http://127.0.0.1:3080`(`README.md`「Run from npm」)。还有一个 `dsh --profile headless` 的无浏览器模式,直接进核心 Agent/Session(`.agents/notes/.../2026-08-09-headless-direct-core-entry-point.md`)。**Electron 桌面壳目前不存在,只是预留的架构位**:`packages/host/webserver` 的文档写明「Electron loads the built files over `file://` and sends fetch requests through an IPC bridge instead of this server」(`docs/subsystems/web-server.md:5`),架构笔记也把它列为 future(`.agents/notes/implemented/architecture/2026-07-19-gui-layering-and-rpc-protocol.md:9-25`:「A future Electron application reuses the same web client packages over an IPC fetch carrier」)。没有移动端。

**对接哪些模型**:不是 DeepSeek 专属。`docs/user/guide/providers.md:15-19` 明说「Add provider,select a provider such as **Anthropic or OpenAI**」,还支持 Bedrock / Vertex / Azure / Codex(各自走原生鉴权),以及任意 **OpenAI 兼容自定义网关**(`llm-pi-ai` 包,`packages/llm/llm-pi-ai/`)。DeepSeek 自家路由是独立的 `llm-deepseek` 包。API key 写入即焚:界面只收不回显,真值存在 `$DSH_HOME/.credentials.yaml`,配置里只留引用(`docs/subsystems/credentials.md`,`providers.md:13`)。

**怎么驱动编码工作**:**自建 agent loop,不是套壳 `claude`/`codex` CLI**。证据链:`docs/subsystems/README.md` 列出的子系统里,`shell.md`(bash 执行器)、`filesystem.md`(文件读写编辑)、`lsp.md`(LSP 导航)、`sandbox.md`(进程级文件效果沙箱:Linux bwrap/Landlock、macOS Seatbelt、Windows ACL,`docs/subsystems/sandbox.md:5,11`)、`code-runtime.md`(模型写代码在 worker thread 里跑,即 "Code Mode")都是它自己实现的工具能力,`apps/cli/config/agent-presets/code/agent.cordis.yml:1-9` 就是标准编码 agent 预设:一套 bash/pwsh/文件系统/LSP 工具 + Code Mode(模型写一段 TS 程序调生成的 SDK,一次调用顶多次工具往返,而不是逐次 tool call)。反过来,DSH **也能被别的宿主当 CLI/协议后端用**:`packages/acp/` 实现了 [Agent Client Protocol](https://agentclientprotocol.com)(stdio JSON-RPC),仓内自带的客户端是 `dsh-subagent-acp`(见 §3)。另外它专门做了 **Claude Code hooks 兼容层**:`packages/hooks/hooks-claude-code/README.md:5-7` 能直接跑用户现有的 `hooks.json`(含 `${CLAUDE_PLUGIN_ROOT}`/`${CLAUDE_PROJECT_DIR}` 替换),这与 BW 自己的 hook 回收机制系出同源,值得留意但不是本次重点。

---

## 2 ·「万物皆插件」到底是什么

**插件的技术定义**:一个导出 `apply(ctx)` 的 TypeScript 模块(函数/对象/类三种写法都行),`ctx` 是 Cordis 的 `Context` 对象,插件靠它注册工具、事件、服务(`docs/user/develop/basic/index.md:15-27,105-138`)。声明 `inject = ['tools']` 这类依赖后,框架保证所依赖的服务就绪了才调 `apply`(同上 `:87-103`)。`ctx.effect()` 登记清理逻辑,插件卸载自动执行,不用手动 `removeListener`(同上 `:66-84`)。**没有独立的"插件语言"或沙箱运行时**——插件就是普通 Node 模块,和宿主同进程同权限运行。

**插件类型**(不是单一种类,是"用同一套注册机制表达一切"):
- **工具插件**(model-facing tool,如 bash/文件编辑/LSP)
- **服务插件**(class 继承 `Service`,给别的插件提供能力,如 llm 路由、沙箱)
- **UI 面板插件**(`dsh.client` 声明,见下)
- **模型 provider 插件**(`llm-pi-ai`/`llm-deepseek` 这类)
- **传输适配插件**(如 `dsh-acp`、`dsh-hooks-claude-code`)

**插件怎么暴露 UI(关键,回答任务里"iframe 还是 web components"的问题)**:**都不是**。是**同源 JS 模块联邦**式的机制:一个包在 `package.json` 里声明 `dsh.client`(`platform: 'web'`,可选 `inject` 依赖边、`immediately` 预取标记),并在 `exports["./client"]` 导出构建好的浏览器 bundle;宿主服务(`ctx.clientModules`,`packages/client/modules/src/index.ts:184`)扫描所有已装插件、拼出一张 `WebBootEntry` 清单,注入到首页 `<head>` 的 `window.__DSH_BOOT__` 里,浏览器端在启动前解析这张图,按需 `fetch` 每个插件的 `/plugins/<id>/client.js`(`docs/subsystems/client-modules.md:1-11,49,55-57`)。也就是说,**UI 插件的代码会被 import 进宿主页面的同一个 JS 运行时**,和宿主 React 应用共享 DOM、共享 window——不是 iframe 隔离、不是 Web Components 封装,更接近"信任你装的每个插件"。全文搜索 `iframe` 只在几个和这套面板机制无关的地方(剪贴板处理、测试 fixture)命中,证实没有 iframe 隔离层。

**插件怎么和宿主通信**:同进程 Node 侧靠 Cordis 的 `ctx` API(服务调用/事件总线);浏览器侧到 Node 侧靠 `packages/host/webserver` 提供的 `/api` HTTP+WebSocket 桥(`docs/subsystems/web-server.md`),即 fetch + SSE/WS,不是裸 IPC(Electron 版本以后才会换 IPC bridge)。

**插件怎么被发现和安装**:**没有应用内市场/搜索面板**。分发单位是两个 `package.json` 概念(`docs/user/develop/basic/publish.md:9-16`):
- **bundle** = 一个 npm 包,`package.json` 里声明 `dsh.bundle: { patch: "./cordis.patch.yml" }`(示例见 `publish.md:36-44`),这份 YAML patch 描述要插入/覆盖哪些插件行;
- **profile** = `$DSH_HOME/profiles/<name>` 下的一个目录,`dsh.profile.bundles` 记录这个运行时组合装了哪些 bundle、顺序如何,由 `dsh plugin --profile <name> add <pkg>` 命令维护(底层直接转发给 `pnpm`,`publish.md:75-101`)。
- 配置叠加顺序固定:profile 的 bundles 列表(顺序生效)→ profile 自己的 patch → `$DSH_HOME` 全局 patch → 命令行 `--patch` 覆盖层,后者赢(`publish.md:112-128`)。
- 发现渠道 = **GitHub 话题标签 `dsh-plugin`**(`README.md`「Add the `dsh-plugin` topic … for discoverability」,`CONTRIBUTING.md:9-15` 重申)+ npm/GitHub 直装(`dsh plugin --profile demo add github:you/hello-plugin`),**没有官方审核的插件市场**。
- 安全提示(源码原文,值得直接搬进 BW 的风险认知):GitHub 直装的插件如果带 `prepare` 构建脚本,pnpm ≥10 会先拒绝执行、需要用户在 `pnpm-workspace.yaml` 里显式 `allowBuilds` 放行,官方原话「Treat that allowance as what it is: **permission to execute the package's code on your machine at install time**, outside any sandbox the agent runs under」(`publish.md:153-173`)——即插件安装期代码执行**不受**那套给 agent 工具用的进程沙箱保护。

**SDK / 接口定义的准确文件路径**:
- 插件基础类型:`vendor/cordis`(Context/Service/Plugin 定义本体)
- 教程:`docs/cordis-tutorial/01-first-plugin.md` ~ `07-into-the-harness.md`
- 面板机制服务:`packages/client/modules/src/index.ts:184`(`ClientModuleRegistry`)、清单类型 `packages/client/modules/src/client/manifest.ts`
- HTTP 载体:`packages/host/webserver/src/index.ts`
- 打包/安装 CLI 行为:`apps/cli/reference/README.md:43-51,80`

---

## 3 · Open Design 怎么接进 DSH(与预设前提不同,已核实)

结论先说:**方向反了**。不是「Open Design 作为 DSH 的 Cordis 插件」,而是 **Open Design 是宿主应用,`dsh` 是它能直接拉起的多个"编码 agent 运行时"之一**——地位和 Claude Code、Codex、Cursor、OpenCode 等 26 个 CLI 一样。证据取自 `nexu-io/open-design` 的 README(`gh api repos/nexu-io/open-design/readme`):

> 「🧩 **DeepSeek Harness is now supported.** Connect DeepSeek's official `dsh` agent harness to OpenDesign as a native runtime, with structured thinking, tool calls, model discovery, cancellation, and session resume.」

以及它的 **Platform Compatibility** 表格明确分两种接入方式:「skills, CLI, and MCP for agents that consume OD」(Claude Code / Cursor / Copilot 等——这些是**反向**把 Open Design 当 MCP server 装,`od mcp install claude`)vs「**native runtime adapters** for agents that OD launches directly」,DSH 属于后一种,状态标 **"✅ Native runtime"**,安装方式是 `od agent setup deepseek-harness`,底层落在 `apps/daemon/src/runtimes/defs/`(适配器契约见其 `docs/agent-adapters.md`,未展开细读)。

也就是说:
1. **Open Design 声明的"DeepSeek Harness Design Plugin"是市场用语,不是 Cordis `dsh.bundle`/`dsh.client` 意义上的插件**——DSH 仓库源码里(`grep -r "open-design\|opendesign\|nexu"`)完全没有对 Open Design 的引用,证实这条集成是单向的、由 Open Design 一侧实现,大概率通过 DSH 的 CLI/ACP 通道(`packages/acp/`,§1 已提到)拉起一个 `dsh` 进程做 agent 后端,而不是把自己塞进 DSH 的 Web UI。
2. Open Design **本身是独立的本机桌面应用**(macOS/Windows,`Local-first native desktop app`),不依赖 DSH 存在——这与 BW 今天嵌入 Open Design 的方式(本机探活 + 内嵌 URL)完全对得上,BW 现有姿势不用变。
3. Open Design 反过来也能被 Claude Code/Cursor 等当 **MCP server** 装(`od mcp install claude`),这是另一条独立集成路径,和 DSH 的 Cordis 插件系统同样无关。

---

## 4 · DSH 的边界:它没有的东西 vs BW 的核心命题

逐项核对 BW 命题关心的概念,DSH 源码里**确实没有**对应物(不是没读到,是子系统清单里压根不存在这一层):

| BW 概念 | DSH 有没有 | 依据 |
|---|---|---|
| 项目(跨会话、跨周的实体)| 没有。最大的持久单位是 **session**(一次对话)和 **profile**(一份运行时组合),都不跨"项目生命周期" | `docs/subsystems/session.md`、`workspace.md` |
| 周计划/北极星/滞后指标/引领指标 | 没有。`goal.md` 是"同会话目标"(single-session,agent 自己够不够继续干的判断),`schedule.md` 是会话内提醒,都不是项目级规划 | `docs/subsystems/goal.md:1-10`、`schedule.md` |
| health 信号灯/项目健康推导 | 没有。有 `token-meter`(用量计量)和 `session-telemetry`(遥测上报),都是运行时观测,不是健康推导 | `docs/subsystems/token-meter.md`、`session-telemetry.md` |
| Issue/任务看板 | 没有持久看板。`packages/todo` 是**单会话**的 `todo_write` 工具(模型每次整表重写,替换式,像 Claude Code 的 TodoWrite),没有跨会话保留、没有指派、没有状态机 | `packages/todo/tool-todo/README.md:5-11` |
| 每活一个 worktree | 没找到面向用户的功能。`worktree` 只出现在 DSH 自己开发用的 cookbook(`maintaining-dsh-code-review.md`、`responding-to-pr-review-on-a-stack.md`)——是他们自己怎么用 Git worktree 管理 stacked PR 的开发流程文档,不是产品功能 | 全仓 grep 结果 |
| 技能市场(严选/鱼塘)| 有技能**发现机制**(`ctx.skills`,local/embedded/remote provider 分层合并),但没有找到面向用户的浏览/安装市场 UI;技能来源仍是文件系统或代码注册,发现渠道和插件一样靠 GitHub | `docs/subsystems/skills.md:1-13` |
| 规范铺底进项目仓(AGENTS.md 等)| DSH **自己的仓**用 `AGENTS.md`+`CLAUDE.md`,但这是它自己的开发规范,**没有**把这套铺进"用户接入的目标项目"的功能——它面向的是"帮我在这个仓里写代码",不是"帮我把这个仓管理起来" | 仓根目录文件本身 |

**DSH 的范围到哪结束**:一个**单会话的编码 agent 运行时**——工具执行、模型路由、沙箱、UI 面板机制、插件框架,这些做得很深很规范化(近 90 个子包各自一篇文档,类型定义逐行核对)。它不管"这个项目本周要做什么、上周做得怎样、健康不健康、活跑没跑完"——**这条线完全是空的,从这里往后就是 BW 的地盘**。这与 BW 用户既有判断(`feedback-mvp-management-core-first` memory:管理体系是主体、connector 是仆从)吻合,DSH 印证了"成熟的 agent 执行底座"和"项目管理体系"是两层不同的东西,没有一个现成项目把两者都做了。

---

## 5 · BW 三条缝合路线评估

**A. BW 作为 DSH 插件(BW 的屏幕活在 DSH 壳里)**

DSH 的插件模型服务于"给一个 Node/Web 应用加能力",不是"整体嵌入另一个独立应用"。BW 是 Rust 原生二进制(Dioxus/wry),要变成 DSH 插件意味着:要么把 BW 的 UI 重写成 `dsh.client` 声明的浏览器 JS bundle(等于放弃 Rust 原生壳,五屏全部用 DSH 的技术栈重做),要么在 Node 侧起一个"DSH 插件"去 shell 出 BW 的二进制(违背"UI 插件同源运行"的设计意图,也拿不到什么好处)。**且 UI 插件与宿主同源共享 window,BW 不可能只借一小块沙箱化面板**。结论:**代价极高、收益极低,不推荐**——等于放弃 BW 自己的原生壳、PTY、内核这些已验证的资产,换一个还在 developer preview、承诺"会有破坏性变更"的外部框架来装 BW 的管理体系,方向反了。

**B. DSH(或其面板)嵌入 BW(像今天嵌 Open Design 一样开一个本机 URL)**

技术上可行但意义不大。DSH `dsh web` 起服务在 `127.0.0.1:3080`,BW 完全可以像今天探活 Open Design 一样探活它、在 WebView 里开一个标签页——但 DSH 的 Web UI 是一个完整的对话式编码界面,和 BW 会话屏已经内嵌的 Claude CLI 终端在能力上高度重叠(都是"跟一个 agent 对话、看它改代码"),嵌入它相当于给用户三个入口做同一件事(嵌入终端 / Open Design / DSH Web UI)。**没有找到"只嵌入 DSH 某个面板、不要整个对话界面"的稳定深链**——面板是宿主页面启动时一次性拼好的模块图(`window.__DSH_BOOT__`),不是可单独 URL 路由到某一插件面板的设计。结论:**不推荐现在做**,除非未来 DSH 面板生态里出现 BW 会话屏用不到嵌入终端替代的独特能力(目前没看到)。

**C. 借 DSH 的插件契约思路,定义 BW 自己的"开工工具"接口(推荐)**

这是收益最实、风险最低的一条。不搬代码、不依赖 DSH 是否稳定,只借鉴它验证过的**几条设计判断**,套进 BW `mvp-blueprint-draft.md` §3 第 4 站已经在定义的"开工工具"(Claude CLI / Cursor / Open Design,未来可能更多):

1. **两级清单借鉴**(对应 DSH 的 bundle vs profile,`publish.md:9-16`):BW 目前是"活的类别标签 → 默认开工工具/技能"的一张映射表(`mvp-blueprint-draft.md` §3 第 4 站),可以借 DSH 的思路把"一个开工工具怎么接入 BW"写成一份**独立声明**(工具名、启动方式——本机进程/URL 探活/CLI 命令、需要的能力如"能不能读写文件""要不要工作区路径"),这样以后新增开工工具(比如某天真接进 DSH 本身,或任何新 CLI)只是加一条声明,不用碰 BW 内核代码——这正是 CLAUDE.md「组件缝合」要的效果。
2. **能力声明化,而非硬编码判断**(对应 DSH `inject` 依赖清单):今天 BW 判断"这个工具能不能用"靠散在各处的探活代码;可以借 DSH `dsh.client`/`dsh.bundle` 那种"声明式清单 + 加载期一次性校验、坏了聚合成一个响亮错误"的模式(`docs/subsystems/client-modules.md:51`),而不是运行时到处 try/catch。
3. **安全认知直接搬**:DSH 官方原话——**装第三方东西=在你机器上执行权限外代码**(`publish.md:153-173`)——这条对 BW 未来如果开放"资产库"给别人贡献技能/连接器时,是必须写进用户可见文案的免责认知,不是只在内部工程笔记里提一句。
4. **不借**:DSH 的 Cordis 插件框架本身(TS/Node 生态,和 BW 的 Rust 内核完全两条技术栈,硬接只会增加一层不必要的翻译层);DSH 的"同源 JS 模块联邦"UI 机制(BW 用 WebView 内嵌整站的方式已经够用,且更安全——每个组件跑在自己的页面里,不共享 window)。

**推荐路线 C**,理由直接对应用户在 CLAUDE.md 定的态度——「管理总控自研,对外能力缝业界成熟实现,不重造轮子;单个原生二进制;能力底座=成熟 CLI」:DSH 证明了"给编码 agent 拼工具/模型/UI"这层已经有成熟框架在做,但没有一个现成项目把"项目管理体系"这层也做了(§4),BW 该重的正是那层,不该重的正是 DSH 已经做深的那层——借它的**接口设计判断**而不借它的**运行时**,风险最小、和 BW 现有架构摩擦最少。

---

## 6 · 风险

- **成熟度与变更频率**:仓库自称 developer preview,「THERE WILL BE COMPATIBILITY-BREAKING CHANGES」;`.agents/notes/implemented/` 下 697 篇架构/功能笔记,最近一周(08-10~08-15)仍有 60+ 篇新增,最新 PR 编号已到 #2620(`git log -1`),说明改动速度非常快,任何"深接口"依赖都可能被下一版打破。npm 只发布过 rc 版本(`0.0.1-rc.1` ~ `0.1.0-rc.7`),尚无 1.0。**这进一步支持"只借判断、不接运行时"的路线 C**,深依赖它的任何具体 API 都是脆的。
- **贡献模式**:官方明说「we cannot accept external pull requests at the moment」(`CONTRIBUTING.md:9`)——是 DeepSeek 内部主导的仓库,外部只能通过独立插件包参与生态,不能改核心。
- **中国网络可用性**:发布渠道是标准 npm registry + GitHub(`npx @deepseek-ai/dsh`、`gh repo clone`),没有发现官方国内镜像;可用性和一般 npm/GitHub 依赖一致,需要能访问这两者的网络环境,未做特殊适配。
- **插件 API 文档化程度**:意外地高——`docs/subsystems/*.md` 大多数页面都有"由脚本从源码生成、CI 校验字节一致"的 **Cordis API** 小节(如 `extensions.md:7`「Generated from source by `scripts/gen-cordis-catalog.ts`」),文档不会和代码脱节,这点比大多数早期项目扎实。但**稳定性**和"文档化程度"是两回事——接口本身仍会破坏性变更。
- **第三方插件的安全模型**:**没有沙箱**。插件代码(不管是 Node 侧插件还是浏览器侧 UI bundle)与宿主同进程/同源运行,唯一的把关点是安装期的 `pnpm allowBuilds` 确认(仅拦截"带构建脚本的 git 依赖",不拦截已构建包或已通过审查的正常安装)和 MCP server 的一句提示「trusted executable code outside the agent sandbox」(`apps/cli/reference/README.md:80`)。DSH 自己给 **agent 的工具调用**(bash/文件写入)配了很扎实的进程级沙箱(bwrap/Landlock/Seatbelt/Windows ACL),但那套沙箱**不覆盖插件代码本身**——这是一个值得注意的不对称:执行"模型要做的事"很小心,执行"你装的插件"完全信任。

---

## 给 BW V4 的建议

**推荐路线 C**:不整体嵌入 DSH、不把 BW 做成 DSH 插件,只借它已经在生产实践里验证过的几条"开工工具/组件缝合"接口设计判断,套进 BW 自己 `mvp-blueprint-draft.md` §3 第 4 站正在定义的开工工具体系。具体可借的三样,带文件路径方便以后回查原始设计:

1. **声明式工具清单**(而非硬编码判断逻辑)——参考 `docs/user/develop/basic/publish.md:9-16`(bundle/profile 两级清单)与 `docs/subsystems/client-modules.md:49-57`(加载期一次性扫描、失败聚合成一个响亮错误)。
2. **能力依赖显式声明**(参考 DSH `inject` 字段,`docs/user/develop/basic/index.md:87-103`)——BW 给每个开工工具声明"需要探活/需要工作区路径/需要网络"等,而不是散在各处 try/catch。
3. **安装期权限的用户可见免责认知**——原话抄自 `docs/user/develop/basic/publish.md:153-173`:「permission to execute the package's code on your machine at install time, outside any sandbox」,BW 资产库以后开放外部贡献时应该有等价的一句话提示。

**不借**:DSH 的 Cordis/TS 运行时(与 BW Rust 内核技术栈不合)、DSH 的同源 JS 模块联邦 UI 机制(BW 现有 WebView 整页内嵌方式已经更安全)。**Open Design 与 DSH 的关系已核实为反向**(Open Design 把 dsh 当可插拔的运行时后端之一,不是把自己注册成 DSH 的 Cordis 插件),BW 今天"本机探活 + WebView 内嵌 URL"的 Open Design 接入方式不受这次预研影响,不用改。
