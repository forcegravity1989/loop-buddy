# 同一项目可被多台 Buddy 分别纳管

> **30 秒导读**:本文设计 V2 的第二项能力,给后续原型、开发与验收使用。**现在作数**:产品边界、信息归属、首到者/后来者纳管流程均已对齐;**Phase A(多人闭环)已实现**(`7a84e45`..`648ad48`,未 push);Phase B(回填)+ C(总览折线)设计已定、未开发。它不是团队协作设计;真正的多人协作与 Buddy 重构由另一个项目推导,本轮不提前实现。
>
> 初始意向见 [`roadmap.md`](roadmap.md) §2.2;产品命题见 [`../../plan/07-product-proposition.md`](../../plan/07-product-proposition.md);V2-①(调度逻辑简化)的设计见 [`issue-dispatch-prompt-skill.md`](issue-dispatch-prompt-skill.md)——两篇接口点见 §2.4。领域词以 [`../../CONTEXT.md`](../../CONTEXT.md) 为准。

---

## 1. 要解决什么

现实中,同一个项目会被多个人分别纳入各自的 Buddy。当前 Buddy 需要让这种真实使用成立,使更多人能用它持续纳管项目、暴露问题,并在实践中判断这套产品是否继续成为正式产品。

这不等于让一个 Buddy 管理多人。对任意一台 Buddy 来说,使用它的人始终是唯一的 Builder:

```text
现实中的甲                         现实中的乙
甲自己的 Buddy                    乙自己的 Buddy
甲是其中唯一的 Builder             乙是其中唯一的 Builder
甲自己的本地过程                   乙自己的本地过程
          \                       /
           同一个项目仓及其中的共同事实
```

一句话定义:

> **同一个项目可以被多台 Buddy 分别纳管;共同的项目事实从项目仓读回,每位 Builder 的工作过程留在自己的 Buddy。**

---

## 2. 已拍板的边界

### 2.1 Buddy 永远只认识一个 Builder

Buddy 不记录现实中还有哪些人参与项目,也不把现实组织关系搬进产品。

因此本需求不做:

- 成员、用户或项目成员表;
- owner、viewer 等身份与权限;
- 邀请、成员发现和在线状态;
- "谁执行了这次操作"的人员审计;
- 群聊、收件箱和审批流;
- Buddy 实例之间的成员关系。

`roadmap.md` §2.2 原先列出的"多人身份与权限模型"不是本需求需要回答的问题。它来自把现实中的多人错误映射成 Buddy 内部成员,应当由本决定取代。

### 2.2 选择"共同事实共享,本地过程各算各的"

本轮选择以下形态:

- 多台 Buddy 可以分别纳管同一个项目;
- 项目仓里的共同事实由每台 Buddy 读回;
- 每台 Buddy 的本地工作过程独立保留;
- 不同步多台 Buddy 的 SQLite 数据库;
- 不建立 Buddy 状态的双向合并机制。

真正让多台 Buddy 的看板、操作历史和人员动作保持一致,属于多人协作。另有项目负责从该方向推导并重构 Buddy,本轮不提前搭建半套同步架构。

### 2.3 不是注定废弃的临时功能

当前优先级是让 Buddy 被真实、持续地使用,从实践中暴露问题。这项最小能力应当自身完整、能够长期保留,而不是为了未来重构预埋一套当前用不到的多人架构。

如果真实使用证明当前 Buddy 有价值,它可以继续成为正式产品;未来项目是否吸收或重构它,由后续实践决定。

### 2.4 与 V2-①(调度简化)的接口点

V2-① 已在 v1 分支实现(未 push),把 issue 执行上下文拆成两个独立维度:**A · buddy 系统提示词资产**(所有 issue 必带,用户不可替换;正本在 `docs/buddy/`,运行时物化到工作区 `.claude/buddy/standards/`,不进用户提交)和 **B · skill**(按活选择,可替换默认)。V2-② 不破这个模型,三处接口点:

