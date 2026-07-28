# loop-buddy ↔ aihot 接线与核心资产补硬(样板间 v2)

日期:2026-07-27 · 状态:spec 定稿,待拆票执行
来源:本次 brainstorming 全记录(用户五次拍板,逐条记在下方「拍板记录」)
上承:`plan/13-github-mainline-creation-flow.md`(GitHub 主体化,D1-D12)、
`examples/README.md`(样板间自足化)、`docs/skills/north-star-discovery/SKILL.md`

---

## 问题陈述

样板间(`examples/aihot/bw-aihot.db`)对外展示的是「一个真实项目在 loop-buddy 里
被管起来的样子」。但把日常库、本地工作区、公开仓三边一起读回之后,真实状况是:
**loop-buddy 和 aihot 之间几乎没有接线,样板间里那根线是裁剪刀在生成时贴上去的。**

### 事实一 · 三条断链(全部 sqlite/gh 读回)

| # | 断在哪 | 读回证据 | 后果 |
|---|---|---|---|
| 1 | 日常库 → 仓 | `sqlite3 workbench.db "SELECT name,github_remote FROM project"` → aihot 那行**为空** | 在 BW 里对 aihot 建单不会开真 GitHub issue;`RunIssue` 不会提 PR;「验收=merge」没有接口 |
| 2 | 本地工作区 → 仓 | `git -C <ws> remote -v` **无输出**;本地 33 次提交,HEAD `890c436`;公开仓 32 次,HEAD `2bfdbab`;`890c436` 在公开仓不存在 | connector 探到的「工作区真实提交数=33」与任何人 clone 到的 aihot(32)对不上 |
| 3 | 样板间 → 仓 | `connector.config = /Users/gravity/Library/Application Support/.../aihot-b7971eca`;31 张 issue 全 `github_number=0 / pr_number=0`;`gh issue list` / `gh pr list` 均为 0;仓内无 `.bw/metrics.toml` | 样板间展示的是 plan/13 之前的降级形态,招牌功能一格看不到 |

链路 1 的根因不是配置遗漏,是**产品缺一个动作**:`set_github_remote` 全仓只有两处
调用,都在 `Command::CreateProject` 内([lib.rs:3154](../../../crates/bw-app/src/lib.rs:3154)、
[lib.rs:3252](../../../crates/bw-app/src/lib.rs:3252)),只覆盖「新建仓」与「克隆已有仓」
两条分支。aihot 走的是第三条 `(Some(path), _)` 绑定本地已有目录
([lib.rs:3121](../../../crates/bw-app/src/lib.rs:3121)),那条分支**只 `set_workspace`,
从不写 `github_remote`、也不建 github-repo connector**。
⇒ **任何用「绑定本地目录」建的项目此生挂不上仓,产品里没有补救入口。**

### 事实二 · 工作台的核心资产从未被真实使用过

```
competitive-analysis   4991 字   uses = 4     ← 真跑过
north-star-discovery   4963 字   uses = 0     ← 一次都没有
metrics-binding        4836 字   uses = 0     ← 一次都没有
```

`seed_standard_issue_trio` 第一行就是 `if proj.github_remote.trim().is_empty() { return Ok(None) }`
([lib.rs:2140](../../../crates/bw-app/src/lib.rs:2140))。aihot 建于 2026-07-20、
`github_remote` 为空 ⇒ 三张标配卡一张没种下 ⇒ 找指标 Skill 从没碰过它。

### 事实三 · aihot 的北极星是错的,而现行 Skill 拦不住它

aihot 现行北极星(`PROJECT.md` + `project.north_star`):

> 连续 7 天,每天都真实生成一份包含 ≥5 条命中关注面的摘要,无需手动修正即可读

三条硬伤,证据都在仓里:

1. **一条 cron 就能永久满分。** 本 spec §5 的 GitHub Actions 上线当天起,文件必然
   存在、命中必然够,这条北极星再也不会变红。能被自动化永久满足的指标是健康检查,
   不是北极星 —— 而产品四控制点的第四条正是「目标清晰且**难造假**」。
2. **阈值防了一个不存在的风险。** 守「命中 ≥5」,`digests/telemetry.json` 真实值是
   `raw:295 / hit:97 / items:29`;`docs/regression.md` 记录优化阶段把输出从 111 条
   砍到 30 条 —— 产品真实痛点是**上限**,北极星守的是下限。
