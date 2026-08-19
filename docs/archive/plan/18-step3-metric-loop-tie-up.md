# 18 · step3 收尾：项目级业务指标闭环（可见→可采→点亮→上卷）+ skill 对齐 + 创建流诚实性

> 本文件是 **step3 收尾窗的执行 plan**。设计层唯一事实源 `plan/06`；命题 `plan/07`；MVP `plan/08`。
> **和 `plan/17`（run 调度重做：解冻+worktree+中止+串行）平行不重叠**——17 管"跑一件活怎么不冻/隔离"，18 管"跑出来的指标怎么采/点亮/看见/上卷"。可并行 worktree 开发（见 §6）。
> 决定是和用户一问一答 grill 出来的，偏差处标了。实践日志一手记录在 `iterations/PRACTICE-buddy.md`。

> ℹ️ **本文 L 编号有两套含义,读到先辨(2026-08-06 标注)**:同一个字母 `L` 在本文里指两件完全不同的事,套错会读错意思。
> 1. **L1/L2/L3**（如 §1.1「北极星 + L1/L2(引领) + L3(滞后)」、§1.2/§1.4/§1.5/§3/§5 等多处）：指某个具体项目自己定的业务指标标签（例如 maas-locate 项目里 L1 = 报告覆盖率），标签随项目具体定义，不是代码里的固定层级；这套用法也出现在 `plan/19`。
> 2. **L4、L6**（如 §1.3「L6 聚合缝」、§3 第 4 项「L6 聚合改」、§4/§5/§6）：指代码里「度量派生链」的固定层级编号（L0 观测 → … → L6 项目聚合信号），权威定义在 `plan/03` §2.5，长期有效、不随项目变。
> 两套编号只是历史撞车，不是笔误——已登记进 `docs/code-schemes.md`（本仓库代号撞车的统一查询表），拿不准某处 L 指哪套时去查那张表。

---

## 0. 这一窗要解决什么

step3（三件套）原计划完成竞品分析→找指标→绑数据。竞品分析卡 bug②（联网墙）跳过；找指标+绑数据 run 跑通了，但**指标进表后看不见、点不亮、改不了**——buddy 价值不可见。读回 maas DB 实测：13 条 metric 全 `signal=unknown`/`hit=0`，但 cron 其实真采到了 codehub 远端数（开放 Issue=11、已合入 MR=9）+ BW 自记阶段完成数（1/2）——**采到了却看不见、点不亮**。

这窗把「指标进表→采数→点亮→UI 看见→人能改→上卷项目健康」五截断点修通，让 buddy 在 maas 上价值可见、且可复制给同事项目。

---

## 1. 决定总览（grill 收口）

### 1.1 指标分两层（用户定的分层）

| 层 | 归属 | 内容 | 进健康灯？ |
|---|---|---|---|
| **层 A·业务指标** | 项目级（`stage_kind=NULL`） | 北极星(定界采纳率) + L1/L2(引领) + L3(未采纳率滞后)，全通用脚本 connector 采 | ✅ 上卷 |
| **层 B·buddy 固有项目管理指标** | 通用（所有项目有） | 开放Issue/已合入MR/阶段完成/每周结算/每周合并PR | ❌ 只当现状数显示 |

**层 A 删两条 manual**：客户端拦截准确率、平均闭环时长——不清不白、不知怎么统计维护，不做（不浪费精力在问号上）。
**层 A 资产 5 条**（CN/DS/DT/EC/NK）：本次 (c) 先去掉，等 bw 本地 scan 采集器做了再加（另一种采集模式，单独趟）。
**层 B 不混进业务卡**：buddy 固有指标自己统计自己渲染，不掺和业务北极星/引领/滞后。

### 1.2 采集：通用脚本 connector（新 kind = `script`）

用户点1：clouddragon 是"项目集连接器按 buddy 方式弄进来激活"。但更优是**通用「项目侧脚本 connector」**——可复制给同事项目，不局限 clouddragon。

