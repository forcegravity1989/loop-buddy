# 00 · Builders' Workbench → Rust 桌面应用 总计划

> 把现有 HTML 原型重写成 **原生桌面应用(Rust)**。
> **桌面唯一(macOS + Windows);Web 是「以后也许」,绝不驱动任何 MVP 决策。**
> 唯一依据:`Builders工作台-项目管理向导.dc.html`(3283 行)+ `support.js`(dc-runtime,1595 行)。

---

## 1. 为什么是 Rust(真实理由)

不是「为了用 Rust 而用 Rust」。目标是一个**比 Electron 更快、更轻的原生桌面应用** —— Rust 是手段,不是约束:

- **复用系统 WebView**(Mac=WKWebView / Win=WebView2),不像 Electron 每个应用各自打包整个 Chromium。因此在三件用户真能感知的事上赢:**冷启动 ~2–4×、空闲内存 ~2–3×、安装包 ~5–15×**(量级,非实测;详见 [`02 §1.5`](02-rust-stack-evaluation.md))。
- **不背 Chromium CVE 跑步机**:浏览器内核安全更新由 OS 厂商负责 —— 单人开发者最被低估的减负。
- **WebView = 真 CSS 引擎**:原型那 1619 条手调内联样式**近乎平移**,而非在自绘 UI 里逐像素重画。

> 一句话裁决:**WebView 已经拿走了「打败 Electron」90% 的收益。** Slint/egui 那种无-WebView 自绘只能再多挤 ~10% 性能,代价却是把整套 CSS 设计系统推倒重画 —— 对一个「设计感极强 + 文档式富 UI」的产品,这是最差的交易。故确认 **Dioxus 0.7 桌面(wry WebView)为首选**,Tauri+Leptos 为退路(桌面唯一后,退路反而更稳)。

---

## 2. 附档索引

| 文档 | 内容 |
|---|---|
| [`01-prototype-inventory.md`](01-prototype-inventory.md) | **迁移规格书**:屏幕全清单 + 导航状态机 + 完整数据模型 + 设计系统 + 交互清单 —— 已逐条核对原文,**准确,基本不动** |
| [`02-rust-stack-evaluation.md`](02-rust-stack-evaluation.md) | **技术栈选型**:Dioxus 0.7 确认首选 + **Electron 五维对比** + 桌面唯一后的裁决 + 退路触发条件 |
| [`03-architecture-and-engine.md`](03-architecture-and-engine.md) | **架构与引擎**:UI 无关内核 + Command/Event + Executor trait + **度量派生链 L0→L6(新增,补上原计划最薄的一环)** |
| [`04-effort-and-mvp.md`](04-effort-and-mvp.md) | **工作量估算 + MVP 切线(本次新增)**:单人天数、阶段、分层 tier、日历换算、最大未知数 |

---

## 3. 两个不可妥协(不变)

1. **UI 无关内核** —— 领域 / 状态 / 引擎 / 持久化绝不 `use dioxus` / `use tauri`,只经「命令进、事件出」。CI 加「内核 crate 禁依赖 UI crate」约束检查 + `cargo check --target wasm32`(为「以后也许的 Web」零成本保活)。
2. **度量内建、绝不编造** —— `Signal` 永远 derive,不可手设;值只能来自 `MetricSource`(含**显式** `Manual`)。这一条原计划只说了一半 —— [`03 §4.5`](03-architecture-and-engine.md) 现在把完整 **6 层派生链**补齐,并把 `Signal` 加上 `Unknown` 第四态(无数据 ≠ 绿)。

---

## 4. 产品面(屏幕地图)

三层正交导航:**Hub × View × Panel+Scope**。

```
全局图标栏(64px 竖栏,永远可见)
├─ workspace(工作台)
│   ├─ view=projects   项目卡片墙(2列网格 + 新建虚线卡)
│   ├─ view=wizard     新建项目 7 步向导(step0 引子 → step1..7 → view=app)
│   └─ view=app        运营视图
│         环节轴:◎全部环节总览 | 01..07 七控制点
│         工具栏:进度 | 工作流 | 定时任务 | 产物 | 版本
│         中央区:5 panel × 8 scope 网格,落地 11 个 showXxx 面板视图
│         左/右栏:健康概览 / 工作流目录树 / 产物文件;Chat 模式
├─ SkillHub / AgentHub / Routines / CronHub  — 全屏库
├─ Connectors / Knowledge / Activity         — 全屏库
└─ Notifications / Settings                  — 通知 / 设置
```

