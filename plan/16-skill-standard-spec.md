# 16 · Skill 标准规范(v1)——以 Claude Code 官方最佳实践为事实源

> 缘起(用户 2026-07-28 原话大意):**当前 Skill Agent 可能没有按一开始的设想去展示**。
> 需要深度检查每一个 Skill,不符合标准规范的应被校正;标准规范直接参考 Claude Code
> 对 Skill 的最佳实践和 Playbook 撰写。目标状态:所有 skill 满足一套规范、可视化一致、
> 创建新工作流(会话)时可以从 SkillHub 选取关联 skill。

## 0. 三份事实源(2026-07-28 真实拉取,非记忆复述)

| 源 | 地位 | 本规范采用的核心约束 |
|---|---|---|
| [agentskills.io/specification](https://agentskills.io/specification) | Claude Code 遵循的 Agent Skills 开放标准;`skill_import.rs` 导入解析的既有锚点 | `name`:1-64 字符,仅小写字母/数字/连字符,不得以连字符开头结尾、不得连续连字符;`description`:1-1024 字符、非空,说清「做什么+何时用」 |
| [Skill authoring best practices](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices)(Anthropic 官方) | 撰写质量最佳实践 | description 第三人称、含具体触发场景与关键词;name 禁保留字 anthropic/claude、忌 vague(helper/utils);正文精简(SKILL.md < 500 行),细节走支撑文件渐进披露;术语一致;禁时效性内容 |
| [code.claude.com/docs/en/skills](https://code.claude.com/docs/en/skills) | Claude Code 产品文档 | description = 「what + when」,是 Claude 决定何时加载的唯一依据;listing 里 description 1536 字符截断;正文 < 500 行 |

BW 侧的既有 Playbook 正本:`bw-core/src/playbook.rs`(五阶段方法论技能 `stage_skills`)
与 `bw-core/src/standards.rs`(`.claude/standards/skill-standards.md`,写进每个项目工作区
的组件规范)。本规范与它们**同源不另立**:BW 自带技能的 desc/content 以代码/仓内文件为
唯一正本,规范条目写进 `skill-standards.md` 随项目分发。

## 1. 规范条目

BW 的 skill 是 DB 实体(`skill` 表),不是目录——`name` 即联合键(workflow `SkillRef`、
agent `skills` 列表、蒸馏溯源都按名 join),对应开放标准里「name 必须等于目录名」的锚定作用。

### S 系列(硬规,违反=待校正)

| # | 规则 | 出处 |
|---|---|---|
| S1 | `name` 匹配 `^[a-z0-9]+(-[a-z0-9]+)*$`,1-64 字符(小写字母/数字/单连字符;不以连字符开头结尾;无连续连字符) | agentskills.io |
| S2 | `name` 在技能库内唯一(联合键不容歧义)。全库级规则:命令层守卫拒绝重名新建/改名,审计全量扫描;卡面徽记只跑 per-skill 机检,不测此条 | BW join-key 约束 |
| S3 | `descr` 非空且 ≤ 1024 字符 | agentskills.io |
| S4 | `descr` 同时说清「做什么」+「何时用」——机检口径:含显式触发段(中文「适用」,或英文 "use when" / "use this" / "use it",大小写不敏感) | 两份 Anthropic 文档均列为 description 首要要求 |
| S5 | `content`(正文)非空——技能是可执行的方法,不是收藏夹书签;蒸馏技能尤其不允许空壳 | skill-standards.md 既有铁律 |
| S6 | 来源标注一致(存储层原始列级,徽记不测):`source='official'` 必须带非空 `official_library`;反向唯一的合法例外是 `self_built`+非空库名 = T11「改编自」留痕。旧编码 `official`+空库名 = 待归一 | BW `parse_hub_source` / `parse_adapted_from` 语义 |

### A 系列(提示,如实报告、不自动改写)

| # | 规则 | 出处 |
|---|---|---|
| A1 | 正文 ≤ 500 行,超长应拆支撑文件渐进披露 | 两份 Anthropic 文档 |
| A2 | `name` 不含保留字 claude / anthropic | best-practices |
| A3 | 蒸馏溯源成对:`distilled_from_issue` 有值则 `origin_agent` 也应有值(系统派生字段,缺损=排查信号,不许手补伪造) | BW 诚实来源纪律 |

### 分域执行(谁受硬规、谁只收提示)

- **BW 自产**(`bw-standard` 标准库 8 条 + 自建 + 蒸馏 + 会话内):S1-S6 全硬规,违规必须校正。
- **官方外库导入**(mattpocock-skills / superpowers / ecc 等):**原文如实保留,绝不擅改**——
  一切违规降为提示(如 `claude-handoff` 含保留字、`writing-skills` 正文超 500 行)。
  校正路径只有两条:上游修了重新导入;或本地编辑——编辑即脱离源头(T11),源翻转
  `SelfBuilt` + 留痕「改编自 <库>」,此后按 BW 自产受硬规。

## 2. 防线四层(规范不靠自觉,靠机器)

1. **命令层守卫**(`bw-app`):`CreateSkill` / `UpdateSkill` / `DistillSkillFromIssue` 对
   S1 名称格式与 S2 重名硬拒(诚实报错,与「名称不能为空」同一通道;三处共用同一个
   守卫函数,不各抄一份)。`ImportSkillPackage` 不拒——外库原文如实进(分域规则),
   违规靠徽记与审计可见。
2. **Boot 自愈对账**(`bw-app` Boot,扩展既有 P8 机制):`Official{"bw-standard"}` 行的
   `desc`+`content` 每次启动与代码正本(playbook `stage_skills` + seed 三件套常量表)
   re-diff,漂移即覆写——该库的行「与正本不一致即过期」是永久不变式(人一编辑,T11 就
   把它翻成 SelfBuilt,不再是 bw-standard 行)。另:bw-standard 全库(playbook 五 +
   标配三)的存量行若域读回 SelfBuilt 且 **pristine**——`content` 与正本逐字一致、
   `desc` 为今日正本或台账所载 pre-plan/16 旧正本(有界历史,非版本跑步机)、无
   `adapted_from` 留痕(T11 翻转过的行绝不回收,否则 desc-only 编辑会被洗掉)、非
   蒸馏行——Boot 升源为 `Official{"bw-standard"}`;其余诚实留在 SelfBuilt,绝不清洗
   用户编辑。
3. **SkillHub 徽记**(可视化一致):每张技能卡跑同一份 `bw-core::skill_spec` per-skill
   机检(S2/S6 是全库/存储层规则,由防线 1/2/4 负责,徽记不测)——
   有硬规违规,卡面出黄徽记「规范 · 待校正 n」;详情逐条列出(含 A 系列提示,弱化样式)。
   **合规=绿色隐身,不出声**(设计系统既有纪律)。SkillHub 卡片与项目栏组件详情共用同
   一组件,两处展示一致。
4. **审计指挥器**(`examples/audit_skills.rs`):对任意 DB 全量机检出报告;`--fix` 只做
   确定性校正(见 §4 台账),幂等可重跑;逐条 sqlite 读回为证。

## 3. 真实库现状(2026-07-28 `workbench.db` SQL 读回,不是估算)

- 65 条 skill:63 official(mattpocock-skills 41 / superpowers 14 / bw-standard 3 /
  旧编码空库名 5)+ 2 self_built;skill_file 100 行。
- **旧编码 5 条**(S6 违规):evidence-first / spec-to-tests / baseline-before-touch /
  fresh-eyes-funnel / breaking-drill——playbook 五阶段方法论技能,存量行是 pre-T2 编码
  `source='official'` + 空库名(域读回已按 SelfBuilt 兜底,但存储层与今日种子约定不一致)。
- **S1 违规 2 条**(自建中文名):`关键词关注面打分法`(agent「日报编辑」按名引用)、
  `多源体量控制法`(蒸馏自真活,uses=6,溯源成对完整)。
- **S4 违规**:BW 自产 10 条(playbook 5 + bw-standard 3 + 自建 2)desc 均无显式触发段。
- 55/63 官方导入 `stage_ref` NULL——**合规**:未人工归类=诚实「通用」,规范明确不猜。
- 六条 workflow(五标准 + ecc-guide)顶层 `skills_json` 全空;五标准工作流的技能绑定
  真实存在于 **phase 层**(`phases[].skills`,T16 设计)——SkillHub 反查只读顶层,导致
  「被 0 个工作流使用」的失真展示(见 §5)。

## 4. 校正台账(全部确定性,依据逐条注明)

| 对象 | 校正 | 依据 |
|---|---|---|
| playbook 五条(种子正本) | `seed_stage_entities_if_missing` 新种改 `Official{"bw-standard"}`;desc 补「适用:…」触发段 | §1 S4/S6;与三件套统一口径(plan/13 拍板「bw-standard=BW 自带标准库诚实标签」后到先例) |
| playbook 五条(存量行) | Boot pristine 升源 + desc 对账覆写 | §2 防线 2 |
| bw-standard 三件套 | desc 补「适用:…」;Boot 对账从 content-only 扩到 desc+content | §1 S4 |
| `关键词关注面打分法` | 改名 `keyword-focus-scoring`,desc 补触发段;同步更新 agent「日报编辑」的 skills 引用;中文原名保留在正文标题 | §1 S1/S4;audit --fix 台账内置映射,非运行时编造 |
| `多源体量控制法` | 改名 `per-source-volume-cap`,desc 补触发段;蒸馏溯源字段不动(SkillEdit 无此字段,结构上碰不到) | 同上 |
| 官方外库违规(保留字名/超长正文等) | 不改写,徽记+审计如实提示 | §1 分域 |
| S6 顽固行(旧编码 `official`+空库名且正文已被人改,pristine 升源不收) | audit --fix 把原始编码归一 `self_built`——与 `parse_hub_source` 已在读的语义完全一致,零行为变化,纯编码卫生 | §1 S6 |

## 5. 展示一致与工作流关联(「一开始的设想」落地核验)

- **反查修复**:`ui::vm::WorkflowDetailVm` 增 phase 层技能并集;SkillHub「被这些工作流
  使用」同时统计顶层 `SkillRef` 与 phase 绑定——五条 playbook 技能从「被 0 个工作流使用」
  回到真实(各被其标准工作流全 phase 注入)。
- **创建工作流选技能**:WorkflowHub 创建/优化/临时任务三个表单已挂 `SkillAgentPicker`
  (真实 SkillHub 目录、输入筛选、点击切换,按名落 `SkillRef{from:"SkillHub"}`)。
  E2E 证据:命令层建带技能引用的 workflow → sqlite 读回 `skills_json` 非空;
  `BW_HUB=skill` / `BW_HUB=workflow` 深链启动 stderr 渲染证明。

## 6. 工程对照表

| 事项 | 锚点 |
|---|---|
| 规范机检(纯函数,wasm 安全) | `crates/bw-core/src/skill_spec.rs`(新) |
| 命令层名称守卫 | `crates/bw-app/src/lib.rs` CreateSkill / UpdateSkill / DistillSkillFromIssue |
| Boot 自愈对账(desc+content+pristine 升源) | `crates/bw-app/src/lib.rs` Boot(P8 机制扩展);正本 `bw-core::playbook::stage_skills` + `bw-store::seed` 三件套表 |
| 种子源统一 bw-standard | `crates/bw-store/src/seed.rs` |
| 徽记与详情违规清单 | `crates/ui/src/vm.rs` SkillCardVm + `app-desktop/src/screens/skill_hub.rs`(component_detail 复用) |
| 反查含 phase 绑定 | `crates/ui/src/vm.rs` WorkflowDetailVm / `skill_hub.rs::workflows_using_skill` |
| 审计指挥器 | `crates/bw-app/examples/audit_skills.rs`(新) |
| 随项目分发的规范文本 | `crates/bw-core/src/standards.rs` SKILL_STANDARDS_MD(补 S/A 条目) |
