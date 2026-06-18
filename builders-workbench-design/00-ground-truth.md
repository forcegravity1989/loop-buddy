# Builders Workbench · 共享真相基线（00-ground-truth.md）

> 这是 A 级红蓝对抗的**攻击靶子**。所有 agent 读这份后，红队攻、蓝队补。**只设计 MVP，不实现。**
> 日期锚定：2026-06-18。

---

## §0 这轮要回答的唯一问题
**面向「Builder」这个新角色的 Builders Workbench，它的 MVP 应该是什么？** 不是把旧的「司南 Loop 工作台」改皮，而是基于 `Skill → Plugin → Loop → Workbench` 这条新主线重新定义最小可用产品。

---

## §1 已被实证grounding的概念（来自 Claude Code 官方文档实时抓取 + Building Effective Agents）

| 概念 | 实证定义 | 来源 |
|---|---|---|
| **Skill** | 一个 `SKILL.md`（frontmatter + 指令 + 可选脚本/资源）。模型自动调用或 `/name` 手动调用，按需加载，可 `context:fork` 进子 agent。= **一项能力**。 | code.claude.com/docs/en/skills |
| **Plugin** | 带 `.claude-plugin/plugin.json` 清单的目录，打包 `skills/ + agents/(子agent) + hooks/ + .mcp.json + monitors/(后台监视) + 默认agent`，可版本化、经 marketplace 分发。= **打包好的角色/工具箱**。 | code.claude.com/docs/en/plugins |
| **Loop**（本项目术语） | Plugin **+ 一个 Goal + 循环执行（带反馈）**。架构 = 清晰 Prompt + 固定 Workflow 模板 + Agent Team + Goal。等价于 Anthropic 的 **"agent"**（LLM 自驱循环奔向目标）。 | Building Effective Agents |
| **Workflow vs Agent** | workflow = 预定义代码路径编排 LLM；agent = LLM 动态自主决定步骤与工具，循环直到达标/到检查点。**Loop 属于后者。** | Building Effective Agents |
| **Routine** | Loop 的**定时/重复**触发形态（cron/`/loop`/`/schedule`）。 | Claude Code |

**结论：`Skill→Plugin→Loop` 是真实的能力跃迁，不是改名。** 每升一级多出的是：Skill→Plugin 多出「打包+团队+工具+分发」；Plugin→Loop 多出「Goal+循环+自主」。

---

## §2 目标用户：Builder（采用用户给定定义，待红队证伪）
- 来源：用户称引自 Anthropic 最新 AI-native 角色组织定义——「组织 = X + ~10 个 Builders」，用户解读 **X = ALL**。⚠️ 此源**未能联网核实**（搜索后端故障），按用户定义当作工作假设。
- **Builder = 广义的独立开发者**，单人需具备全生命周期能力：洞察/需求分析 → 需求设计 → 需求开发 → 产品上线 → 产品运营 → 产品运维 → 集成构建 → 项目管理。
- 工作方式：不亲手做每一步，而是**操作一队 Loop**（每个 Loop 是一个自驱 agent）覆盖各生命周期环节；Builder 提供稀缺的人类判断 + 目标设定。

---

## §3 产品：Builders Workbench
**一句话**：Builder 用来「把 Skill 攒成 Plugin、把 Plugin 升级成 Loop、再让一队 Loop 覆盖整个产品生命周期」的操作台。

它要同时是三件事：
1. **Loop 的运行台**——监控/介入跨生命周期的多个 Loop（沿用前一版「只浮出需要你的 ~5%」的克制思路）。
2. **Loop 的工厂**——把已有 Plugin + 一个 Goal「毕业」成一个 Loop；把一次性实践沉淀成可复用 Routine/Loop。
3. **生命周期地图**——显示 8 个环节里哪些已有 Loop、哪些是缺口。

---

## §4 现有资产（家底）
- **~50 个 Skill**（已建）。
- **2–3 个 Plugin**（已建）。
- **2 个 Loop**（可认为已成形）：① 问题处理（≈ 需求开发/修 bug）；② 问题运维（≈ 产品运维）。
- **生命周期 8 环节，已覆盖 2，缺 6**：洞察/需求分析、需求设计、产品上线、产品运营、集成构建、项目管理 —— 这些都有「独立 Builder 做过的独立项目（即 Plugin）」，但**尚未总结成 Loop**。

---

## §5 v0 MVP 提案（红队请往死里打这一节）
**定位**：给**单个 Builder**用的、能操作「跨生命周期 Loop 队」的最小工作台。

**MVP 包含（4 件）**：
1. **Loop 库 + 运行台**：跑现有 2 个 Loop；首页只浮出「需要你处理」的少数 + 「自主运行中」的汇总（沿用前版克制设计语言，但术语全部大白话）。
2. **Loop 工厂（核心赌注）**：一条把「Plugin + Goal」做成 Loop 的流水线 —— 选一个已有 Plugin、给它一个目标、套一个 Workflow 模板、配一个 Agent Team → 产出一个可运行 Loop。**用它现场把第 3 个生命周期环节（建议「集成构建」或「需求设计」）从 Plugin 升成 Loop，作为活体证明。**
3. **生命周期地图**：8 环节 × 状态（已有 Loop / 有 Plugin 未成 Loop / 空白），一眼看清缺口。
4. **沉淀回路**：Loop 跑出的好实践 → 一键存成新 Skill/更新 Plugin（轻量，不做复杂治理）。

**MVP 明确不做**：8 个环节全建（只做框架 + 2 个种子 + 1 个新建作证）；多 Builder 协作；marketplace 分发；复杂的规则毕业治理（前版 trustTier/毕业三闸那套先不上 UI）。

---

## §6 脆弱的赌注（红队优先钻这些）
- **B1 Builder 是真角色还是幻想？** 「单人全生命周期」会不会在专业化面前崩塌（设计/运营/运维要的判断力天差地别）？X=ALL 是有意义的定义还是同义反复？
- **B2 每个生命周期环节都能干净地变成一个 Loop 吗？** 「洞察」「产品运营」这类开放、无明确终止条件的环节，套「固定 Workflow + 清晰 Goal」是否成立？还是只有「修 bug」这种闭合任务才适合 Loop？
- **B3 Loop 工厂是真机制还是 PPT？** 「把 Plugin 实践总结成 Loop」具体怎么发生？谁写 Goal、谁定 Workflow 模板、谁组 Agent Team？会不会沦为人肉手搓、根本不可复用？
- **B4 工作台会不会在 Loop 变多时崩？** 一个 Builder 真能同时盯住覆盖 8 环节的一队 Loop 吗？还是又回到 O(n) 人肉瓶颈、自主性是假的？
- **B5 MVP 是不是过载？** 4 件套是不是又想一口吃成全平台？真正的最小楔子（让一个 Builder 这周就获益）是什么？砍到只剩什么还成立？