3. **量供给不量价值。** `PROJECT.md` 自述痛点是「AI 热点圈信息**过多**」,价值兑现
   在读的人身上;「生成了一份文件」完全不触及那一侧。

而 `docs/skills/north-star-discovery/SKILL.md` 的虚荣指标判据是一张**三项黑名单**
(commit 数 / PR 数 / issue 数),并把「创作产出」明列为合法的北极星类别。
aihot 那条恰好落在「创作产出」里 ⇒ **这条 Skill 会给它放行。**
黑名单不是原则,核心资产在这里有一个可指认的缺口。

### 附带发现(写进来备查,不单独立票)

- **样板间唯一那盏非灰的灯是误读**:`每日命中率 目标≥8% 真实值32.9% signal=amber`
  —— 黄不是因为不达标(超标 4 倍),是观测过期被降级
  ([eval.rs:49](../../../crates/bw-core/src/derive/eval.rs:49) stale 把 Green 压成 Amber)。
  「数据太旧」与「产品变差」在墙上长得一样。
- **`examples/README.md` 数字错**:写「29 Done/1 InProgress/1 Todo」,真实是
  **29 done / 2 in_progress / 0 todo**。
- **样板间有一个库内查不到出处的数字**(裁剪刀的第二个漏洞,与 connector 绝对路径同源):
  样板间里 `competitive-analysis uses=4`,但样板间里挂了 `standard_skill` 的 issue **一张也没有**
  —— 那 4 次真实使用发生在「个人画像」项目上,而它已被裁剪刀按隐私边界删掉。
  `uses` 是全局累加、`DeleteProject` 不回退(这本身是对的:uses 记的是真实发生过的使用),
  但对公开发布的裁剪副本,它留下一个**违反「任何界面数字都能 sqlite3 独立查证」的数字**。
  处理见 P6 —— **不改计数**(改写真实计数才是造假),而是在 README 如实标注出处不在本副本内。
  顺带暴露一个产品口径问题:`uses` 是全局计数还是项目内计数,展示上有歧义 —— 不搭车,记录备查。
- **`.bw/metrics.toml` 强制北极星**:`pub north_star: NorthStarDef` 非 `Option`
  ([metrics_file.rs:100](../../../crates/bw-engine/src/metrics_file.rs:100)),
  一份没有 `[north_star]` 的文件整份解析失败。而 `write_charter` 在北极星为空时
  老实写「待定」([lib.rs:5617](../../../crates/bw-app/src/lib.rs:5617))、SQLite 列
  也可空 —— **只有正本文件强制**。产品一边说「无数据=Unknown 绝不假装绿」,一边
  逼每个项目第一天就填一个北极星。

---

## 拍板记录(用户,2026-07-27)

| # | 问题 | 拍板 |
|---|---|---|
| D1 | 关联落点要哪几层 | 四层全要:从今往后真跑 GitHub 主体化 / 修指错漏清的字段 / 指标正本进仓 / 样板间不腐烂 |
| D2 | aihot 下一段的真活 | 让 aihot 自己每天真跑(GitHub Actions) |
| D3 | 样板间靠什么不灰 | 打开即真采(能远程采的绑 `kind=github`,采不到的如实 Unknown) |
| D4 | 接线方案 | A · 接线优先,真活验收 |
| D5 | 推公开仓授权 | **已授权**:给本地工作区接 remote 并推 `890c436`(公开仓 32→33) |
| D6 | 北极星换哪条 | **先别定**。错的北极星没有指导意义,可以边做边想 |
| D7 | 产品指标组 | **先别定指标,先接线** |
| D8 | Skill 还是 Agent | 「找指标 Skill 是我们的核心资产,是工作台里比较关键的一个能力」⇒ 主干换成:补硬这条 Skill,让它在 aihot 上真跑第一次,**指标由 Skill 产出,不由对话拍板** |
| D9 | 执行分派 | 实施走 sonnet subagent;Opus 只做大脑(设计、拆票、监理、验收) |

D6+D7+D8 合起来是本 spec 的主干换向:**这次不定 aihot 的指标。** 指标是
`north-star-discovery` 的产出物,由队友带着补硬后的 Skill 在真活里产出、走 PR、人 merge。

---

## 设计

六件事,严格串行编号,`P1→P6`。P2 与 P1/P3 文件不冲突可并行,其余因共享
`bw-app/src/lib.rs` 与 `app-desktop` 热点文件必须串行(plan/13 附注「并行流风险」同款教训)。

### P1 · 补产品动作:`Command::AttachRepo`(给存量项目接仓)