- **connector kind 枚举 + collect_kind 枚举都加 `script`**（`bw-core/src/model.rs` CONNECTOR_KIND_* + `bw-engine/src/metrics_file.rs` CollectKind + `bw-store/src/lib.rs` CronMode 周边）
- **connector 表行**（项目级实例）：`kind=script`，config 存 ① 脚本路径（相对工作区，如 `governance/workspace/clouddragon/refresh_data.py`）② 输出文件路径（`data.json`）③ 跑脚本的命令
- **probe**：文件存在检查（轻，不真跑）；**collect arm**：shell-out 脚本 → 读输出 JSON → 按每条 `metric.collect_query`（`data.json:<json 字段路径>`）取值 → 写 observation → recompute
- **maas 实例**：connector 接 `refresh_data.py`（内部 fetch CloudDragon + derive + 写 `data.json`，buddy 不管内部）；北极星/L1/L2/L3 各自 `collect_query=data.json:<字段>`
- **可复制**：任何项目有"产出指标值的脚本"就能接
- **诚实标注（偏差）**：grill 中途说过"不加第5种 collect kind、不碰解析器"——**做通用脚本 connector必须加 `collect_kind=script`**，这是中改（碰 CollectKind enum + metrics_file.rs 解析 + `docs/metrics-toml-format.md` 枚举）。**推翻了那句**，因为不做这步 script connector 采不了。skill prompt 那句"不加第5种 kind"仅指 skill 最小调不新增 collect 语义值，和这里的枚举加值不冲突——agent 产出 `collect_kind=script` 需要 enum 支持。
- **环境依赖**：脚本自身依赖（maas 的脚本要 Playwright+Chrome+SSO 登录态）是**项目侧责任**，buddy 只 shell-out 调、不管脚本内部。同事项目接入要保证脚本能独立跑（plan 标注）。

### 1.3 L6 聚合缝（点睛之笔）

读 recompute 链（`bw-store/src/sqlite.rs:994-1106`）发现：L4 阶段聚合只卷 `stage_kind=Some` 的；L6 项目聚合 = worst-of 各**阶段**聚合；**项目级指标（`stage_kind=NULL`）只更新自己那行 signal、不上卷 L6**。→ 北极星点亮了项目卡还是灰，挫败"看到价值"。

- **改 L6**：项目聚合把项目级 metric 也卷入 = `reduce_worst_of(阶段聚合 signals + 项目级业务指标 signals)`
- **小改一处**（`sqlite.rs:1098` 附近）。`reduce_worst_of` 有 Green 就不 Unknown → 北极星 Green 拉亮项目灯
- **诚实标注**：符合 buddy 原产品哲学（北极星驱动项目健康、"目标清晰且难造假"），但**代码当前没上卷是缝、不是原设计**。这是补缝让它符合哲学，不替它圆场说"原来就这样"。不破铁律（recompute 仍是唯一写入者，derive-only）。

### 1.4 UI 增量（不另造，一套体系渲染）

用户定：和 buddy 一个体系，只增量加显示。

- **ProgressAll 加「项目级业务指标」区段**（`app-desktop/src/screens/op.rs:1602` ProgressAll 段）：用现有 `MetricCard`（`op.rs:1834`）渲染层 A（北极星+L1/L2+L3）。`kernel.rs:970` 的 `filter(stage_kind==Some)` 保持阶段卡逻辑，项目级指标在新区段渲染（不再被全过滤）
- **北极星顶栏高亮**：保留 `op.rs:104` TopBar，同时新区段有一张卡（两处显）
- **层 B 单独「项目管理指标」小段**：现状数显示，不点灯，带来源徽（`ProjectCardVm` 加 `collect_kind`/来源徽记，`ui/src/vm.rs:33-51`）
- **总览墙移植 HealthOverviewCard**：op.rs:299-340 的跨阶段健康概览移植到 `wall.rs`（墙入口页当前无概览，已知 GAP）
- **SyncMetricsFile 加 UI 按钮**（运营视图）：当前只在 `MergeIssuePr` 后 auto-fire（`lib.rs:6411`），运营视图无入口。加按钮
- **UpdateWeekPlan 接 UI**：当前死命令（无 UI 触发，`lib.rs:193`）。week_plan 表「本周目标」列接活

### 1.5 skill 最小调（让 agent 初版跑对、可复制）

只改 2 份 SKILL.md prompt 文本，不重做 skill：

