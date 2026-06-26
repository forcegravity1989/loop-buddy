# 00 · Builders' Workbench → Rust 迁移总计划

> 把现有 HTML 原型,重写成 **Rust** 应用。**桌面优先(macOS + Windows),Web 次之**,一套代码尽量多端复用。
> 本计划依据唯一的两个文件:`Builders工作台-项目管理向导.dc.html`(3283 行)+ `support.js`(dc-runtime)。

## 附档索引
| 文档 | 内容 |
|---|---|
| [`01-prototype-inventory.md`](01-prototype-inventory.md) | **迁移规格书**:屏幕全清单 + 导航状态机 + 完整数据模型 + 设计系统 + 交互清单 |
| [`02-rust-stack-evaluation.md`](02-rust-stack-evaluation.md) | **技术栈选型**:Dioxus / Tauri / Slint / egui / Leptos / Yew 对比(2026-06 版本号),首选 + 次选 + 裁决 |
| [`03-architecture-and-engine.md`](03-architecture-and-engine.md) | **架构与引擎**:Cargo crate 布局、Rust 领域类型、状态管理、持久化、工作流引擎、分阶段路线 |

---

## 1. 我们在迁移什么(一句话)

**Builders' Workbench** —— 面向资深独立开发者 / 一人公司(OPC)的 AI 原生项目工作台。

现有原型(`Builders工作台-项目管理向导.dc.html`)是一个基于 React 的自定义模板 DSL(`dc-runtime` / `support.js`)写的复杂单页应用 —— 状态驱动、纯派生视图、命令式事件。它包含:
- **7 步新建项目向导**(竞品洞察 → 北极星 → 引领/滞后指标 → 原型 → 进度管理)
- **5 panel × 8 scope 运营矩阵**(进度/工作流/定时任务/产物/版本 × 全部/7环节)
- **9 个 Hub 全屏库**(技能/智能体/例程/定时/连接器/知识/活动/通知/设置)

迁移要把这套三段式保留:只是把宿主从「React + HTML DSL」换成「Rust」。

---

## 2. 两个关键决策

### 决策 A · 技术栈 = **Dioxus 0.7**(首选)
- **桌面**:Dioxus desktop = `wry` WebView(Mac=WKWebView / Win=WebView2),**浏览器级 HTML/CSS**,原型那内联 CSS 设计系统、暖纸 clay 色板、中文衬线混排、SVG sparkline **几乎可平移**。
- **Web**:同一套 RSX 编成 WASM/DOM,**一套代码出双端**。
- **前端也是 Rust**:RSX ≈ JSX,贴近原型现有的 React 心智,迁移成本最低。
- **次选**:Tauri v2(`2.11.3`)+ Leptos `0.8` 或 Yew 写前端(Rust→WASM)。**何时改选**:Dioxus 0.x API 演进或某平台打包卡死时,退回 Tauri+Leptos —— 仍是「Rust 前端 + WebView」,不退回 JS。
- **明确排除**:① 裸 Tauri(前端是 JS,违背「语言用 Rust」);② Slint / egui(非 HTML/CSS 的自绘 UI,迁移成本最高);③ Dioxus Native/Blitz 无-WebView 渲染器(2026 官方明说尚不建议投产)。

### 决策 B · 架构 = **UI 无关内核**
所有领域逻辑、状态、持久化、工作流引擎沉到**零 UI 依赖**的 crate;桌面/Web 各一层薄外壳,复用 80%+。两条不可妥协:
1. **UI 无关内核** —— 绝不 `use dioxus`/`use tauri`,只经「命令进、事件出」。
2. **度量内建、绝不编造** —— `Health` 永远 derive,指标值只能来自 `MetricSource`。

---

## 3. 产品面(屏幕地图)

三层正交导航:**Hub × View × Panel+Scope**。

```
全局图标栏(64px 竖栏,永远可见)
├─ workspace(工作台,hub='workspace')
│   ├─ view=projects   项目卡片墙(2列网格 + 新建虚线卡)
│   ├─ view=wizard     新建项目 7 步向导
│   │     step 0 引子 → step1 竞品洞察 → step2 竞品差距分析
│   │     → step3 北极星 → step4 引领指标 → step5 滞后指标
│   │     → step6 原型创建 → step7 进度管理 → view=app
│   └─ view=app        运营视图
│         环节轴:◎全部环节总览 | 01..07 七控制点
│         工具栏:进度 | 工作流 | 定时任务 | 产物 | 版本
│         左侧栏:任务历史 / 健康概览 / routine feed
│         中央区:12 panel×scope 组合(showXxx 标志矩阵)
│         右侧栏:工作流目录树 / 产物文件列表(可折叠)
│         Chat:workflow panel 下选中 session 时弹 Builder/Agent 对话
│
├─ SkillHub / AgentHub / Routines / CronHub  — 全屏库
├─ Connectors / Knowledge / Activity         — 全屏库
└─ Notifications / Settings                  — 通知 / 设置
```

