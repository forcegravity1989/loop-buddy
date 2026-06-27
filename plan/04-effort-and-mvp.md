# 04 · 工作量估算 + MVP 切线

> 单人开发者视角的天数估算 —— 它直接决定 MVP 速度。
> 所有数字是**人·天**(low=顺利,high=现实摩擦)。日历换算见 §6。

---

## 0. 假设(读数前必看)

- **单人**,无团队。**中级 Rust**(trait/enum/Result/借用检查器在应用层得心应手)但**第一次用 Dioxus 0.7**(RSX / signals / `dx` 工具链)—— 学习坡**单列**,不摊进各组件。
- **~6 个有效专注小时/天**。low = 少踩坑;high = 真实摩擦(RSX 怪癖、组件边界的借用检查器拉锯、signal 重渲染调试、CSS-in-RSX 保真passes)。
- 栈按 [`02`](02-rust-stack-evaluation.md):Dioxus 0.7 桌面(wry);crate 布局按 [`03`](03-architecture-and-engine.md)。`bw-providers` / `app-web` 在 MVP 之外。
- 估算价的是**对原型 1619 条手调内联样式的「保真」复刻**,不是「差不多」。这是最大成本驱动,**不当免费**。内联 `style=` 字符串大多可逐条搬进 RSX,但每屏仍需对着原图做视觉 diff + 调校。
- 因为桌面用与浏览器**同款** WebView 引擎,CSS **渲染必然一致** —— 每屏风险是「布局/状态接线 + 调校」,不是「这颜色/阴影能不能出来」。这正是它不像 Slint/egui 那样爆炸的原因。
- **MVP = 阶段反转**(先纵切证脊椎),数据为 Manual 来源 + 本地落库;真 Executor / Connectors / Cron / Web 明确排除在下列总量外(触及处会标注)。

---

## 1. 头条数字

| 里程碑 | 范围 | 单人·天 | 日历(~5 天/周) |
|---|---|---|---|
| **M1 · 走通脊椎**(P0–P2) | workspace + 派生链 + 架构脊椎 + 一条纵切 UI | **66.5 – 122** | **≈ 3–6 个月** |
| **M2 · 保真 MVP**(P0–P3)= Tier A | 全部屏幕保真 + 本地持久化 + derive 健康 + MockExecutor | **≈ 100 – 190** | **≈ 5–9 个月** |
| 组件清单总量(含打包,不含 C/D/E) | 见 §2 rollup | **114 – 218.5** | — |

> **两套估算的吻合点**:「效率组件清单」的 `mvp` 标记项之和 = **66.5–122**,与「路线图」的 **P0+P1+P2** 之和**逐位相等** —— M1 这个数最硬。M2 与总量是两套视角的独立估算,量级一致、有 ±5–15% 噪声;**以组件清单 114–218.5 为总量真相源,阶段天数为规划视角**。

---

## 2. 组件级估算(分组 rollup)

| 分组 | low | high |
|---|---:|---:|
| 学习坡(首次 Dioxus) | 5 | 9 |
| Rust 内核:core / engine / store / app(架构 + 领域 + 持久化 + 总线) | 28 | 54 |
| ui:: selector + sparkline 数学(buildApp 移植 + 测试) | 8 | 15 |
| 设计系统 + 内联-CSS 保真税 + CJK 字体 | 12 | 24 |
| 跨切:3 轴导航状态机 + streaming 态 + 信号矛盾消解 | 6 | 13 |
| 项目墙 + 7 步向导(含 shell 共 8 屏) | 13.5 | 24.5 |
| 运营视图:chrome + 侧栏 + 11 个 panel-view | 22.5 | 42 |
| Chat 模式 + 可折叠 rail | 3.5 | 7 |
| 9 个 Hub 全屏库 | 6 | 11 |
| app-desktop 壳 | 2.5 | 5 |
| 打包/签名/公证(mac + win + CI) | 7 | 14 |
| **合计** | **114** | **218.5** |

**单项里值得拎出来的几条**(完整 40+ 项在 workflow 产出里):