1. **资产不变性**:`docs/buddy/` + `docs/skills/` 是"无论 buddy 怎么重构都不变、可直接继承的真实资产目录"——V2-② 不碰这两处。
2. **多 workbench 物化一致性**:V2-① 的物化是 per-workspace、从 `include_str!` 烘进二进制的资产本地物化(代码在 `crates/bw-app/src/buddy_materialize.rs`,打包在 `crates/bw-core/src/buddy_assets.rs`)。每个 workbench 二进制里装同一份资产,各自物化天然一致。**V2-② 不建共享物化副本**——多台 Buddy 各自物化同一份二进制内资产即可,不改架构。
3. **只读查看者**:V2-① 说"能跑 issue 的 = 实际拥有该 workbench"。V2-② 按这个口径:**最简不做单独的只读 viewer 模式**。有 Buddy = 能跑 issue = 拥有这个 workbench;只想看的人 = 不点开工的 Builder,不为它单开一条态。

`.bw/` 目录分工(两层不撞):`.bw/` = 项目级正本(`project.toml`/`metrics.toml`/`connectors.toml`,在仓里、committed、各项目自有);`docs/buddy/` + `.claude/buddy/standards/` = V2-① 的通用规范与运行时物化副本(全局、烘进二进制、不 committed)。V2-② 只往 `.bw/` 加 `project.toml`,不碰 V2-① 那两层。

---

## 3. 信息归属

本设计沿用 `CONTEXT.md` 已有原则:**产品信息正本在仓,过程信息在 Buddy 本地库。**

### 3.1 多台 Buddy 应共同读到的事实

这类信息描述"项目是什么、现在交付到哪里",正本在项目仓或仓平台中:

- 代码、规格、原型和其他项目文件;
- 仓平台上的 Issue 与 PR/MR;
- 项目意图正本 `.bw/project.toml`(项目名/类型/对标/机会/北极星指针——首到者创建流写入,后来者读回;§6);
- 项目指标定义 `.bw/metrics.toml`(含北极星 + 采集方案);
- 采集装置登记 `.bw/connectors.toml`;
- 后续明确纳入项目仓的项目规范、提示词与技能;
- 可以从仓、提交历史、Issue、PR/MR 和真实采集装置重新读出的事实。

"共同"不表示 Buddy 之间互传数据。每台 Buddy 都独立从同一个项目仓或仓平台读取。

### 3.2 留在各自 Buddy 的过程

这类信息描述"我如何使用 Buddy 管这个项目",保留在每位 Builder 自己的本地库:

- 本地会话与交互式运行现场;
- 运行历史和本机执行状态;
- 本地定时任务及其运行历史;
- 交棒过程记录(append-only,阶段位置各算各的);
- 从本机过程产生、且不能从共同事实重新读出的本地账目;
- Buddy 的本地展示缓存与连接状态。

这些信息不在多台 Buddy 之间同步。两位 Builder 的本地过程不同,是合法且诚实的结果。

### 3.3 灰区判定(逐项落定)

- **项目意图(对标/机会/项目名/类型)**:已落定。正本是仓里 `.bw/project.toml`,SQLite 是缓存(对仗 `metrics.toml`)。首到者创建流写正本,后来者 `SyncProjectFile` 读回。后续改动走 PR 评审(改文件),不需要是 issue。
- **北极星 + 指标定义**:已落定。正本 `.bw/metrics.toml`,沿用既有 `SyncMetricsFile`。
- **指标历史观测**:已落定。接入时对非手填指标跑回填采集(脚本/github 连接器能产多久产多久),写成历史观测;observation append-only 支持任意 ts,不改 schema。手填指标不回填(人填的就是人填的,没有历史可捞)。
- **阶段位置与 handoff**:已落定。过程信息,各算各的,**不从仓读**。后来者接入后显式"对齐到哪一步"+ 留痕;首到者从 prototype 起。
- **Buddy 看板 Issue 状态 ↔ 仓平台 Issue 状态**:尚待对齐。后来者本地 Issue 全空;仓平台上的 Issue 是共同事实(Buddy 可从仓平台读回 issue 列表,但本地 Issue 行是否/如何从远端 issue 重建,本轮待定,见 §6 注)。
- **项目自有的技能/队友/工作流**:尚待对齐。正本应在仓里(产品信息),但当前技能/队友/工作流表无仓内正本,本轮不新建(留 follow-up)。

原则:能从共同项目事实重新得到的,不依赖另一台 Buddy 的数据库;纯属个人工作过程的,不伪装成团队共同状态。

---

## 4. 不做什么