七个控制点:**竞品洞察 · 需求导入 · 北极星指标 · 引领指标 · 滞后指标 · 原型创建 · 进度管理**。
完整交互详见 [`01 §3`](01-prototype-inventory.md)。

---

## 5. 架构总览(crate 布局)

```
crates/
  bw-core/       领域内核:Project / OpStage×7 / Workflow / Metric 类型 + 度量派生链(零 IO/零 UI)
  bw-engine/     工作流引擎:WorkflowSpec→执行图 + append-only 事件流 + Executor trait(mock/真实)
  bw-store/      持久化:SQLite(sqlx)+ 迁移 + observation/会话/运行历史
  bw-app/        编排大脑:AppState + Command/Event 总线 + 用例(★桌面共享)
  ui/            共享 ViewModel + selector(state→可渲染 DTO,对应原型 buildApp())
  app-desktop/   桌面薄壳【Dioxus 0.7 desktop / wry】
  (app-web/      Web 薄壳【非 MVP 构建;保留为非编译 stub,trait 缝 + sync 列已留口】)
```

详见 [`03`](03-architecture-and-engine.md)。

---

## 6. 设计系统(必须保真的 token)

| 类别 | 值 |
|---|---|
| 底色(暖纸) | `#EFEBE2` · 图标栏 `#E9E3D7` |
| 主色(clay) | `#C5654A` |
| 三态信号 | green `#5F7355` · amber `#B5862F` · red `#B0503A`(+ 新增 `Unknown` 灰,表「无数据」) |
| 字体 | 标题 **Noto Serif SC** · 正文 **Noto Sans SC** · 数字 **JetBrains Mono** |
| 圆角 / 阴影 | 6–12px / `0 8px 26px rgba(35,33,28,.08)` |

> **校正**:原计划把 `tabular-nums` 列为设计 token,但原型源码里**根本不存在**该声明。JetBrains Mono 本身即等宽字体,数字对齐由字体保证 —— 若要显式加 `font-variant-numeric: tabular-nums` 那是**我们的新增**,不是原型既有项。
> **字体须本地 bundle**:原型走 Google Fonts CDN;原生桌面必须用 `asset!()` 打包 Noto Serif/Sans SC + JetBrains Mono(离线正确性 + 在 wry 上验证 CJK 整形)。

**最高风险的两个保真点**(照搬数据会废,必须复刻规则):
1. **「绿色隐身、只有红黄出声」的过滤逻辑** —— 健康概览只浮出「进行中」和「signal≠green」的项,归档沉到脚注。这是 `buildApp()` 业务规则,不在模板里。
2. **中文优先的衬线/无衬线/等宽三体混排** —— Noto Serif SC 作为项目名/标题必须正确加载。

---

## 7. 路线图(MVP 优先:先证脊椎,再铺屏)

**核心反转**:原计划 Phase 0 = 「先把所有屏幕用 mock 数据还原」,把最贵的交付放最前,却要等它做完才第一次压测架构(**头号风险**)。现在反过来 —— **先用一条最薄的纵切证明脊椎,再横向铺屏**。完整明细 + 组件级天数见 [`04`](04-effort-and-mvp.md)。

| 阶段 | 目标 | 单人·天 | 里程碑 |
|---|---|---|---|
| **P0 · 基座** workspace + Dioxus 爬坡 + 核心领域 + **派生链设计** | 把两个最硬的设计决策(度量派生链 + derive/persist 矛盾)钉进 bw-core 类型,屏幕依赖它**之前** | 13.5–25 | |
| **P1 · 架构脊椎** Command/Event + Executor(mock) + SQLite 切片 + selector | 头号风险在「无 UI」地面证透:headless 集成测试跑通 建项目→向导→workflow→落库→重开还在 | 22–42 | |
| **P2 · 纵切 UI** 设计系统 + 项目墙 + 完整向导 + 一个环节运营视图 | 首次把真 Command/Event/store/Mock 路径接到响应式 Dioxus UI,验证最险的 **Event→Signal 桥** | 31–55 | **M1 走通脊椎 ≈ 67–122 天** |
| **P3 · 铺屏** 其余 10 panel-view + 9 Hub + chat + rail + 保真调校 | 广度活,复用 P2 模式;单列「内联 CSS 保真税」 | 35–68 | **M2 保真 MVP ≈ 100–190 天** |