- **bw-app Command/Event 总线 + ~25 用例处理器**:7–13 天。架构心脏、头号风险。
- **bw-core 度量派生链 + 消解 derive/persist 矛盾**:4–8 天。原计划最薄一环,是**设计活**(阈值模型、`5/7` 这种字符串比较语义、来源 gating),不只是写代码。
- **ui:: selector 移植 buildApp() ~530 行**:6–11 天。11 个 showXxx 条件 + 健康过滤 + commit timeline 映射,必配单测。
- **向导 Step 1 竞品洞察**(~127 行密集标记:sticky 方法栏 + GATE 卡 + 5 段流程条 + 6×6 竞品矩阵):2.5–4.5 天,向导里最重。
- **向导 Step 7 进度管理**(5 列可编辑 weekPlan 网格 + 派生 hit 徽章 + 三态信号选择器):2.5–4.5 天。
- **9 个 Hub**:6–11 天。广度非深度,9 个模式相近的列表/表格/网格,按批计价,保真税逐屏照付。
- **内联-CSS→RSX 保真税**(跨所有屏的视觉 diff/调校):6–12 天,**主导性隐性成本,单列以保总量诚实**。

---

## 3. 阶段拆解(MVP-优先排序)

> 排序原则:**在最小的面上、最早地、退掉最高的风险**。

### P0 · 基座:workspace + Dioxus 爬坡 + 核心领域 + 派生链设计 — **13.5–25 天**
**目标**:把两个最硬的**设计**决策钉进 bw-core 类型,屏幕依赖它之前。
- Cargo workspace(6 crate)+ Dioxus 锁 `0.7.9` + CI「内核禁依赖 UI」+ `cargo check --target wasm32`(Web 留口零成本保活)。
- Dioxus 学习坡:throwaway「hello signals」app 跑 RSX / signal / props / 事件 / `dx` 热重载 / `asset!()`(用官方 LLMs.txt 压缩爬坡)。
- bw-core 领域模型(非法状态不可表达 + serde)。
- **bw-core::derive 6 层链**;`Signal{Green,Amber,Red,Unknown}`;封口 `Derived<Signal>`(只能在 derive.rs 内构造 → **编译期保证健康永不被手设**);`parse_target(&str)` 覆盖源码 mini-DSL(`≥5` `≤24h` `<800` `100%` `7/7` `清零` `全覆盖` `↑` `跟踪`)+ amber 带。
- 消解 derive/persist 矛盾为 **derived-cached**:`SetWeeklySignal` 命令换成 `AnnotateWeeklyReview{human_override, reason}`;Manual 来源 gating(`手填` 徽章 + 过期→amber 上限)。

**出口**:`cargo test` 绿;`evaluate_metric()` 对源码**每一种**目标写法单测到位(数值比较符、bare 百分比/比率、4 个定性、Missing→Unknown、stale→Amber);**全 workspace 无法在 derive 外构造 Signal**(compile-fail 测试验证)。

### P1 · 架构脊椎:Command/Event + Executor(mock) + SQLite 切片 + selector — **22–42 天**
**目标**:在「无 UI」地面把头号风险证透 —— 可测、可持久、能发事件的核 + mock 引擎。
- bw-app:AppState + `dispatch(Command)` + `subscribe()->Stream<Event>` + `snapshot()`;~25 用例处理器(open_project、7 个向导步、set_panel/scope、send_session_message、promote_workflow、start_optimize、complete_wizard、back_to_projects、annotate_weekly_review)。
- bw-engine:Executor trait + `run_workflow` 阶段循环 + RunEvent 流 + **MockExecutor**(确定性假产出)。
- bw-store:sqlx SQLite,**仅切片表**(project / metric / op_stage / stage_metric / **observation〔新·append-only〕** / session / message / **weekly_review〔新〕**);signal 列改 NULLABLE 写穿缓存,**只由 `recompute_signals(project_id)` 写**;typed value/target 列。
- ui:: selector:移植 buildApp() ~530 行为纯函数(signal_color、phase_style、overview_attention「绿色隐身」过滤、project_overall_progress、wow_delta、version_commits 映射、artifact_gallery、11 个 showXxx 条件)+ `sparkline_path()` 数学,各配单测。

**出口**:一条 **headless 集成测试**无 UI 跑通全切片(CreateProject→7 步向导→CompleteWizard→RunWorkflow(mock) 发 PhaseStarted/Completed/Done→全落库);**杀进程重开,项目 + 派生信号 + 会话消息都还在**;`recompute_signals` 是唯一写 signal 列的代码路径(审计 + 测试)。