本轮明确不做:

1. SQLite 文件同步、复制或双向合并;
2. Buddy 自建云端服务;
3. 成员、身份、角色和权限模型;
4. 多人实时协作、通知和评论;
5. 多台 Buddy 的运行历史、定时任务或会话共享;
6. 为未来多人重构预埋当前没有真实用途的字段和接口;
7. 把 GitHub/CodeHub 代码仓连接误写成 Buddy 实例之间的连接;
8. 共享物化副本(V2-① per-workspace 物化已天然一致,见 §2.4);
9. 单独的只读 viewer 模式(最简:有 Buddy = 能跑 issue,见 §2.4)。

---

## 5. 接入路径基线(改前读回 · 史实,2026-08-12)

> **横幅**:本节是 Phase A **动手前**的代码读回,保留作决策史实。Phase A 落地后以 §6/§7/§9 与源码为准——三件套 gate、`.bw/project.toml`、创建流三连读回已实现;阶段对齐 UI / 回填仍属 follow-up。

**5.1 接入已有仓今天就是一等公民。** Repo 卡"接入已有仓"→ `GithubOrigin::Existing` / `CodehubOrigin::Existing`,handler 真克隆,写 `workspace_path` + `remote_path` + connector。每个 Buddy 各自克隆到 `workspaces_root/<slug>`,天然隔离。

**5.2(已修) 三件套曾在接入路径重复建。** 改前 `seed_standard_issue_trio` 只看 `remote_path` 非空。Phase A 改为以 `.bw/project.toml` 是否存在为判据(有=后来者跳过)。

**5.3(已修) 接入时曾不读 `.bw/` 正本 / 无 project.toml。** Phase A 引入 `.bw/project.toml`,创建收尾对非空工作区跑 `SyncProjectFile` + `SyncMetricsFile` + `SyncConnectorsFile`。

**5.4(仍成立) 初始状态硬编码,不读仓的成熟度。** `create_project` 仍写死 `phase='cold_start'` / `active_stage='prototype'`;后来者阶段对齐 UI 仍是 follow-up(§9)。

**5.5(部分已修)** 意图五字段正本已在 `project.toml`;北极星仍在 `metrics.toml`。单人时代漏的不对称由 Phase A 补上意图正本,回填历史仍归 Phase B。

---

## 6. 首到者与后来者的纳管流程

判据:**仓里有没有 `.bw/project.toml`**。有 = 后来者;没有 = 首到者。

### 6.1 首到者(仓里无 `.bw/project.toml`)

一个仓可能从未被任何 Buddy 纳管过;第一个把 Buddy 用上去的人就是首到者。即便仓已有代码和历史,只要没 `.bw/project.toml`,Buddy 视为首到。

1. 走今天创建流:Repo 卡(新建仓 或 接入已有仓)→ Intent 卡(手填项目名/类型/意图/对标/三月成功标准)。
2. 创建流把意图写进新文件 `.bw/project.toml`(正本),跟 initial commit 一起 push(新建仓)或新建 commit push(接入已有仓)。产出时机是创建流,不是 issue——跟 README 一样是出生证。
3. 三件套照建(`gh issue create` ×3)——首到者该发启动包。
4. 跑 `SyncProjectFile` + `SyncMetricsFile` + `SyncConnectorsFile`(file→SQLite):仓里若已有 metrics/connectors 正本就读回,否则空、如实。
5. 对非手填指标跑回填采集(§3.3 指标历史):脚本/github 连接器能产多久产多久。
6. 初始阶段 = prototype(无 handoff 记录,诚实)。

### 6.2 后来者(仓里有 `.bw/project.toml`)

一个已被 Buddy 纳管过的仓(已有 `.bw/project.toml`),第二个人接入。

1. 走接入已有仓:clone → 检测到 `.bw/project.toml` → 走后来者分支。
2. 跑 `SyncProjectFile` + `SyncMetricsFile` + `SyncConnectorsFile`:从仓里正本读回意图 + 指标定义 + 采集装置,本地落缓存。Intent 卡跳过(或预填只读)。
3. **三件套不建**(判据:`.bw/project.toml` 已存在 = 已是 Buddy 项目,不需要再发启动包)。修掉 §5.2 的重复建问题。
4. 对非手填指标跑回填采集:脚本能捞多久捞多久,写成历史观测。修掉"Buddy 纳管才开始统计"的假象。
5. 阶段:Buddy 问一句"这个项目你现在接手到哪一步",后来者显式对齐 + 留痕(不从仓读 handoff——过程信息各算各的)。
6. 本地 Issue / 运行 / 会话 / handoff 全空(B 的本地过程,各算各的)。

