# 穿刺笔记：colbymchenry/codegraph 源码研究(2026-08-18)

> **30 秒导读**:这是对开源项目 `colbymchenry/codegraph`(~67k star,npm 包 `@colbymchenry/codegraph`,"给 Claude Code / Cursor / Codex / OpenCode / Gemini / Antigravity 等 agent 用的预索引代码知识图,自动跟代码变化同步")的**源码级**穿刺笔记,给 BW V4 的会话屏右侧代码结构侧栏、运作活②「资产盘点与代码微重构」、agent 会话铺底三处设计做参考。**状态:预研,未拍板**,不是设计文档,不改代码。规则:只信真源码,不从 README 猜结论(README 部分数字确实核对过,标注来源)。**BW 仓库本来就在用 codegraph**——本文先读 BW 自己的接入现状(§2),再读 codegraph 源码本体(§1、§3-6)。codegraph 仓库克隆自 `https://github.com/colbymchenry/codegraph`(`git clone --depth 1`,commit `c6aaa20`,2026-08-07 快照;所有 codegraph 内部路径都是该仓库路径,不是 BW 仓库路径)。

---

## 1. codegraph 是什么

**技术栈**(`package.json`、`LICENSE` 实读):
- npm 包,`bin.codegraph = "./dist/bin/codegraph.js"`,Node ≥20(内嵌运行时,CLI/MCP 本体不吃这条,只有「把 codegraph 当库 import 进自己进程」才要求宿主 Node ≥22.5,见 README:577-620)。
- 解析:`tree-sitter-wasms` + `web-tree-sitter`(WASM 版 tree-sitter,`src/extraction/wasm/*.wasm` 有 30 个语言的 grammar)是通用兜底;但 README:259-270(「Built for speed — the Rust kernel」一节)说明**热路径是一个原生 Rust 解析内核**,覆盖 20 个语言(含 Rust 自身),每个语言上线前用「和 WASM 参考实现的图必须逐字节相同」验收,没有预编译二进制或语法错误的文件才落回 WASM 逐文件解析——两条路径产出同一张图。这点对 BW(Rust 项目)是好消息:BW 自己的代码走的是原生内核,不是慢的 WASM 兜底路径。
- 存储:纯 SQLite(`better-sqlite3` / Node 内建 `node:sqlite`,`src/db/schema.sql`),WAL 模式,无额外服务。
- 许可证:`LICENSE` 全文 MIT,版权方 Colby Mchenry,无额外商业条款。

**图里存什么**(`src/db/schema.sql:18-95` 实读):
- `nodes` 表:每个符号一行(`kind`/`name`/`qualified_name`/`file_path`/`start_line`-`end_line`/`signature`/`visibility`/`is_exported`/`is_async`/…),`kind` 覆盖 class/function/method/struct/enum/enum_member/trait/type_alias/variable/constant/import/file 等(BW 库里实测 12 种,见 §2)。
- `edges` 表:符号间关系(调用、导入等),带 `line`/`col`/`provenance`。
- `files` 表:每个被跟踪文件一行,`content_hash`/`size`/`node_count`/`generated`(是否生成代码,靠文件名约定 + 文件头 banner 双重判定,注释里专门举了 `*.pb.go` 这类反例)。
- `unresolved_refs` 表:索引时暂时解析不出目标的引用,带候选与状态,后续 `sync` 时重试(`schema.sql:80-95` 注释)。
- 还有一张 FTS5 全文索引(`nodes_fts`,`schema.sql:105-125`)撑 `codegraph query` 的模糊搜索。

