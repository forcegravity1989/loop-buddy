# 给减负线作者的收尾合入 prompt(用户 2026-08-18 拍板:先合入,再开 V4)

> **30 秒导读**:下面代码块是一段可以**原样粘贴**给正在 `claude/cut-legacy-engine-2026-08-18` 分支(worktree `prompt-migration-claude-md-39f7dc`)上工作的那个会话的 prompt。它不知道 V4 MVP 的存在,这段话告诉它 V4 要什么、别拔什么、怎么收尾合入。用完可删本文或留作史料。

```text
背景更新(来自用户,2026-08-18):V4 MVP 已开始规划,设计草案在本机另一个 worktree 里(尚未 commit,直接按绝对路径读):
- /Users/gravity/projects/builders-workbench/.claude/worktrees/linear-product-benchmark-21a776/docs/v4-prototype/mvp-blueprint-draft.md(全貌草案,重点看 §1 继承清单、§5 去留表、§7 建法)
- /Users/gravity/projects/builders-workbench/.claude/worktrees/linear-product-benchmark-21a776/docs/v4-prototype/standard-module-draft.md(规范铺底模块)
- /Users/gravity/projects/builders-workbench/.claude/worktrees/linear-product-benchmark-21a776/docs/v4-prototype/research/orca.md(Orca 预研)
只读它们,不要改它们(那个 worktree 归 V4 规划会话管)。你这条减负线(PR #102 十个 commit + 你叠上去的第二轮 cut-legacy-engine 九个 commit)是 V4 的前置:用户决定「先把它收尾合入,再开 V4」。**先把你手头正在做的那一步做完并 commit,再开始下面的收尾**;不做任何新功能。

一、V4 会保留并直接依赖的「器官」——收尾时一根手指都别碰(不删、不改签名、不改行为):
- 交互式执行器与嵌入终端整链(bw-engine interactive_cli.rs / pty_backend.rs、bw-app terminal.rs / issue_run.rs、app-desktop op/terminal_widget.rs):V4 的会话屏与「运作活自动开工」都走它,它不是旧引擎。
- Open Design 本机探活与嵌入(app-desktop open_design.rs)。
- 连接器(codehub / GitHub 纳入、拉 issue、探活、MergeIssuePr 合入)、.bw/project.toml 正本与后来者接入。
- 指标正本管道(.bw/metrics.toml、connectors.toml、采集脚本、CollectMetrics)、观测只追加、Derived<Signal> 密封与 recompute_signals。
- 定时任务的「建活(CreateAutopilotTask)」与「采集指标」两种模式(你已收敛成这两种,正合 V4)。
- 蒸馏(DistillSkillFromIssue)、hook 回收(hook_listener)、buddy 系统提示词 + 按活选技能(V2-①)、bw-standard 8 份技能的播种与对账、skill/agent 物化。
- Issue 状态机(Done 只能从 InReview 进)、settle-once 记账、交棒/观测 append-only 表。

二、V4 会重做或不带的(你不用管、也不要顺手再删——V4 新壳落地时统一处理):五阶段轴与阶段舱、六个面板、十个 Hub 图标、Workflow 库、Knowledge/Activity/Notify 屏。你第二轮已删的旧聊天式引擎、message 表、聊天 UI、旧一次性 claude -p 执行器与 Mock,方向与 V4 一致,保留你的删法。

三、收尾步骤:
1. 把 origin/main(最新 d6678d2,含 PR #101 与 docs/doc-boundaries.md、releases.md、LEFTOVERS.md、v4-prototype/ 这些新文件)合进你的分支。已知冲突 7 个文件 12 处:CLAUDE.md、crates/bw-app/src/lib.rs(4 处,你拆了它)、crates/bw-engine/src/interactive_cli.rs(3 处)、docs/code-schemes.md、docs/v1-prototype/LEFTOVERS.md、docs/v3-prototype/README.md、plan/README.md。解法原则:代码冲突以你的拆分结构为骨、把 main 侧 V3 的改动逐段搬进对应新文件(纯搬家,不改行为;用「行多重集」比对证明零丢失);文档冲突以 main 的写作纪律与文档边界为准。
2. 文档体系合一(两套整理互不知情):保留你的 docs/archive/ 与 docs/README.md;把你的 docs/BACKLOG.md 各条并进 main 的 docs/LEFTOVERS.md(它是「还没干的活」的唯一清单,见 docs/doc-boundaries.md),然后删 BACKLOG.md;不要移动、改名 docs/v4-prototype/、docs/releases.md、docs/LEFTOVERS.md、docs/doc-boundaries.md、docs/vN-prototype/;CLAUDE.md 保留 main 的「写作纪律」「不为向后兼容留旧路径」两节 + 你的三层导航,把 plan/00-05、09-12 等被你归档的路径引用改到 docs/archive/plan/ 下(例如设计 token 引用 plan/00-PLAN.md §6)。
3. 门禁全过:cargo fmt --all --check;cargo clippy --workspace --exclude app-desktop -- -D warnings;cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features;cargo check -p ui --target wasm32-unknown-unknown;./scripts/guard-kernel-ui-free.sh;cargo check -p app-desktop。
4. E2E 抽查(不依赖网关):real_demo 指挥器 --mock 跑一遍(它现在是你改的主环驱动)+ BW_OPEN 深链启动看 stderr [BW_OPEN];sqlite 读回一条活的 settled_at 只由人点完成写入。
5. push:你的第二轮分支 cut-legacy-engine-2026-08-18 是叠在 PR #102 的分支 claude/debt-reduction-refactor-2026-08-17 之上的,**把它快进推到 #102 那条分支上**(git push origin cut-legacy-engine-2026-08-18:claude/debt-reduction-refactor-2026-08-17),让 #102 一个 PR 覆盖两轮,不要另开第二个 PR。然后把 PR #102 描述改成覆盖两轮(第一轮六切片 + 第二轮拔旧引擎),写明:Windows PTY 后端未真机验证、macOS 已验;冲突解法与「行多重集」证据;文档体系合一做了什么;并加一行「V4 MVP 规划见 docs/v4-prototype/mvp-blueprint-draft.md,本 PR 是其前置」。不要合并,由用户合。
6. 更新 docs/LEFTOVERS.md 里因你而变的条目(旧引擎相关条目关掉并写收据),不新开平行清单。

四、汇报用人话:做了什么、结果如何、还剩什么;代号第一次出现要带一句说明;不要吹。
```