> **Issue 重建(尚待对齐,本轮不实现)**:后来者本地 Issue 全空,但仓平台上的 Issue 是共同事实。本轮最简不把远端 issue 重建为本地 Issue 行——后来者的看板从零开始(只看得到接入后自己建的活)。是否给一个"从仓平台读回 issue 列表"的只读视图,留 follow-up。

---

## 7. `.bw/project.toml` 的合入模型

`.bw/project.toml` 是产品信息正本(**不是活**)。接入已有仓时主分支常受保护、不让直接 push,所以该路径走:**建分支 + 起 PR + Buddy 默认合入**;新建仓(owned)可直接上主干。合入失败用 Buddy tip 通知 Builder(对仗 ActionsBanner 的 Ok/Fail 回显)。

- **首到者写正本**(实现口径):
  - **新建仓(owned 工作区)**:写 `.bw/project.toml` + 直接 commit 上默认主干(仓是我们刚建的,通常无分支保护),再随落地推送一起上远端。
  - **接入已有仓(cloned,非 owned)**:写文件 + `bw/project-init` 分支 + 起 PR + **Buddy 自动合入**;合入后工作区收拢回默认主干。失败(无合入权限、分支保护禁自合等)→ tip 通知,Builder 手动处理。
- **为什么不破"Done 永不自动"**:那条铁律管的是**活**(Issue 的完成永远人点);`.bw/project.toml` 是配置文件、不是活,创建时自动合入自己的意图正本 ≠ 自动完成一件活。**issue PR 永不自动合入**——这条不变。
- **后续 intent 改动**(谁改了对标/机会):直接改 `.bw/project.toml`,走正常 git commit + PR 评审(跟改代码一样,Buddy 不掺合、不自动合入)。
- **后来者**:只读回正本,不写不推。

---

## 8. 回填采集设计(Phase B · 与 V1 Issue 3 配对)

Phase B 不是多人专属问题,而是"老项目接入 Buddy,指标不该只有一点或空"——单人接入成熟仓同样要。已对齐用户口径:近 30 天(≈4 周)历史,当前值 + 上周增量 + 4 周折线,每周一看进展。

**8.1 触发与范围**
- 接入时(首到者 on 成熟仓 + 后来者)跑一次回填;之后日常 `CollectMetrics` cron 继续追加当日观测(已有机制,`lib.rs:5171`)。
- 回填范围:近 30 天,每天一个观测点 → 聚成 4-5 个周点画折线。

**8.2 按 collect_kind 分流(诚实)**
- `script`:今天 `ConnectorDef`(`bw-engine/src/connectors_file.rs:53`)只有 `name/kind/script/command/output`,**没有日期参数**。回填需脚本支持按天取值——要么 `ConnectorDef` 加可选日期 arg(如 `--as-of <date>`),要么脚本一次产出 series。**脚本不支持日期 → 不回填,诚实留单点/Unknown**(不假装历史)。**这个约定写进 `docs/buddy/standards/connectors.md`(绑数据 skill 的契约,W1 那套标准手册)**——这样写脚本的 agent/人通过系统提示词的渐进加载就能知道"支持 `--as-of` 就能被回填";不支持就诚实留单点。V2-② 更新这个 W1 标准 doc(它是活的标准,不是冻结交付)。
- `github`:查询串已带 `@{Nd}` 日期占位符,回填 = 按周跑不同日期窗的同一查询。
- `manual`:不回填(人填的没有历史可捞)。

**8.3 写回与 DB 定位**
- 每个回填点 = 一条 observation(append-only,`ts` = 那天)。observation 表本来就支持任意 ts,不改 schema。
- **DB 是缓存,不是正本**——每个 Buddy 独立从源(script/github)采集回填,不共享 DB;"B 的库里有 A 的指标"这个担心因此消解:B 不读 A 的库,B 自己从同一个源捞。
- 不走"DB 不管、每次现取"那条路:script 采集有副作用且慢,observation append-only 还负载着手填指标与审计追溯,不能废。