**找指标 `docs/skills/north-star-discovery/SKILL.md`**：
- **A1** Step1「读输入」（`:57-59`）加第4处：读项目仓既有指标体系（`governance/`、`derive_*.py`、项目自己的 `docs/metrics-rationale.md`）。**若存在，三层指标优先对齐映射、不另起炉灶自造**。rationale 如实记"已对齐项目既有 X 层"或"项目无既有，本轮首次推导"
- **A2** Step5 引领示例（`:86`）去 buddy 化：删"合并PR数/结算Issue数放这层合适"，改"优先复用项目既有过程指标；项目无才用 BW 自有记账，须标注'本条来自 BW 自有记账非项目既有'"
- **A4** Step6 定 collect（`:87-91`）加"项目侧自采脚本不降级 manual，`collect_kind=script`、`query` 写脚本路径+输出字段"

**绑数据 `docs/skills/metrics-binding/SKILL.md`**：
- **B1** connector 诊断行（`:47`）修正：项目侧脚本是自动采、不降级 manual，`query` 写脚本路径
- **B2** 诊断表前加一步：先扫项目仓 `governance/`/`derive_*.py` 既有采集脚本

**根因**（agent 已锚定）：不是 agent 跑错，是 skill Step1 不读项目既有体系 + Step5 喂 buddy 自有概念 + 绑数据 connector 分支主动降级 manual。改完 agent 初版跑 maas 会读 `derive_leading.py` 对齐 L1/L2/L3、北极星走 script 不误标 manual、不塞 phantom——和 §1.2/§1.3 对得上。

### 1.6 创建流 C/E 失败就停

- **缺口 C 空壳项目**：未配 workspace 根目录时 GitHub 新建/接入失败仍建项目行（`remote_path` 空 → 不建 trio/不挂 cron/不扫 skill），墙上多一个空壳
- **缺口 E auto-mint 悄悄本地开仓**：建仓失败兜底 `provision_workspace` 本地 mint 装接上（`lib.rs:3952-3984`/`4462-4489`），toast 一闪用户以为建好了
- **修法**：失败就停、如实报错、不兜底 mint 空壳（和"不假装健康"同精神）。小改 CreateProject 兜底分支
- **倾向做**（小改+诚实性），plan review 时可砍

---

## 2. 不在本窗

| 项 | 去向 |
|---|---|
| bug①冻死 / bug⑤发送框 / bug④worktree | `plan/17`（专门窗口） |
| bug②联网墙 | 竞品分析卡实践时 |
| 三件套造指标阶段优化（skill 重做） | 只最小调 prompt，不重做 |
| §4.1 创建流收窄 / §4.5 UpdateIssue / §4.8 归属反转 / 通用 connector 两层 / 规范 | 后续（领导说别急规范；两层归属本次标后续） |
| bw 本地 scan 采集器（资产5条 CN/DS/DT/EC/NK） | 后续（另一种采集模式单独趟） |

---

## 3. 改动清单 + 代码锚点 + 验收