**为什么**:事实一的根因。不补这个,后面五件全部无从谈起。

**做什么**(bw-app 命令层 + app-desktop UI,内核 crate 不动):

```rust
AttachRepo { owner: String, repo: String, push_local: bool }
```

1. `bw_engine::github::probe_repo("owner/repo")` **先探活** —— 仓不存在或无权限:
   如实报错、**一个字节不写库**(对齐 D12 软降级:失败不伪造)。
2. `store.set_github_remote(id, "owner/repo")`。
3. 补建 `github-repo` connector(`config = owner/repo`,任何机器上都成立)。
   `CreateProject` 的另两条分支都建了它,绑定分支漏了 —— 一并补齐。
4. 项目已有工作区且该工作区 `git remote` **为空** → `git remote add origin`;
   `push_local=true` 时推当前分支。**工作区已有 remote 且与目标不符 → 如实报错,
   绝不覆盖**(不静默改写用户的 git 配置)。
5. 事件沿用 plan/14 C14 的 `ActionProgress{Started→Ok/Fail}` 配对,`name` 用
   `"<项目名> · 接入仓库"`,一个 name 贯穿全程供 UI 配对。

**无 schema 变更**(`github_remote` 列已在),双守卫铁律不涉及。
UI 只进 app-desktop:项目设置面板加「接入仓库」,**仅对 `github_remote` 为空的项目显示**。

**读回验证**:
```bash
sqlite3 <db> "SELECT name,github_remote FROM project WHERE name='aihot 日报';"   # → forcegravity1989/aihot
sqlite3 <db> "SELECT name,kind,config FROM connector WHERE project_id=…;"        # → github-repo 行存在
git -C <ws> remote -v                                                            # → 非空
git -C <ws> status -sb                                                           # → 与 origin/main 同步
```

### P2 · 补硬核心资产:`north-star-discovery`

**为什么**:事实三。这条 Skill 是工作台的核心能力,但它的判据是黑名单不是原则,
aihot 是它放行错误的活样本。

**改 `docs/skills/north-star-discovery/SKILL.md`,四处**(正文正本在这个文件,
`seed.rs` 用 `include_str!` 编译进二进制 —— [seed.rs:183](../../../crates/bw-store/src/seed.rs:183)):

1. **自动化免疫检验**(进「硬性约束」段,与虚荣指标黑名单并列):
   > 起草完北极星,问一句:**假如给这个项目挂一条永不失败的定时任务,这条北极星会不会
   > 永久满分?** 会 → 它量的是系统在运转,不是用户在获益,退回重写。
   > 反例(真实):aihot「连续 7 天每天生成 ≥5 条摘要」在 GitHub Actions 上线当天起永久满分。
2. **价值兑现三段拆解**(插在「工作步骤」第 2 步之前,成为新的第 2 步):
   供给(做出来了)/ 使用(用户真的用了)/ 价值(用户因此变好了)。
   **北极星落在第 1 段一律退回。** 反例:aihot 痛点是「信息过多」,北极星整条在第 1 段。
3. **常见坑加一条「把供给当价值」**:对内容/信息类产品尤其致命 —— 「每天出一份」
   看起来太像成果。
4. **阈值必须用真实历史数据校准**(进「工作步骤」定 `target` 那一步):
   不能拍一个当前值远超的下限。反例:aihot 守「≥5」,真实 97 命中 / 29 条输出。

**同步改 `metrics-binding` / `competitive-analysis`?** 不改 —— 本次只动被证实有缺口的
那一条,不做顺手扩散(留白如实)。

**存量库回填(不做就白改)**:`seed_standard_issue_skills_if_missing` **同名即跳过、
不覆盖**([seed.rs:231](../../../crates/bw-store/src/seed.rs:231),注释白纸黑字
「内容更新走 UpdateSkill,不是重新 seed」)。改 SKILL.md 只影响全新库,而本次全部价值
在存量库(日常库 + 样板间)。因此必须加一次性回填:

- `app_meta` 守卫键 `standard_skill_content_refresh_v1`,先例三个在
  [legacy_migration.rs:153/172](../../../crates/bw-app/src/legacy_migration.rs:153)。
- Boot 时若守卫未置:对三条标配 Skill **按名比对 content**,不同则 `UpdateSkill` 覆盖,
  置守卫。**只覆盖 `official_library='bw-standard'` 的行** —— 用户自建/蒸馏的同名技能
  绝不动。
- `uses` / `distilled_from_issue` / `origin_agent` **一律不动**(派生字段,skill-standards
  铁律)。