- **M1(P0–P2)= 走通脊椎**:内部里程碑。一个项目流端到端跑通、落库、信号全 derive。**这是关键闸门 —— Dioxus / Event→Signal 桥要疼,在这里就疼,不是第 6 个月才发现。**(两套独立估算在此数字上完全吻合:66.5–122 天。)
- **M2(P0–P3)= 保真 MVP**:可展示/可用的桌面应用,全部屏幕保真、本地持久化、Manual 值 + derive 健康、MockExecutor。

**叠加 tier**(每个都靠 MVP 已付清的 trait 缝,是加法不是重写):

| Tier | 内容 | 增量·天 | 归属 |
|---|---|---|---|
| **A** | = M2 保真桌面 MVP(**我们的主交付 / 推荐切线**) | (0) | 我们 |
| **B** | 签名/公证 + Windows 打包 + CI 矩阵 | +7–14 | 我们 |
| **C** | 真 Claude 执行(AnthropicExecutor / Claude Code 子进程) | (外部) | **同事团队**,经 `Executor` trait 接入 |
| **D** | Connectors + Cron 真喂指标(L0 产出源 Manual→Connector,链不变) | +12–24 | 我们(trait 我们的;真 connector 实现可再分) |
| **E** | Web 端(以后也许;内核 + Store trait + sync 列已留口) | +12–26 | 我们 |

> **团队边界**:我们聚焦**创建(向导)+ 管理(运营视图 / 度量 / 版本 / 产物)**;**真 AI 执行由同事团队负责**,经 `Executor` trait 接入 —— 这正是 UI 无关内核 + Executor trait 设计的兑现。我们交付 **MockExecutor + 冻结的 trait 契约 + 一致性测试套件**;同事的真实现照此契约编码、过同一套测试即可热插拔,双方零耦合并行。Tier C 不占我们预算。

**日历换算**(~5 个有效工作日/周,单人):**M1 ≈ 3–6 个月;M2 ≈ 5–9 个月**。范围本身就是不确定性 —— 决定落在低端还是高端的最大单一变量是**第一次用 Dioxus 的学习曲线**。

---

## 8. 最关键的五个风险(详见 [`04`](04-effort-and-mvp.md))

1. **首次 Dioxus 学习曲线** —— 最影响整体落低端还是高端。对策:P0 用 throwaway app 把坡爬完,P2 在最小纵切上验证,再铺屏;规划按高端(9 天)算。
2. **Dioxus 0.7→0.8 破坏性变更**(0.8.0-alpha 已在路上)—— Cargo.toml 锁 `0.7.9`;视图只在 app-desktop,炸不到内核;退路 Tauri+Leptos 保温。
3. **度量派生设计扩张** —— 字符串目标(`60%`/`5/7`/`↑`/`清零`)+ 边界(99.9% 可用率不能用相对 10% amber 带)。对策:P0 当设计活做透,amber 带建模为 `enum{RelPct,AbsPoints}`,`Signal` 加 `Unknown`,目标 mini-DSL parser 对源码每一种写法做单测。
4. **Event→Signal 桥** —— 最险的集成点。对策:P1 先 headless 证后端,P2 设「无泄漏 / 无过度渲染」为出口标准。
5. **1619 内联样式的「保真税」** —— 主导性隐性成本。对策:WebView 保证渲染一致(同引擎),风险是誊写非能力;sparkline 数学进 ui::selector 纯函数单测;P3 单列保真税。

---

## 9. 待你拍板

> 范围已定:**创建 + 管理是我们的**;真 Claude 执行由同事团队经 `Executor` trait 接入(见 §7)。这让第 1、2 题更清晰。

1. **MVP 切线**:既然真 AI 不是我们的活,我们的完整产品面**就是 M2 保真管理工作台**(全跑在 MockExecutor 上,同事的真实现经 trait 热插拔)。确认以 **M2 为目标**?(M1 是其中的关键内部闸门,非终点。)
2. **trait 契约何时冻结**:`Executor`(+ `PhaseNode`/`PhaseOutput`/`RunEvent`)是**跨团队接口**,应在 **P1** 定稿并交付一致性测试套件,好让同事并行开发。要不要我把「敲定 Executor 契约 + 测试套件」列为 P1 显式交付物,并先起草契约草案?
3. **下一步动作**:要不要我直接落地 **P0** —— workspace 骨架 + Dioxus throwaway 爬坡 + bw-core 领域模型 + 派生链类型(含 `Signal{+Unknown}` 与封口 `Derived<T>`)?
