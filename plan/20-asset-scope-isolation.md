# plan/20 · 资产与配置的三层隔离:全局 / 项目 / 工作目录

**日期**:2026-08-05 · **触发**:用户指令「我们的工作空间、项目配置没有做隔离……参考 workbuddy、codex、claude 等,将工作目录、项目资产和全局做隔离,因为全局配置是基础配置,是为了能将工作目录的事情完善的。」
**性质**:把 plan/08 §1 的归属反转拍板(2026-07-16,「项目自有,Hub 只管搜索与共享」)真正落地——即 schema.sql 162-168 行自陈欠账的「查询收窄,P2 一次性做够,不留半破的收窄」。

## 1. 为什么(现状的病)

三张 hub 表(skill/agent/workflow_spec)2026-07-20 加了可空 `project_id`,但**从 store 查询、注入链到 UI 选择池,没有任何路径按「当前项目」收窄**:

- 在项目 A 的看板里能把活指派给项目 B 扫描进来的队友;
- 技能注入按名在**全库**匹配(`catalog.iter().find(|s| s.name == r.name)`),跨项目撞名谁先入库谁被注入;
- `record_skill_use_by_name` 是 `UPDATE skill … WHERE name=?` ——同名多行**全部** +1,settle-once 在作用域维度是破的;
- 「构建师」等五角色是全局单例,战绩混着所有项目的活;
- ProjectRail 把「他项目的行」也计进「共享」数。

唯一的硬边界只在 `delete_project` 的级联删除里生效。

## 2. 参照系共性(workbuddy 谱系 / codex / claude,2026-08-05 实地调研)

| 参照系 | 全局层 | 项目层 | 工作目录层 |
|---|---|---|---|
| Claude Code | `~/.claude`(settings + 用户级插件技能) | `<repo>/.claude` + `CLAUDE.md`(入库共享) | `settings.local.json` / 每个 worktree 一份(本机私有) |
| Codex CLI | `~/.codex/config.toml` + 全局 `AGENTS.md` | 项目根 `AGENTS.md`(入库共享) | 逐级向下,越近越优先 |
| multica | Workspace 资产(团队库) | repository 资产(仓库自管,云端**不收编**) | runtime-local(凭证与代码全留本机) |
| loop-buddy 设计稿 | base(基线) | override(局部改) | ——(promote:局部验证后升格回基线) |

提炼(与 BW 相关的四条):
1. **全局=基础默认,为项目服务**,不承载单项目知识;
2. **项目层=覆盖 + 专属资产**,共享靠复制/入库,不靠隐式引用;
3. **就近优先**(Claude Code 官方规则原话 "most specific wins"),或 multica 式**分池并列**、绝不隐式合并;
4. 局部验证后可**升格**回基线(loop-buddy promote;BW 已有实例:plan/19 冠军技能升基础技能)。

## 3. 三层对照(BW 版拍板)

| 层 | 语义 | BW 对应物 | 生命周期 |
|---|---|---|---|
| **全局层(基础配置)** | 跨项目复用的基础库与默认,为项目服务 | `project_id IS NULL` 的 skill/agent/workflow_spec 行(bw-standard、外部选型库、共享目录);bw-core 代码正本(playbook 五剧本、标准文本);全局 DB `workbench.db`;`BW_WORKSPACES` 根 | 与安装同寿;删项目不动它 |
| **项目层(项目资产)** | 项目自有、各自演化、各自记账 | `project_id = <p>` 的行:出生/补种的五角色副本、标配三件套副本、蒸馏技能、收录(Adopt)副本、项目自建 | 随项目;`delete_project` 级联(已有) |
| **工作目录(运行现场)** | 执行现场;登记可见,绝不冒充库资产 | `project.workspace_path`;种A 扫描行(`project-assets`,永不注入);BW 下发物(`PROJECT.md`、`.claude/standards/`、`.bw/metrics.toml`) | 随磁盘;重扫同步 |

「全局配置是基础配置,是为了能将工作目录的事情完善的」在 BW 里的操作含义:**全局层通过两条单向通道服务项目与现场——①出生/补种时把基础套件复制进项目(各立各的账);②收录(Adopt)时把共享资产复制进项目**。反向只有一条:蒸馏/验证过的项目资产将来可升格全局(promote,本次只留口不做)。

## 4. 作用域规则(R1–R5,本次拍板)