**语言覆盖**:README:143-188 列了 20+ 语言(TS/JS/Python/Go/Rust/Java/C#/PHP/Ruby/C/C++/Swift/Kotlin/Scala/Dart/Lua/R/Nix/Erlang/CFML/COBOL/Solidity/Terraform/Svelte/Vue/Astro/Liquid/Pascal 等),另有 17 个 Web 框架的路由识别(README:315-340,Axum/actix/Rocket 在列,和 BW 用的 web 框架栈无关但说明覆盖面广)、以及 iOS/React Native/Expo 的跨语言桥接识别(README:341-370)。

**怎么同步**(`src/sync/watcher.ts`、`src/sync/git-hooks.ts` 实读):
- 默认是**文件监听 + 防抖增量同步**,不是「每次问都重新扫全仓」:macOS/Windows 用单个递归 `fs.watch`(一个 FSEvents/ReadDirectoryChangesW 句柄,常数级文件描述符开销——`watcher.ts:9-23` 注释交代了这是为修一个「每个文件一个 fd,macOS 系统级文件表耗尽」的真实故障 #644/#496/#555/#628);Linux 用每目录一个 inotify watch(目录数量级,不是文件数量级)。默认 debounce 2000ms(README「How auto-syncing works」折叠块)。
- watcher 关闭的场景(WSL2 挂载盘等)有降级路径:装 git hooks(`post-commit`/`post-merge`/`post-checkout`,`src/sync/git-hooks.ts:1-30`)在这几个会改文件的 git 操作后台跑一次 `codegraph sync`,幂等、guard 住 `command -v codegraph` 才生效。
- 手动兜底:`codegraph sync`(增量)、`codegraph index --force`(全量重建)。BW 的 CI 用的正是手动 `codegraph sync` + `codegraph status --json` 校验(见 §2)。

**agent 怎么消费**——两条并行的路径,**不是二选一**:
1. **MCP 服务器**(`src/mcp/index.ts:1-35` 头注释):`codegraph serve --mcp`,支持三种运行模式——单进程直连(`CODEGRAPH_NO_DAEMON=1` 或探测不到 `.codegraph/` 时)、代理转发到共享后台守护进程(多个 agent 会话共用一份索引 + 一个 SQLite 句柄,省内存)、以及守护进程本体。**对外只暴露一个工具 `codegraph_explore`**(README:563-577):作者的实测结论是「一个强工具比一堆窄工具让 agent 选得更准、更省上下文」;`codegraph_node`/`codegraph_search`/`codegraph_callers`/`codegraph_callees`/`codegraph_impact`/`codegraph_files`/`codegraph_status` 默认注册但不在工具列表里露出,要用得设 `CODEGRAPH_MCP_TOOLS=explore,node,...` 环境变量重新点亮,或者直接用它们对应的 CLI 命令(`codegraph node`/`query`/`callers`/…)。
2. **纯 CLI**(不装 MCP 也能用):`codegraph explore/node/query/callers/callees/impact/files/status`,人和 agent 都能直接跑,输出可读文本或 `--json`。BW 现在用的就是这条路径(见 §2)。

**给主流 agent 装 MCP 的安装器**(`src/installer/targets/*.ts` 逐文件确认存在):`claude.ts`/`cursor.ts`/`codex.ts`/`gemini.ts`/`opencode.ts`/`hermes.ts`/`kiro.ts`/`antigravity.ts`/`copilot-vscode.ts`/`copilot-jetbrains.ts`/`copilot-cli.ts`。以 Claude Code 为例(`claude.ts:1-20` 头注释)干三件事:①往 `~/.claude.json`(全局)或项目内 `.mcp.json`(项目级——**不是** `.claude.json`,那个 Claude Code 根本不读,这是它专门修过的一个真实 bug #207)写 MCP server 条目;②(可选,`autoAllow`)往 `settings.json` 写 `mcp__codegraph__*` 权限白名单;③(可选,默认询问)往 `UserPromptSubmit` 写一个 `codegraph prompt-hook` 钩子,在结构性提问时提前注入 `codegraph_explore` 结果。**注意**:较早版本还会写一段 CLAUDE.md 说明块,但 `#529` 之后已经不这么干了——现在单一事实源是 MCP `initialize` 时返回的 instructions,`claude.ts:220-230` 的 `removeInstructionsEntry` 专门负责把旧版本残留的说明块摘掉。这一条对 BW 有直接参考价值,见 §2 的差距。

---

## 2. BW 现在怎么用它,差距在哪

BW 已经接入 codegraph,但**只接了 CI + CLI 半条腿,没接 MCP 半条腿**:

- **CI 双工作流**(`.github/workflows/codegraph.yml`):`verify` job 在每个 PR 上跑,装好 `scripts/codegraph-version` 钉住的版本(当前 `1.5.0`)后执行 `scripts/verify-codegraph.sh`——校验版本号一致、`.codegraph/codegraph.db` 存在且没被 gitignore、`codegraph sync --quiet` 后 `codegraph status --json` 显示 `initialized`/`index.state=complete`/`builtWithVersion` 匹配/无 pending 变更,外加一次 `sqlite3 ... PRAGMA integrity_check`。`publish` job 只在 `main` 分支 CI 绿了之后跑,重新生成快照、`git commit`+`push` 回 `main`(`chore(codegraph): refresh shared snapshot`,`git log` 里能看到近 5 次这样的自动提交,最新一次是 `d6678d2`)。这套「PR 上只验证、合入 main 后机器人单点刷新提交」的设计是为了让并行 PR 不会围着一个 20MB 二进制文件打架——这条思路在 `CONTEXT.md`(见下)已有草稿说明。
- **共享快照真的进仓**:`.codegraph/codegraph.db` 当前 19MB(172 文件、3468 节点、15242 条边,本地 `codegraph status --json` 实测),靠 `.gitattributes` 标记 `binary`、`.codegraph/.gitignore` 用 `*` + `!codegraph.db` 白名单只留这一个文件(WAL/SHM/日志等运行态文件不进仓)。
- **给 agent 的用法说明**:仓库根目录 `AGENTS.md`(整篇就是这一个主题)教 agent 优先用 `codegraph explore`/`node`/`callers`/`callees`/`impact` 而不是广撒网 grep,并且强调「视为已读,不要重复整份读文件」「改完代码跑 `codegraph sync`」「功能分支不要提交这个 db,发布是机器人的事」。`DEVELOPMENT.md` 有对应的中文版「共享代码图谱」小节(45-60 行)。
- **词表**:`CONTEXT.md` 里有一版「代码图 / CodeGraph」词条草稿(定义 + _Avoid_),但**没有合并进当前分支**——`git log --all` 显示它只存在于未合并的远端分支 `origin/claude/docs-gardening-2026-08-12`(commit `7dc571e`),当前 `CONTEXT.md` 里 `grep 代码图` 是 0 命中。这是一个待补的小尾巴,不是本次任务范围,但值得记一笔:写「资产盘点」相关文档时如果要用「代码图」这个词,词表还没有正式收录。

**真实差距(实测确认,不是猜)**:
1. **没有装 MCP**。本 worktree 根目录没有 `.mcp.json`,`~/.claude.json` 里 `grep codegraph` 零命中,`.claude/settings.json` 也没有 `mcp__codegraph__*` 权限或 `prompt-hook`。也就是说 codegraph 官方推荐的「装一次、agent 自动查」路径**从未跑过**;当前完全靠 `AGENTS.md` 文字说明,寄望 agent 自己在 shell 里敲 `codegraph explore`——这在有 Bash 工具的 CLI 里能生效(本次实测好用,见下),但换成走纯 MCP 协议接入的宿主(比如某些不给 Bash 权限的封装)就完全用不上图。
2. **只验证,没被交互式会话真正读过**:CI 的 `verify-codegraph.sh` 只关心「图是不是新鲜、完整性通过」,不产出任何给人看的内容。图是否真被 Claude Code 会话查询过,取决于每次会话是否照 `AGENTS.md` 的建议主动敲命令——没有强制,也没有埋点验证。
3. **本地实测发现一个对 BW 有意义的技术坑**:`codegraph callers "SqliteStore::recompute_signals"` 返回空列表,但这个方法明明被到处调用——原因是调用点走的是 `dyn Store` trait object 动态派发(`Store::recompute_signals`),不是具体类型的静态调用,静态分析在这类跨 trait 边界的场景下容易漏边。BW 大量用 `dyn Store` 这类 trait-object 设计(以及密封的信号推导类型),意味着 `callers`/`impact` 在 BW 自己代码库上有**已知的假阴性风险**,用它做「资产盘点/死码判定」时不能直接采信零调用者=死代码。

---

## 3. 会话屏右侧「代码结构侧栏」:codegraph 能不能顶上

先对齐现状:`mvp-blueprint-draft.md` 的**待拍-22 已经拍板**——右侧代码结构侧栏首版内容是「文件树 + 本活改动的文件与 diff + 分支状态」,**明确不做符号级大纲**(`mvp-blueprint-draft.md:299`),这和 Orca 预研的结论(`research/orca.md` §「右侧栏」:Orca 的右侧栏也只是文件树 + git 状态/diff/PR/检查,没有符号级大纲)是一致的。所以本节回答的是「以后想加符号级大纲时,代价大不大、有没有现成的查询接口」,不是建议现在就做。

一句话解释给非专业读者:**「符号级大纲」= 一个文件里有哪些函数/类,列成一个列表**(类似 VS Code 的 outline 面板),不是逐行代码。

**查询接口现成、能被 Rust 桌面应用低成本调用**:
- 子进程调 CLI,拿 `--json`,BW 用 Rust 的 `std::process::Command` 起一个短命令、解析 stdout 里的 JSON 就行,不需要嵌入 Node 运行时,也不需要直接读 SQLite schema(schema 版本号会随升级变,CLI 输出格式是它承诺维护的公开接口,直接读表是绕开这层保证)。
  - 文件级大纲(每个文件多少符号、多大):`codegraph files --filter <dir> -j` —— 本地对 `crates/bw-store` 实测,一行一个文件,`{path, language, nodeCount, size}`,几十毫秒级返回。
  - 具体某个文件的符号列表(名字/种类/签名/行号):`codegraph node -f <file> --symbols-only`——本地对 `crates/bw-store/src/sqlite.rs` 实测,153 个符号,每条 `name (kind) 签名 — :行号` 一行,文本或结构化都能拿(该命令目前没有单列 `--json` 输出符号表,只有 `node --symbols-only` 的文本块;要结构化就用 `codegraph query -k function -j`/`codegraph query -k struct -j` 按种类分别拉,`query` 支持 `-j`)。
  - 某个符号被谁调用/调用了谁/改动影响面:`codegraph callers/callees/impact <符号> -j`,本地对 `SqliteStore::recompute_signals` 实测过(结果见 §2 的假阴性坑)。
- 首版工作量估算(文件树 + 改动文件 diff + 每文件符号大纲,三者拼在一张侧栏里):
  - 文件树 + 改动文件 diff + 分支状态(待拍-22 已定范围):纯 `git` 命令(`git status --porcelain`/`git diff`/`git branch --show-current`),和 codegraph 无关,量级是「几个 git 子进程调用 + 一个树形渲染组件」,小。
  - 若之后想加「每个改动文件的符号大纲」这一层:每个改动文件多跑一次 `codegraph node -f <file> --symbols-only`(或用 `-j` 等价拉法),量级是「再加一次子进程调用 + 解析文本/JSON + 一个折叠列表 UI」,单文件几十毫秒,一次活通常改几个文件,不构成性能问题;真正的成本是**先跑一遍 `codegraph init`/CI 保证 `.codegraph/codegraph.db` 在场**(BW 已经做到,见 §2)和**处理 codegraph CLI 不在 PATH 的降级**(见 §6 风险)。综合评估:在「文件树+diff」已经做出来的前提下,加一层符号大纲是**小到中等**的增量工作(一个子进程调用链 + 一层 UI),不是新起一个系统。

---

## 4. 每个活的 agent 会话怎么从代码图里受益

codegraph 官方对 Claude Code 的推荐集成方式就是「装 MCP + 一次 `codegraph init`」(README「Get Started」§1-4、`src/installer/targets/claude.ts`),核心是**装一次、后面全自动**:MCP 服务器 initialize 时的 instructions 就是给 agent 的唯一用法说明(不需要额外维护一份 CLAUDE.md 文案,#529 之后官方已经把这条路堵死、自动清理旧版本残留的说明块)。

BW V4 的 agent 会话铺底路径是 `mvp-blueprint-draft.md` §2.6 定义的五层渐进加载(第 0 层 buddy 系统提示词 → 第 1 层 仓内 `AGENTS.md` → 第 2 层 本活技能 `SKILL.md` → 第 3 层 按需规范件 → **第 4 层 项目知识,明确点名包含 codegraph 索引**)——这个设计已经把 codegraph 放进了正确的层级(不在第 0 层常驻提示词里塞图查询指令,而是让 agent 按需去第 1 层的 `AGENTS.md` 里发现它)。对照 BW 现在的真实做法(§2):这条第 1 层路径**已经在跑**,`AGENTS.md` 就是这份「按需说明」。

三处可以做、成本都不高的加强(供 V4 设计参考,非本次任务范围):
1. **第 0 站铺底时顺带 `codegraph init`**:待拍-14/规范铺底那一步(`mvp-blueprint-draft.md:112`)往新接入项目铺 `standard-module-draft.md` 那一整套核心件时,可以在同一次 commit 里跑一次 `codegraph init`(如果项目还没有 `.codegraph/`),让 L4 从「有描述、要 agent 自己发现」变成「铺底那一刻就真实存在」。这和 BW 自己仓库现在的接入方式(CI 首次跑 `codegraph init` 建库)是同一件事,只是把「谁来跑」从 CI 挪到铺底流程。
2. **AGENTS.md 一行**:BW 自己仓库根目录的 `AGENTS.md` 已经是范本(见 §1「安装器」段的对照),铺底给用户项目时可以直接复用这份「优先用 codegraph explore/node/callers/callees/impact,不要一上来读整份源码」的说明,不用重新设计一版。
3. **要不要装 MCP,是个独立决策,不必现在拍**:装 MCP(`codegraph install --claude`)能让 `codegraph_explore` 自动出现在 agent 的工具列表里,免去「agent 记得敲 CLI 命令」这个不保证的环节;代价是要多写一份 `.mcp.json`/`settings.json` 改动逻辑,且要处理「用户机器上根本没装 codegraph 二进制」的降级(见 §6)。鉴于 BW 现在纯 CLI 路径在本次实测里工作正常(`AGENTS.md` 说明 + Bash 权限就够用),**MVP 阶段沿用纯 CLI + AGENTS.md 说明,不必强求装 MCP**,是更小的一步;要不要再加 MCP 是后续可以单独拍的一条,不在本文范围内下结论。

---

## 5. 运作活②「资产盘点与代码微重构」能不能靠它降本

`mvp-blueprint-draft.md:94/178` 已经把 codegraph 索引列为这张活的输入之一。核对能力边界:

- **能直接拿、成本低**:
  - 「哪些文件偏大/符号偏多」(常见的「该拆分了」信号):`codegraph files -j`,一条命令拿到全仓每文件的 `nodeCount`/`size`,按大小或符号数排序即可,和 BW 这次「减负重构」实践里手动数行数、按职责拆分 `lib.rs` 的做法(见 memory「BW debt-reduction refactor」)是同一件事的自动化版本。
  - 「改这个符号影响多大」:`codegraph impact <符号> -j`,微重构前先看一眼影响面,决定要不要动。
- **没有现成命令,得自己拼**:「死代码/从未被引用的符号」——codegraph **没有内置的 dead-code/unused-symbol 报告**(CLI Reference 里没有这个子命令,README 全文搜索「dead code」/「unused」零命中)。要拿到这个清单,得自己写一层:`codegraph query -k function -j` 之类按种类拉出全部符号,再对每个跑 `codegraph callers -j` 判断零调用者——这是 O(符号数) 次 CLI 调用,量级从「几十毫秒」变成「几秒到几十秒」(BW 库 3468 个节点级别),而且**§2 已经实测确认**:BW 大量用的 `dyn Store` trait-object 动态派发会让 `callers` 漏边、产生假阴性(判定「死代码」但其实是活的)。结论:能做,但不是「一条命令」的量级,且在 BW 自己的代码风格下需要人工复核 trait 方法这一类结果,不能直接把「零调用者」当结论自动生成 MR。
- **`codegraph affected`** 是另一个能直接用的现成命令:传入改动的文件(或 `git diff --name-only | codegraph affected --stdin`),算出哪些测试文件受影响——但 BW 的核心纪律是「不留单元测试,靠 E2E」(`CLAUDE.md`「核心纪律」第 6 条),这个命令对 BW 目前的验证方式帮助有限,除非以后测试策略变化。

---

## 6. 和 BW 已有想法的重叠、以及风险

**重叠**:如实说,**基本没有重叠**。codegraph 解决的是「agent 在一次会话里怎么低成本理解代码结构」,是纯粹的开发者工具/索引问题;BW 的核心想法(周计划、四层度量派生链、健康信号只能推导、技能蒸馏、同一件活绝不记两次的记账约束)是项目管理方法论层面的东西,两者不在一个维度上打架,也谈不上互相印证——如果说有交集,唯一的交集就是「运作活②要不要用它降低资产盘点成本」(§5),和「会话屏侧栏未来要不要加符号大纲」(§3),两处都已经如实说明是增量、非阻塞项。

**风险**:
1. **索引体积随仓库增长**:BW 当前 19MB / 172 文件 / 3468 节点,量级很小。README 的官方基准(`README:263-267`,作者测的,非本次复现)称 Swift 编译器仓库(27k 文件)全量索引约 100 秒、增量同步约 4 秒,Linux 内核(70k 文件、200 万符号)在 2 核 6GB 的机器上 12 分钟内跑完——数量级上 BW 短期不会撞到这个天花板,但**这些是官方自测数字,不是本次独立复现的结果**,标注来源以示区分。
2. **Windows 支持是真的,但有已知坑**:README:26/746 标「Windows: supported」,有 PowerShell 安装脚本;但 Troubleshooting 一节(README:846-856)明确点出 WSL2 + Windows 挂载盘(`/mnt/c` 之类)下共享后台进程用的本地 socket 不可靠,官方的兜底是自动退化成每会话独立进程,或手动设 `CODEGRAPH_NO_DAEMON=1`;WSL 和 Windows 两侧各自挂一份索引时还得用 `CODEGRAPH_DIR` 区分,否则 SQLite 跨文件系统边界加锁会出问题。BW 的目标用户在 Windows 上(`CLAUDE.md` 明确提到 Windows 主线),这条要留意,但不是「不支持」,是「WSL2 混合场景要按官方指引配置」。
3. **Node 依赖装进一个 Rust 应用里,是个真实的架构缝合问题**:codegraph 本体是 npm 包,BW 是原生 Rust 桌面应用,不会把 Node 运行时打进 BW 自己的二进制——现在 BW 的用法(CI 里 `npm install --global`,本地开发者机器上手动装)延续的是「codegraph 是外部工具,BW 只调用它的 CLI/子进程」这条边界,和 BW「AI 执行=本机 `claude` CLI」的既有模式(调用外部成熟 CLI,不自建)是一致的设计取向,不是新引入的风险类别;真正要处理的是**用户机器上没装 codegraph 二进制时的降级**——目前 BW 的 `AGENTS.md`/`DEVELOPMENT.md` 只教「怎么用」,没写「没装怎么办」,V4 铺底如果要把 codegraph 变成默认动作,得补上这条(检测不到 `codegraph` 命令就跳过,不阻塞铺底)。
4. **License 干净**:MIT,和 BW 自己(检查 BW 仓库根目录未见 LICENSE 冲突迹象)、以及 BW「能力底座用成熟 CLI」的既有原则不冲突。
5. **churn**:`CHANGELOG.md` 达 257KB,版本迭代密集(BW 当前钉的 `1.5.0`,§1 提到的多个 issue 号 #207/#411/#529/#644/#1466 等说明这是个还在快速修坑的项目);BW 已经用「`scripts/codegraph-version` 钉版本号 + CI 校验版本匹配」的方式防住了「CI 装的版本和快照构建版本对不上」这一类漂移,这条防线已经到位,不需要新增。

---

## 给 BW V4 的建议

1. **第 0 站铺底(规范铺底流程)**:如果新接入项目还没有 `.codegraph/`,铺底 commit 里顺带跑一次 `codegraph init <项目路径>`(需先探测 `codegraph` 命令是否在 PATH,不在则跳过、不阻塞铺底——对齐 BW「留白如实标注」的纪律,不假装图存在)。
2. **会话屏右侧代码结构侧栏**:MVP 就按待拍-22 已定的范围做(文件树 + git diff + 分支状态,纯 `git` 命令,和 codegraph 无关);符号级大纲留作后续增量,真要做时用 `codegraph files --filter <改动目录> -j` 拿文件级概览、`codegraph node -f <file> --symbols-only` 拿单文件符号表,都是子进程 + JSON/文本解析,不需要直接碰 SQLite schema。
3. **运作活②资产盘点与代码微重构**:「哪些文件该拆分了」直接用 `codegraph files -j` 按 `size`/`nodeCount` 排序,便宜好用;「死代码」不要指望一条命令,且要向承接这张活的技能(「资产盘点与微重构」)交代清楚 `dyn Trait`/接口实现这类动态派发在 BW 自己代码库里会让 `callers` 判断失真,微重构 MR 里对「零调用者」结论要留人工复核余地,不能直接自动删。
4. **AGENTS.md 一行**:BW 根目录现有的 `AGENTS.md` 已经是够用的范本,V4 铺底时原样复制给用户项目即可,不需要重新设计;是否再加一层 MCP 注册(`codegraph install --claude`)留作独立的后续决策,不在本次结论范围。
5. **顺手的小尾巴(非本次任务,供后续拾遗)**:`CONTEXT.md` 的「代码图 / CodeGraph」词条草稿已经写好但压在未合并分支 `origin/claude/docs-gardening-2026-08-12`(commit `7dc571e`)里,没进当前分支——后续文档要用「代码图」这个词之前,建议先把那次词表对齐合过来,避免本文档和正式词表各说各话。