七个控制点:**竞品洞察 · 需求导入 · 北极星指标 · 引领指标 · 滞后指标 · 原型创建 · 进度管理**。

完整交互详见 [`01` §3](01-prototype-inventory.md)。

---

## 4. 架构总览(crate 布局)

```
crates/
  bw-core/       领域内核:Project / OpStage×7 / Workflow / Metric 类型 + health 推导(零 IO/零 UI)
  bw-engine/     工作流引擎:WorkflowSpec→执行图 + append-only 事件流 + Executor trait (mock/真实)
  bw-providers/  真实执行:AnthropicExecutor / Claude Code 子进程 / Connectors / Cron / Hub(feature-gated)
  bw-store/      持久化:SQLite(sqlx)+ 迁移 + 会话/运行历史
  bw-app/        编排大脑:AppState + Command/Event 总线 + 用例(★桌面/Web 共享 80%)
  ui/            共享 ViewModel + selector(state→可渲染 DTO,对应原型 buildApp())
  app-desktop/   桌面薄壳【Dioxus 0.7 desktop / wry】
  app-web/       Web 薄壳【Dioxus 0.7 web / WASM】
```

详见 [`03`](03-architecture-and-engine.md)。

---

## 5. 设计系统(必须保真的 token)

| 类别 | 值 |
|---|---|
| 底色(暖纸) | `#EFEBE2` |
| 图标栏底色 | `#E9E3D7` |
| 主色(clay) | `#C5654A` |
| 三态信号 | green `#5F7355` · amber `#B5862F` · red `#B0503A` |
| 字体 | 标题 **Noto Serif SC** · 正文 **Noto Sans SC** · 数字 **JetBrains Mono**(`tabular-nums`) |
| 圆角 / 阴影 | 6–12px / `0 8px 26px rgba(35,33,28,.08)` |

**最高风险的两个保真点**(照搬数据会废,必须复刻规则):
1. **「绿色隐身、只有红黄出声」的过滤逻辑** —— 健康概览左栏只浮出「进行中」和「signal≠green」的项,归档记录沉到脚注;平铺全量数据就丢了产品灵魂。这是 `buildApp()` 里的业务规则,不在模板里。
2. **中文优先的衬线/无衬线/等宽三体混排** —— 数字非等宽,工程感立垮;Noto Serif SC 作为项目名/标题必须正确加载。

---

## 6. 分阶段路线图

| 阶段 | 目标 | 可验收产物 | 关键风险 |
|---|---|---|---|
| **Phase 0 · UI 壳 + mock** | 搭 workspace + 全 crate 骨架;`bw-core` 实体 + health 推导;`MockExecutor` 喂确定性假产出;Dioxus 桌面还原三视图(项目/向导/运营)+ 5panel×8scope 矩阵 + 9 个 Hub 全屏 | Mac+Win 双平台跑起来;所有屏幕导航可切;`cargo test` 覆盖 health/signal 状态机 | 原型视觉保真度(内联 CSS 迁到 RSX);SVG sparkline 数学复刻 |
| **Phase 1 · 本地持久化 + 真实领域** | `bw-store` SQLite + 迁移;命令-事件总线接库;项目/指标/会话/工作流 CRUD;7步向导真写库 | 关机重开数据还在;向导完成 → 运营视图真读库渲染;会话消息持久 | JSON 字段演化/迁移;`Manual` 指标来源 → UI 警示 |
| **Phase 2 · 真实执行 + Claude 接入** | `bw-providers::AnthropicExecutor`(或 Claude Code 子进程);会话 chat 真调 Claude;`dynamic→static` promote 真写库;routine cadence 调度 | 一条真 workflow 端到端:session 消息真回、promote 持久、routine feed 真观测 | API 成本与限流;Claude Code 子进程在 Windows 稳定性 |
| **Phase 3 · Connectors/Cron + Web 端** | Connectors(git/CI/日志/竞品)真喂指标;Cron 驱动 Routine 监测;`app-web`(WASM 复用 bw-app/ui)上线 | 真实 telemetry 驱动 health signal;Web 端复用 80% 跑同样所有屏幕 | WASM 下 providers 不能直连 → 需瘦后端;CJK 字体 subset 化 |

**最关键架构风险**:UI 栈过早耦合 —— 内核严守零 UI 依赖,CI 加「内核 crate 禁依赖 UI crate」约束检查。

---

## 7. 待你拍板的开放问题

1. **栈定案**:接受首选 **Dioxus 0.7**,还是要我先做一个「Dioxus vs Tauri+Leptos」的最小双端 spike(各还原一屏)再定?
2. **产品范围**:Rust 版的终点是 **(a) 保真 UI 重写(mock 数据,做到 Phase 1)**,还是 **(b) 真能跑的产品(接 Claude、跑真 workflow,做到 Phase 2/3)**?这决定投入量级。
3. **下一步动作**:要不要我直接落地 **Phase 0 的 workspace 骨架 + Dioxus 一屏 spike**?