- **R1 · 池只见自有,基础库共享可见**:项目语境的**队友指派池**只列本项目的行(plan/08 完成标准原文:「指派下拉只出现自己的五个角色」);项目语境的**技能选择池**列本项目行 + 全局基础库行(标注共享——全局配置是基础配置,本就为项目服务);全局语境的表单(Hub 建工作流/编队、无项目的 cron)只列**全局**行——引用的作用域不得宽于引用者。**他项目的行在任何池里都不出现**。种A 排除规则维持不变。
- **R2 · 按名解析就近优先**:运行期按名解析(技能注入、cron 按名找 workflow)= 本项目行 → 全局行 → 如实落空;**他项目行永不命中**。同一作用域内撞名(外库导入可致)按既有稳定序取首行,如实不猜。
- **R3 · 记账行 == 注入行**:uses/战绩打点必须落在**被解析注入的那一行**(按 id,不再全表按名 UPDATE);解析落空则如实跳过。项目行的账归项目,共享行的账归共享,绝不因同名互相污账。
- **R4 · 撞名守卫按作用域**:名字唯一性在**同一作用域内**强制(全局一池、每项目一池);跨作用域允许同名——这是「复制归我」的天然结果,由 R2 消歧。
- **R5 · 收录=复制归我**:`AdoptIntoProject`(plan/08 原命名)把全局行复制一份归当前项目:新 id、`project_id=本项目`、描述尾注「引入自 <归属> · <日期>」、uses/战绩清零(新账)、`skill_file` 一并复制;**source 原样保留**(出处保真:Official 库文本仍是那个库的原文,T11 编辑翻转链继续生效);之后各改各的,与源脱钩。

## 5. 工程对照(本批切片)

| 件 | 内容 | 锚点 |
|---|---|---|
| W1 · 项目基础套件补种 | 每项目五角色 agent 副本(与全局行同源自 `playbook::role_agents()` 代码正本,`runs/wins` 从 0 立新账);出生时种 + Boot 幂等补种存量项目;判存收窄到 per-project。**技能不复制**(见偏差 5) | `bw-store/src/seed.rs`(新 per-project 种子)、`bw-app` Boot/CompleteCreation |
| W2 · 池收窄(R1) | op.rs 指派/技能池只取本项目;workflow_hub/cron_hub 表单池只取全局(cron 选了项目则本项目);ProjectRail「共享」计数只数 NULL 行;`AssignIssue` 命令层守卫(assignee ∈ 本项目 且非种A) | `op.rs:642-656`、`workflow_hub.rs:131/142/251`、`cron_hub.rs:387`、`project_rail.rs:48`、`lib.rs:7342` |
| W3 · 解析与记账(R2+R3) | `skills_prompt_block`/`standard_skill_block` 加 project 参数,就近优先;`finalize_run` 先解析后按 id 记账;store 增 `record_skill_use`/`record_agent_run`(by-id),废 by-name 全表 UPDATE;cron 按名找 workflow 本项目优先 | `lib.rs:2312/2435/2107-2112`、`sqlite.rs:2071/2271`、tick_scheduler |
| W4 · 撞名守卫(R4) | `guard_skill_name_unique` 加作用域参数;CreateSkill(Hub)查全局池,Distill 查项目池,Update 查本行所在池 | `lib.rs:1363-1380` |
| W5 · 收录(R5) | `Command::AdoptIntoProject { kind, id, project }`;skill/agent/workflow 三类;详情页「引入本项目」按钮(无活跃项目则不出现) | 新 handler、`component_detail.rs` |
| W6 · E2E 读回 | 临时 DB:两项目 + 全局库;SQL 读回——A 池不含 B 行、补种幂等不重复、收录后 project_id/尾注落值、同名就近注入且 uses 只 +1 于项目行;深链 `BW_OPEN`+`BW_PANEL` 渲染证明 | `examples/`、sqlite3 |

## 6. 如实记录的偏差(以源码为准,不擅改设计,偏差留痕)

1. **plan/08 P2② 「五套剧本复制成项目自有第 1 版」不再适用**:阶段剧本已改为 bw-core 代码正本、运行时现铸(`stage_workflow_with_playbook`,never persisted)——每项目天然同打法、run 记账本就按项目落,无需复制 DB 行。DB 里的五条模板行只是 Hub 目录条目。剧本的「随项目演化」将来若做,走「项目自有 workflow 覆盖同名剧本」的就近优先路,本次不做。
2. **plan/08 「找不到本项目行→如实跳过,绝不错记到共享行」收敛为 R3「记账行==注入行」**:plan/08 设想 P2 把一切复制进项目后,项目 run 只该命中项目行;但今天全局 Hub 工作流可在项目里运行(RunHubWorkflow)、其 skills refs 指向全局行——注入了共享行却不记账,是漏账不是诚实。故记账跟着注入走:注入了哪行就记哪行,跨项目仍绝不。
3. **HubSource::Adopted 徽记继续保留给「第三方插件引入」**(plan/12 §6 原语义):`AdoptIntoProject` 复制时 source 原样保留,出处保真;收录副本靠 `project_id` 归属徽记 + 描述尾注辨认,不翻转 source。
4. P2 的 ③体检 cron ④晨间建单 ⑤三指标与隔离无关,不在本批。
5. **bw-standard 技能(方法论五件 + 标配三件套)不复制进项目**:plan/16 §2 防线 2 的 Boot 自愈对账(`Official{bw-standard}` 行与代码正本 re-diff,漂移即覆写)要求全局正本唯一——复制进项目要么被对账覆写(「各改各的」破产),要么脱离对账(正本失真)。项目想改标配 → `AdoptIntoProject` 收录一份(R5,source 保留、就近优先注入拿项目版),全局正本继续保真。这也正是 Claude Code「插件技能全局只读 + 项目可覆盖」的同构。