**读回验证**:
```bash
sqlite3 <db> "SELECT name,length(content),uses FROM skill WHERE name='north-star-discovery';"
sqlite3 <db> "SELECT value FROM app_meta WHERE key='standard_skill_content_refresh_v1';"  # → done
# 正文里能 grep 到四处新增的关键句
```

### P3 · 补入口:`CreateIssue { standard_skill }`

**为什么**:「先定方法再干活」在今天的 `RunIssue` 路径上跑不通,两个断点:

- `distilled_skills_block` 只挑 `distilled_from_issue` 非空的技能
  ([lib.rs:1817](../../../crates/bw-app/src/lib.rs:1817)) ⇒ **自建/标配方法论技能永远不会被自动注入**。
- `Command::CreateIssue` 没有 `standard_skill` 字段([lib.rs:5138](../../../crates/bw-app/src/lib.rs:5138)),
  只有 `seed_standard_issue_trio` 能设 ⇒ 手工建的 issue 挂不上技能。

而 `standard_skill_block` 本身**已经是通用的** —— 按名在技能库里查,任何 slug 都能解析,
解析不到就诚实返回 `(empty, [])`([lib.rs:1862](../../../crates/bw-app/src/lib.rs:1862))。
机制齐备,只差入口。**与 P1 同构。**

**做什么**:`Command::CreateIssue` 加可选字段 `standard_skill: String`(默认空,
现有调用点全部传空 ⇒ 行为逐字节不变);app-desktop 建单表单加一个「关联技能」下拉
(可空,列出技能库里 `content` 非空的行)。`uses` 计数由 `run_issue_now` 既有路径自动
生效,不新写记账代码(settle-once 由既有 `record_skill_use_by_name` 保证)。

**读回验证**:建一张挂了技能的 issue → `sqlite3 "SELECT standard_skill FROM issue WHERE …"`
→ `RunIssue` 后 `sqlite3 "SELECT uses FROM skill WHERE name=…"` 恰好 +1(不是 +2)。

### P4 · aihot 接线(用 P1 的新动作,不裸改 SQL)

1. 日常库对「aihot 日报」执行 `AttachRepo{forcegravity1989/aihot, push_local:true}`
   → 链路 1+2 一次接上,`890c436` 推上公开仓(32→33,**已获 D5 授权**)。
   历史不重写、只往前长 ⇒ 样板间 666 条 artifact 记的 `git_commit` 引用仍有效。
2. 裁剪刀补一刀(`crates/bw-app/examples/build_aihot_fixture.rs`):
   **删掉 git-repo connector,只留 github-repo connector**。没有本地工作区就不该有
   本地仓 connector,留一个指向不存在目录的 connector 比没有更糟。

**读回验证**:P1 的四条 + `gh api repos/forcegravity1989/aihot --jq .pushed_at` 变新。

### P5 · aihot 真活(两张真 GitHub issue,顺序不可换)

两张都走完整环:BW 建单=真开 GitHub issue → `RunIssue` 队友真改文件 → `open_pr` →
issue 转 InReview → **人 merge** → `MergeIssuePr` 记账 → issue 关闭转 Done。
**执行器只提 PR,永不 merge**(plan/13 D3/D11)。

**第一张 · 找指标**(挂 `standard_skill = north-star-discovery`)
- 这是核心资产的**第一次真实使用**,`uses` 0→1。
- 交付物:`.bw/metrics.toml`(正本)+ `docs/metrics-rationale.md`(人读推导),
  两份都走 PR 进 aihot 仓 —— D14「改指标和改代码过同一道门」。
- **aihot 的北极星与三层指标由这一步产出,本 spec 不预设。**(D6/D7/D8)
- 前置:仓内应有 `docs/competitive-analysis.md` 供 Skill 读;**当前不存在** ⇒
  按 Skill 自身规定,在 `docs/metrics-rationale.md` 里如实写「本轮无竞品分析报告输入」,
  不假装读过,也不为此额外补一张竞品分析票(留白如实,不扩散)。
- 若产出的北极星 `collect.kind` 落在 `connector`/`bw` ⇒ 样板间里如实 Unknown,
  并在 `examples/README.md` 写明「要它亮就克隆仓库」。**不为了让灯亮而改指标定义。**

**第二张 · aihot 每日真跑改 GitHub Actions**
- 为什么排第二:它是「自动化免疫检验」的活教材 —— 先有补硬后的指标,再上自动化,
  才能真实演示「自动化没有把北极星刷绿」。