| # | 件 | 文件:锚点 | 改什么 | 验收（读回为证） |
|---|---|---|---|---|
| 1 | skill 调 | `docs/skills/north-star-discovery/SKILL.md` + `docs/skills/metrics-binding/SKILL.md` | A1/A2/A4/B1/B2 改 prompt | 重跑找指标 agent → `.bw/metrics.toml` 有 `collect_kind=script`、对齐 maas、无 phantom/误标 manual |
| 2 | 手动改盘对齐 maas | `.bw/metrics.toml` + metric 表 | 用 maas 真实定义重写（北极星+L1/L2/L3，collect_kind=script）；SQL 或 SyncMetricsFile | `sqlite3 metric` 表行 role/collect_kind/collect_query 对齐；标"手动绕法非终态"进 PRACTICE |
| 3 | 通用脚本 connector | `bw-core/model.rs` CONNECTOR_KIND_* + `bw-engine/metrics_file.rs` CollectKind + `bw-store/lib.rs` + `bw-app/lib.rs` probe/collect arm | 加 `script` kind + probe(文件存在) + collect arm(shell-out 脚本→读 JSON→取字段→observation) | 建 script connector + 手动 `CollectMetrics` → `sqlite3 observation` 有 maas `data.json` 值 |
| 4 | L6 聚合改 | `bw-store/sqlite.rs:1098` 附近 | L6 卷入项目级 metric | 北极星 Green → `sqlite3 project.signal`=green（或 metric signal 推得） |
| 5 | UI 项目级业务指标区段 | `app-desktop/screens/op.rs:1602` ProgressAll + `kernel.rs` build_vm | 加区段渲染层 A MetricCard（不再被 stage_kind 过滤掉） | 深链 `BW_PANEL=progress` → stderr `[BW_OPEN]` + 截图见北极星/L1/L2/L3 卡 |
| 6 | 层 B 现状数小段 + 来源徽 | `op.rs` + `ui/vm.rs:33-51` ProjectCardVm | 层 B 单独小段，现状数不点灯带徽 | 截图见开放Issue/已合入MR 现状数 + 来源徽 |
| 7 | 总览墙移植 HealthOverviewCard | `app-desktop/screens/wall.rs` | 从 op.rs:299-340 移植跨阶段概览 | 深链 `BW_OPEN` 墙 → 截图见概览 |
| 8 | SyncMetricsFile UI 按钮 + UpdateWeekPlan 接 UI | `app-desktop/screens/op.rs` + settings | 加按钮 + 死命令接活 | 改 metrics.toml → 点按钮 → metric 表刷新读回 |
| 9 | 创建流 C/E 失败就停 | `bw-app/lib.rs:3952-3984`/`4462-4489` CreateProject 兜底 | 失败不兜底 mint、如实报 | 无 workspace 建项目 → 如实报错不建空壳（读回无 ghost project 行） |

---

## 4. 执行顺序 + commit 约定

worktree 隔离开发（见 §6）。顺序（依赖：可见→可采→点亮→上卷）：
1. skill 调（最快，纯 prompt）
2. 手动改盘对齐 maas（绕法定义，标非终态）
3. 通用脚本 connector（中改，加 kind+collect arm）
4. L6 聚合改（小改）
5. UI 项目级业务指标区段
6. 层 B 现状数小段 + 墙移植概览
7. SyncMetricsFile 按钮 + UpdateWeekPlan
8. 创建流 C/E

每件独立 commit，代号前缀（如 `18-①skill · …` / `18-③script-connector · …`），过门禁（`cargo fmt/clippy/wasm32 check/guard-kernel-ui-free/check app-desktop`），E2E 读回取证，写进 `PRACTICE-buddy.md` 对应步作「问题→判断→改了啥」。

---

## 5. 验收（E2E 读回为证，不截图代证）

- **通用脚本 connector**：建 connector + `CollectMetrics` → `sqlite3 observation` 有 maas `data.json` 值（L1/L2/L3/采纳率）
- **L6 上卷**：北极星 Green → `sqlite3 "SELECT signal FROM project WHERE name='maas-locate'"`=green
- **UI**：`BW_OPEN=maas-locate BW_PANEL=progress target/debug/builders-workbench` → stderr `[BW_OPEN]` + 截图见项目级业务指标区段
- **skill**：重跑找指标 agent → `.bw/metrics.toml` 读回 `collect_kind=script`、对齐 maas、无 phantom
- **创建流 C/E**：无 workspace 建项目 → 如实报错，`sqlite3 project` 无 ghost 行
- 门禁全过；行为靠 E2E 读回 + `/code-review`

---

## 6. 和 plan/17 并行 worktree

- **17 改**：`run_issue_now`（`lib.rs:3321-3407`）+ worktree-per-issue + `kernel.rs` 调度 + CancelRun
- **18 改**：connector/collect arm（`lib.rs` probe/collect 区 + `sqlite.rs`）+ L6（`sqlite.rs:1098`）+ `app-desktop/screens/op.rs`+`wall.rs`+`ui/vm.rs` + skill docs
- **交叉**：都碰 `lib.rs`，但**不同函数区**（17 在 run_issue_now/调度，18 在 probe/collect/CreateProject 兜底），冲突可控，merge 时按函数区解
- **worktree 交叉**：17 改了 `metrics_file` run 内读路径（worktree）vs `SyncMetricsFile`（主工作区）；18 的 SyncMetricsFile UI 按钮读主工作区，不冲突
- **可同时开两个 worktree 各自开发**，最后分别 merge