### P2 · 纵切 UI:设计系统 + 项目墙 + 一次完整向导 + 一个环节运营视图 — **31–55 天**
**目标**:把真 Dioxus 窗口架到已证脊椎上,端到端跑通**一条纵切**;验证最险的 **Event→Signal 桥**。
- 设计系统地基:全局 CSS(::selection / 滚动条 / `#EFEBE2`)、暖纸/clay token 为 Rust const、共享样式 helper、10 个内联 stroke-SVG 图标 —— 确立「逐条内联 vs helper const」的复用打法。
- CJK + mono 字体 `asset!()` bundle(替 CDN;mac 上 wry 验证 CJK 整形)。
- app-desktop 壳:窗口、挂 ViewModel、**Event 流→`Signal<ViewModel>` 响应式桥**(无泄漏/无过度渲染)、UI 事件→Command、3 轴导航状态机接线。
- 项目墙 + **完整 7 步向导全 8 屏**(向导是数据录入路径,纵切运营视图要 derive 的 Manual 值靠它种下)。
- **一个运营 panel-view:showProgStage**(运营 chrome + 7 环节轴 derive 信号点 + 5 tab 工具栏 + 一个侧栏变体 + KPI sparklines + owns/accept/control 卡 + 大趋势 sparkline + WoW)。
- streaming/loading 态重建为真 Dioxus loading(**不**照搬 dc-runtime 的 `sc-shine` @keyframes hack)。

**出口**:mac 上启动→项目墙→新建→走完 7 步录真值→落 showProgStage,**每个信号点/sparkline 都从录入值 derive**;退出重开数据还原;Manual 指标显示 `手填 · 未接入度量源` 徽章 + `as_of`,改值实时重 derive 重上色;首轮视觉 diff 通过;**Event↔Signal 桥在后台 MockExecutor 跑时无泄漏/无过度渲染(集成风险退役)**。

### P3 · 铺屏:其余 10 panel-view + 9 Hub + chat + rail + 全局保真调校 — **35–68 天**
**目标**:脊椎已证、打法已定,把原型剩余部分铺满 —— 广度非深度,逐屏复用 P2 模式。
- 10 个 panel-view(showProgAll / showWfLib / showWfStage / showWfDetail / showRoutAll / showRoutStage / showArtAll / showArtStage / showVerAll / showVerStage)。
- 4 个侧栏变体全套 + Chat 模式(气泡 + composer→send→落库→mock 回)+ 可折叠 rail(工作流目录树)。
- bw-store 其余表 + repo(workflow / routine_feed / 8 张 hub_* + sync 列)。
- 9 个 Hub 全屏(含 Settings toggle)。
- **内联-CSS→RSX 保真税**:对所有屏逐一视觉 diff/调校。

**出口**:11 个 panel-view 全可经导航状态机到达且 setState 副作用正确;9 Hub 保真;chat 经 bw-app+Mock 往返;逐屏 side-by-side 视觉 diff 通过;**mac 上保真桌面 MVP 功能完整**(Windows 打包在 Tier B)。

---

## 4. Tier 阶梯(MVP 之后,皆加法)

| Tier | 内容 | 增量·天 | 可独立交付 |
|---|---|---:|:---:|
| **A** | P0+P1+P2+P3 = 保真桌面 MVP(mac 可跑、未签名本地构建)—— **推荐切线** | 0 | ✓ |
| **B** | mac 签名/公证(Developer ID + Hardened Runtime + notarytool + stapler)+ Windows(.msi/.exe + Authenticode/HSM + WebView2 引导)+ CI 矩阵 | +7–14 | ✓ |
| **C** | 真 AI:MockExecutor 换 `AnthropicExecutor` / Claude Code 子进程。**同事团队负责**,经 `Executor` trait 接入;我们只交付 MockExecutor + 冻结契约 + 一致性测试 | (外部·~10–20) | ✓ |
| **D** | 真指标:Connectors(git/PR、CI、网关日志、telemetry)经 `Connector::pull()` 产 Observation;Cron 驱动 Routine cadence;过期由真窗口驱动;Manual 徽章在绑定真源后自动摘除。**L0→L6 链与类型不变,只换 L0 产出源** | +12–24 | ✓ |
| **E** | Web 端(**以后也许**):点亮 app-web(WASM)、wasm-opt/懒加载、SSR/瘦后端、CJK subset、provider 代理、IndexedDB adapter。内核 + Store trait + `updated_at/rev/SyncCursor` 已留口,**无需迁移 schema** | +12–26 | ✓ |