- 交付物:`.github/workflows/daily.yml` —— 定时 → `python -m aihot.main` 真抓
  HN/arXiv → 写 `digests/YYYY-MM-DD.md/.html` + 更新 `telemetry.json` → 提交推回。
- **issue 正文必须写进这六条可预见的坑**(它们后续是这件活蒸馏成技能时的正文骨架):
  1. `GITHUB_TOKEN` 推的 commit **不触发后续 workflow**(防递归);默认权限只读,
     不显式 `permissions: contents: write` 则 push 失败。
  2. `schedule:` 是 **UTC**,GitHub 明确不保证准时(高峰延迟甚至跳过)⇒「每日」的
     断言必须落在产出文件的日期上,不能信 cron 表达式。
  3. 仓库 **60 天无活动,scheduled workflow 自动停用**。
  4. 抓取失败时**绝不提交半截或空的 digest** —— 否则「文件存在即达标」被自己的 CI 造假。
  5. runner 的 IP 抓 HN/arXiv 可能被限流:**本机能跑 ≠ CI 能跑**,必须先
     `workflow_dispatch` 手动真跑一次成功,再开定时。
  6. 每天一个 commit 会把历史刷成噪音 ⇒ 先决定 digests 走不走单独分支。
- 跑完后 `DistillSkillFromIssue` 蒸馏一条**复利技能**(带 `distilled_from_issue`),
  与 P2 补硬的**方法论技能**两种出身并存 —— 样板间正好展示这个区别。

**诚实风险(不掩盖)**:真执行器走本机 `claude` CLI,网关 529 / 配额抖动可能失败。
失败就如实停在原地重试,**绝不降级成 mock 冒充成功**。

### P6 · 重裁样板间 + 文档改对

- 跑 `cargo run -p bw-app --example build_aihot_fixture`(含 P4 的 connector 修补)。
- `examples/README.md`:
  - 数字改对:**29 done / 2 in_progress / 0 todo**。
  - 新增「哪些灯远程可点亮、哪些必须克隆仓库」一节;写明 `collect_github_count` 走
    `gh api`,对方没 `gh auth login` 会失败 —— 失败**不写观测**,指标保持 Unknown,不假装。
  - 如实写明:**31 张存量 Issue 是 plan/13 之前的本地身份,按 D2 不迁移**;从第 32 张起
    才是真 GitHub issue。样板间同时展示两代形态,并标清楚。
  - **标注 `competitive-analysis uses=4` 的出处不在本副本内**(见「附带发现」):那 4 次
    发生在已按隐私边界删除的项目上,本库内无法追溯。**不改计数** —— 改写真实计数才是造假;
    如实说明才是解法。

---

## 范围外(明确不做)

- **不补开 31 张存量 GitHub issue**(plan/13 D2 + Out of Scope 白纸黑字:保持本地身份、
  如实留白、不假装迁移)。给没发生过的事补痕迹,违背「不假装」。
- **不定 aihot 的北极星与三层指标**(D6/D7/D8) —— 那是 P5 第一张活的产出物。
- **不新建队友**:「把本机定时迁成 CI 定时」是方法不是人格;新建一个 `0/0` 战绩的队友
  去干第一件真 GitHub 活,样板间里只多一个空壳。用现成「运维师」(0/0)或
  「日报编辑」(2/2,aihot 专精)。
- **不开 `CollectKind::GithubFile`**(从公开仓直读 telemetry.json 的新采集口):方向成立,
  但它是引擎能力扩展,不混进这条线;等 P5 第一张活产出真实指标后再判断是否需要。
- **不改 `.bw/metrics.toml` 的北极星必填约束**:已记在「附带发现」,单独立票再议 ——
  改它会动 `MetricsFile` 结构与 `SyncMetricsFile` 语义,不该搭这趟车。
- **不修「stale 降级成 amber 与真不达标不可区分」**:同上,已记录,不搭车。
- 不动 `metrics-binding` / `competitive-analysis` 两条 Skill 的正文。

---

## 验证纪律(照 CLAUDE.md,不写单元测试)