**8.4 V1 Issue 3(总览折线)纳入本窗口**
- V1 产品化时总览界面难看,根因正是 Issue 3(总览重构)**设计了但没开发**。用户拍板:把 Issue 3 并进本窗口——它与 Phase B 是一对:Phase B 填历史观测、Issue 3 画折线,两边一起落,折线才有数据。
- Issue 3 的设计事实源 `docs/v1-prototype/issue3-overview-refactor.md`(高保真 mockup 已对齐,scope delta/提 issue 都完成,只差开发):业务指标卡 = `当前值+目标+灯 → delta 上周 → 按周折线从 observation 聚周`。本窗口按该设计照做,不重设计。
- Issue 3 守它的窗口边界(见该文 §3):不动 `collect_kind` 枚举收口、不动绑数据 skill 正规化(那些归 W2 Phase3);只动总览 UI(`op.rs`/`wall.rs`/`ui/vm.rs`)+ VM 数据契约(聚周折线/delta)。

---

## 9. 当前设计检查点

已确认:

- [x] 一台 Buddy 里始终只有一个 Builder;
- [x] 现实中的多个人,各自在自己的 Buddy 纳管同一个项目;
- [x] 选择"共同项目事实共享,本地过程各算各的";
- [x] 项目仓是共同事实的来源,SQLite 不在 Buddy 之间同步;
- [x] 真正多人协作留给另一个推导与重构 Buddy 的项目;
- [x] 本能力先服务真实使用,也允许经实践后成为长期正式能力;
- [x] 引入 `.bw/project.toml` 作项目意图正本 + 首到/后来者判据(承重墙);
- [x] `.bw/project.toml` 合入:首到者创建流建分支+起 PR+Buddy 默认合入(配置不是活,不破"Done 永不自动";issue PR 永不自动合入不变),失败 tip 通知(§7);
- [x] 三件套按 `.bw/project.toml` 存在与否判建(有就不建,没有才建);
- [x] 接入时对非手填指标跑回填采集,修掉"纳管才开始统计"假象;
- [x] 阶段位置各算各的,不从仓读 handoff(后来者「对齐到哪一步」UI/留痕仍 follow-up,见下);
- [x] 与 V2-① 接口点对齐:不碰 `docs/buddy/`+`docs/skills/`、不建共享物化副本、不做单独 viewer 模式;
- [x] 回填约定(`--as-of`/series)写进 `docs/buddy/standards/connectors.md`(W1 标准手册,绑数据契约),让写脚本的经系统提示词渐进加载就知道(§8.2);
- [x] V1 Issue 3(总览折线)纳入本窗口,与 Phase B 配对开发(§8.4)。

尚待对齐 / 留 follow-up:

- [~] 实现切片:A(多人闭环)已实现 + commit v1(7a84e45/fc3765a/b7302ca/648ad48)+ code-review 硬修(物化半写回滚、project-init 后收拢默认分支、首到者亦三连读回、resume/空默认 Skill toast);headless verify 以本轮门禁+读回为准;真 E2E(深链渲染/真 PR 自动合入/双 Buddy·cowelink)defer 用户。B(回填+更新 connectors.md)+ C(V1 Issue 3 折线)配对后行。
- [ ] 诚实 gap:三件套 gate 负向(有 project.toml → 跳过)无自动化 test,仅代码确认(正向有 verify_c8 覆盖);Windows 本机全量 `cargo test` 会因页面文件耗尽(os 1455)失败,非代码问题,CI 在 Linux 跑全量不受影响;
- [ ] `.bw/project.toml` 的确切字段集与格式(对仗 `metrics.toml` 的格式文档);
- [ ] 回填机制选型:按天调脚本 N 次(`--as-of`)vs 一次产 series(§8.2,Phase B 开发前定);
- [ ] 后来者"对齐到哪一步"的 UI 形态与留痕表;
- [ ] 仓平台 Issue 列表只读视图(本轮不实现,留 follow-up);
- [ ] 项目自有技能/队友/工作流的仓内正本(本轮不建,留 follow-up);
- [ ] 可重复执行的双 Buddy 验收剧本。