> **团队边界**:我们聚焦**创建 + 管理**;Tier C(真 AI)是**同事团队**经 `Executor` trait 的活,**不占我们预算** —— 我们交付 MockExecutor + 冻结的 trait 契约(P1)+ 一致性测试套件,他们的真实现照契约过测即可热插拔,双方零耦合并行。

**组合参考(我们的盘子)**:M2 保真桌面(Tier A)≈ **100–190 天**;可分发(A+B)≈ **108–204 天**;若 connector 也归我们(A+B+D)再 **+12–24 天**。真 AI(C)由同事并行,不串行进我们的关键路径。

---

## 5. Web 降级省下了什么(并入桌面唯一)

桌面唯一直接从 MVP 抹掉这些真实人天:WASM 包体工程(2–5)、SSR/hydration + Axum 瘦后端(3–6)、CJK 字体 subset + FOUT(1–3)、「浏览器不能直连 provider→瘦后端代理」整个可部署服务(4–8)、IndexedDB adapter(2–4)。**合计 ~12–26 天**,对单人 MVP 是实打实的提速。

**但留口**(架构已为此付过费,别删):bw-core/engine/app/ui 维持零-UI + WASM 可编译(CI 的 wasm32 check 免费保活);Store trait 维持可换实现;每表保留 `updated_at + rev/SyncCursor`。→ 未来 Web/同步零迁移成本。具体:删 app-web 出 MVP workspace 成员(或留非编译 stub),所有 Web 关切归入一个「以后也许」段。

---

## 6. 日历换算与现实

天数 ÷ ~5 个有效工作日/周。但单人**很少**每周满 5 个满产出日(杂务、生活、调试黑洞),保守按 4 折算更稳:

| | 单人·天 | @5 天/周 | @4 天/周(更现实) |
|---|---|---|---|
| M1 走通脊椎 | 66.5–122 | 13–24 周 | 17–30 周 |
| M2 保真 MVP | ~100–190 | 20–38 周 | 25–48 周 |

→ **M1 现实日历 ≈ 4–7 个月;M2 ≈ 6–11 个月**(单人)。若 Dioxus 上手顺、保真税轻,落低端;若学习坡深 + 反复调像素,逼近高端。

---

## 7. 最大的未知数(决定落低端还是高端)

1. **首次 Dioxus 学习曲线深度** —— **最影响全局**的单一变量。RSX + signals 可能比预算顺,也可能在「组件边界借用检查器 + signal 重渲染调试」上爆掉 high。(0.8-alpha 若中途逼升级是次级排期风险。)
2. **度量派生设计** —— 链只说了一半;`60%`/`5/7` 字符串目标让「比目标」非平凡;**设计**部分(非编码)真开放。
3. **1619 内联样式「保真」真实成本** —— 移植看着近乎照抄,但要真**匹配**手调间距/11 阴影/3–12px 圆角/webview 里 CJK 行度量的视觉 diff 次数,是最大沉默变量。
4. **buildApp() selector 正确性面** —— ~530 行派生;测试能 de-risk,但写「钉死原型精确行为(含文字读数)」的测试本身也不小,易低估。
5. **Event 流↔Dioxus Signal 桥** —— 最险集成点,首个 spike 前难估。
6. **打包尾巴** —— mac 公证 + Windows Authenticode-on-HSM + SmartScreen,首过拒签 + 证书硬件令牌物流,部分非工程、时间高度可变。
7. **CJK 字体策略** —— 3 family × 多字重体积大;是否需 subset、WKWebView 与 WebView2 上 CJK 整形/断行是否一致,要两平台实测才知。

---

## 8. 一句话

**走通脊椎(M1)≈ 67–122 天 / 4–7 个月**,是关键闸门 —— 架构、Dioxus 桥、度量派生在这里全部见真章。**保真 MVP(M2)≈ 100–190 天 / 6–11 个月**,就是我们的完整盘子。真 AI(Tier C)由**同事团队**经 `Executor` trait 接入,不在我们关键路径上 —— 我们焊死在 Mock + Manual,在 **P1 冻结 trait 契约 + 交一致性测试**让同事并行,把 M2(+ 打包 / 连接器)做扎实。