每件 P 票完成即过门禁全套,行为正确性靠 E2E 读回:

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop
```

每件的读回证据写在各 P 段内。**演示/报告里的每个数字都从真实 DB 或 `gh` 读出,
绝不硬编。** UI 编译过即可,行为在 bw-app 命令层 + E2E 兜底 —— 如实,不假装 UI 测试。

**操作真实资产前的护栏**:P4/P5 会写用户的公开仓与日常库。
- 日常库先备份(`workbench.db.bak-<日期>-aihot-integration`),沿用既有备份命名。
- 推公开仓已获 D5 授权,仅限接 remote + 推 `890c436` 这一个提交。
- **首次真开 GitHub issue 前向用户报告并等确认**(plan/13 测试拍板:真实账号 E2E
  单独成票,执行前报告并等确认)。

---

## 执行分派(D9)

- **实施**:sonnet subagent,一票一个,独立 commit,代号前缀 `P1-`…`P6-`。
- **Opus(主 agent)**:设计、拆票、监理、门禁与读回验收、`/code-review`。不亲自实施。
- **串行约束**:P1 → P2 → P3 → P4 → P5 → P6 **全部串行**。
  ~~P2 可与 P1 并行~~ —— **实施时推翻**(见「实施偏差」E1):P2 的 Boot 回填必须
  在 `bw-app/src/lib.rs` 挂钩,与 P1 改 `Command` 枚举/dispatch 在同一文件,
  同 worktree 并发会互相覆盖。
- P5 两张真活需要用户在场点 merge,不可无人值守跑完。

---

## 实施偏差(落地后回填,以源码为准)

| # | 票 | spec 原文 | 实际做法与原因 | commit |
|---|---|---|---|---|
| E1 | P2 | 「P2 可与 P1 并行」 | **推翻,全串行**。P2 的 Boot 回填要挂在 `bw-app/src/lib.rs`,与 P1 改 `Command` 枚举/dispatch 同文件,同 worktree 并发会互相覆盖 | — |
| E2 | P1 | 顺序 `探活→写 github_remote→建 connector→接本地 origin` | **改成 `探活→接本地 origin→写 github_remote→建 connector→推送`**。原顺序下第 4 步撞 `Mismatch` 时前两步已写库,而 `AttachRepoCard` 只在 `github_remote` 为空时渲染 ⇒ 卡片消失、**用户再无 UI 入口重试**,是死路。改后「失败即零写库」覆盖到 Mismatch。**推送仍在写库之后**是有意保留:推送失败时 `github_remote` 已设是事实正确的(仓确实关联上了),用户可自己 `git push` | `94c8ed4` |
| E3 | P2 | 「Boot 时……`UpdateSkill` 覆盖」 | 不走 `Command::UpdateSkill` 的 dispatch,直接 `store.update_skill(.., flip_to_self_built: false)`。后者的 T11「编辑即脱离源头」会在 content 变化时把 `Official → SelfBuilt` —— 那条规则是给**人**从官方正文分叉出去用的,这次方向相反(官方正文追平自己),翻转是错的 | `f101cf4` |
| E4 | P2 | (未规定) | 回填不做 `db_path` 线程化与文件备份(三个 `MigrateLegacyShellsIfNeeded` 先例都做了):这次是对 ≤3 行做定向 `content` UPDATE,不是破坏性删除/清洗通道 | `f101cf4` |

E2 的读回验证:`probe_repo(19) → reconcile_local_remote(42) → set_github_remote(54) → create_connector(67) → push_current_branch(105) → ActionState::Ok(119)`(行号为 `AttachRepo` 块内相对行)。

E3/E4 的读回验证(独立复核,非 agent 自报):真实日常库副本跑 Boot 前后 ——
`north-star-discovery` `length(content)` 4963→**6305**;三条标配 Skill 的 `uses`
**一个都没变**(`competitive-analysis` 仍是 4);`source` 仍 `official`/`bw-standard`;
自建技能(`关键词关注面打分法`/`多源体量控制法`)字节未变;
`app_meta[standard_skill_content_refresh_v1] = done`;二次跑幂等。

## 留白与偏差(如实记录)

- `docs/competitive-analysis.md` 在 aihot 仓里**不存在**,P5 第一张活按 Skill 自身规定
  降级处理,不额外补票。
- P2 的四处补硬全部从 aihot 这一个真实案例提炼。样本量=1,如实标注:它们是**可预见的
  坑的归纳**,不是统计结论;后续在别的项目上跑到反例应回来修。
- 「样板间不腐烂」(D3)在本 spec 里只完成一半:P5 第二张活让仓每天有真提交,但
  BW 侧能远程采的仍只有 `search/issues` 一路。产品指标要远程点亮,取决于范围外那条
  `CollectKind::GithubFile` —— **如实标注为未建,不假装已解决。**
