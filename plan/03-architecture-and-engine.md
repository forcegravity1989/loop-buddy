# 03 · Rust 应用架构与工作流引擎设计

> 目标:把 `Builders工作台-项目管理向导.dc.html` + `support.js` 重写成桌面优先(Mac+Win)、Web 次之的 Rust 应用。核心是把**领域逻辑 / 状态 / 持久化 / 工作流引擎**全部沉到与 UI 无关的 crate,让桌面与 Web 复用 80%+ 代码;UI 栈由 [`02-rust-stack-evaluation.md`](02-rust-stack-evaluation.md) 选型,这里只留薄适配层。

---

## 0. 设计总纲(两条不可妥协)

1. **UI 无关内核**:所有领域类型、状态、引擎、持久化只依赖 `serde` / `tokio` / `sqlx` 等通用库,**绝不** `use dioxus` / `use tauri`。UI 通过「命令进、事件出」两个端口接入。
2. **度量内建、绝不编造**:`Metric` 的值只能来自 `MetricSource`(connector/CI/日志);`Health` 永远是 `derive(...)` 出来的,不可直接 set。

---

## 1. Cargo workspace / crate 布局

**一句话**:一个 `bw-core` 领域内核 + `bw-engine` 工作流引擎 + `bw-store` 持久化 + `bw-app` 应用编排,四者零 UI 依赖;桌面/Web 各自只是一层外壳 + 共享 `ui` 视图模型。

```
builders-workbench/                 (Cargo workspace root)
├── Cargo.toml                      [workspace] members + 统一依赖版本
└── crates/
    ├── bw-core/        ← 领域内核:实体 struct/enum、不变量、health 推导
    │                     依赖:serde, time, thiserror, uuid。无 async、无 IO。
    ├── bw-engine/      ← 工作流执行引擎:WorkflowSpec→执行图、调度、事件流、
    │                     Executor trait(mock/真实)
    │                     依赖:bw-core, tokio, async-trait, futures。
    ├── bw-providers/   ← 引擎的「真实执行」实现:Anthropic API / Claude Code 子进程、
    │                     Connectors、Cron、Skills/Agents Hub 装载器。全部实现内核的 trait。
    │                     依赖:bw-engine, reqwest, tokio。feature-gated。
    ├── bw-store/       ← 持久化:SQLite(sqlx) repo、迁移、会话/运行历史
    │                     依赖:bw-core, sqlx(sqlite), serde_json。
    ├── bw-app/         ← 应用编排层(UI 无关的「大脑」):AppState、Command/Event 总线、
    │                     用例(open_project / run_workflow / promote_workflow / send_message)
    │                     把 core+engine+store+providers 接成一个可订阅的状态机。
    │                     依赖:以上全部 + tokio。★这是桌面/Web 共享的 80%。
    ├── ui/             ← 共享「视图模型」(ViewModel)+ 纯函数 selector
    │                     把 AppState 投影成 UI 友好的 DTO(颜色/标签/进度条已算好,
    │                     对照原型 buildApp() / buildHubs())。依赖:bw-core, serde。
    ├── app-desktop/    ← 桌面外壳【薄 · 选型 = Dioxus 0.7 desktop(wry WebView)】
    │                     只做:窗口、把 ui::ViewModel 渲染出来、把用户操作转成 Command 投给 bw-app。
    └── app-web/        ← Web 外壳【薄 · 选型 = Dioxus 0.7 web(WASM/DOM)】
                          复用 ui + bw-app(WASM 下 bw-store 换 IndexedDB/远端,bw-providers 走后端代理)。
```

**80%+ 复用**

| 层 | 桌面 | Web | 复用 |
|---|---|---|---|
| `bw-core / bw-engine / bw-app / ui` | 同一份 | 同一份(可编 WASM) | **100%** |
| `bw-store` | SQLite 本地文件 | WASM:换 IndexedDB adapter 或走后端 | trait 同,impl 二选一 |
| `bw-providers` | 直连 Anthropic / 起 Claude Code 子进程 | 浏览器不能直连 → 走瘦后端代理 | trait 同 |
| 外壳 | `app-desktop` | `app-web` | 各 ~10%,只接线 |

---

## 2. Rust 领域模型(地道类型草图)

放在 `bw-core`。原则:**非法状态不可表达**。

```rust
// ---------- 标识 ----------
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProjectId(Uuid);
// 同法:WorkflowId / SessionId / MetricId / RoutineId

// ---------- 信号(原型只有三态;新增 Unknown 表「无数据」,绝不让缺数据默认成绿) ----------
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Signal { Green, Amber, Red, Unknown }
// ★ Signal 不可在 bw-core::derive 之外构造:用封口 newtype Derived<Signal>(私有字段 + mod sealed),
//   唯一构造者是 L2 evaluate_metric() 与 L4/L6 reduce_worst_of()。详见 §2.5。

// ---------- 指标分型(原型 leading[] / lagging[]) ----------
#[derive(Clone, Serialize, Deserialize)]
pub struct MetricSource { pub kind: SourceKind, pub note: String }
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum SourceKind { GatewayLog, Ci, GitPr, Telemetry, Connector, Manual /* UI 显式警示 */ }

#[derive(Clone, Serialize, Deserialize)]
pub struct LeadingMetric {   // 原型 state.leading[]: name/def/cur/target/source/ok/lastTarget/hit/driver
    pub name: String,
    pub def: String,
    pub current: String,     // 当前值(已格式化)
    pub target: String,
    pub source: MetricSource, // ★ 没有 source 就建不出来 → 绝不编造
    pub last_target: String,
    pub hit: bool,
    pub driver: String,      // 本周抓手(原型 weekPlan 可编辑)
}
#[derive(Clone, Serialize, Deserialize)]
pub struct LaggingMetric {   // 原型 state.lagging[]: name/def/cur/target
    pub name: String,
    pub def: String,
    pub current: String,
    pub target: String,
}

// ---------- OpStage(原型 opStages×7,7 个控制点) ----------
#[derive(Clone, Serialize, Deserialize)]
pub struct OpStage {
    pub kind: StageKind,             // 见 enum
    pub phase: StagePhase,           // '已定稿'|'迭代中'|'监测中'|'持续运行'
    pub progress: u8,                // 0..100
    pub trend: Vec<f32>,             // 近6周进度值(sparkline)
    pub metrics: Vec<StageMetric>,   // 各项 KPI
    pub routine: Routine,            // 定时观测
    pub method: Option<StageMethod>, // principle/logic/lead/lag/funnel(仅部分环节有)
    pub owns: String,                // 该环节「我负责什么」
    pub accept: String,              // 验收信号描述
    pub control: String,             // 控制点说明
    pub create: Vec<Session>,
    pub optimize: Vec<Session>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum StageKind {
    CompetitorInsight, RequirementIntake, NorthStar,
    Leading, Lagging, PrototypeCreate, ProgressMgmt
}
#[derive(Clone, Serialize, Deserialize)]
pub enum StagePhase { Finalized, Iterating, Monitoring, Running }

// ---------- 定时观测(原型 opStage.routine) ----------
#[derive(Clone, Serialize, Deserialize)]
pub struct Routine {
    pub schedule: Cadence,         // '每日' | '每周' | '实时' | Cron(String)
    pub signal: Signal,
    pub watches: Vec<String>,      // 监测项名称
    pub feed: Vec<FeedItem>,       // append-only 观测记录
}
#[derive(Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub time_label: String,        // '今日' '本周' '2min前'
    pub level: FeedLevel,
    pub text: String,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum FeedLevel { Info, Warn, Err }
#[derive(Clone, Serialize, Deserialize)]
pub enum Cadence { RealTime, Daily, Weekly, Cron(String) }

// ---------- Session(原型 create[]/optimize[] 任务会话) ----------
#[derive(Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub snippet: String,
    pub status: SessionStatus,
    pub msgs: Vec<Message>,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum SessionStatus { Active, Archived, Done }
#[derive(Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,   // Builder | Agent
    pub text: String,
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Role { Builder, Agent }

// ---------- Workflow(原型 static/dynamic) ----------
#[derive(Clone, Serialize, Deserialize)]
pub enum WorkflowKind {
    Static { maturity: Maturity, version: u32, uses: u32, scope: String },
    Dynamic { origin: String, stage: String },
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Maturity { Mature, Polishing, Fresh }

#[derive(Clone, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub id: WorkflowId,
    pub name: String,
    pub kind: WorkflowKind,
    pub prompt: String,              // 原始 prompt / query
    pub goal: String,
    pub stage_ref: Option<u8>,       // 关联的控制点 n=1..7
    pub phases: Vec<String>,         // 阶段名称列表
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub loop_config: LoopConfig,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct LoopConfig { pub retries: u8, pub max_iter: u8 }
#[derive(Clone, Serialize, Deserialize)]
pub struct AgentRef { pub name: String, pub def: String, pub from: String }
#[derive(Clone, Serialize, Deserialize)]
pub struct SkillRef { pub name: String, pub def: String, pub from: String }

// ---------- Project(原型 state.projects[]) ----------
#[derive(Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,                // '看板/网页应用' | '对话应用' ...
    pub desc: String,
    pub phase: ProjectPhase,
    pub signal: Signal,
    pub progress: u8,
    pub stages: Vec<OpStage>,        // 7 条(运营中时)
    pub leading: Vec<LeadingMetric>,
    pub lagging: Vec<LaggingMetric>,
    pub north_star: String,
    pub ns_def: String,
    pub weekly_signal: Signal,
    pub cold_step: Option<u8>,       // 冷启动时当前向导步骤
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum ProjectPhase { Running, ColdStart }

impl Project {
    /// signal 永远 derive,不可直接 set —— 与 L4/L6 同款 worst-of 归约(含 Unknown 档)
    pub fn derive_signal(&self) -> Signal {
        // 任一 Red→Red;否则任一 Amber→Amber;
        // 否则(有 Unknown 且零 Green)→Unknown;否则→Green。详见 §2.5。
        reduce_worst_of(self.stages.iter().map(|s| s.routine.signal))
    }
}

// ---------- Hub(原型 state.hubs[]) ----------
#[derive(Clone, Serialize, Deserialize)]
pub struct HubCard {
    pub id: HubKind,
    pub name: String,
    pub count: u32,
    pub color: String,
    pub desc: String,
    pub items: Vec<String>,   // 示例项名称
}
#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum HubKind { Workflow, Skill, Agent }
```

---

## 2.5 度量派生链(metric → signal → health)

> 这是原计划**最薄的一环**。原型里每个 `signal`/`hit`/`weeklySignal` 都是**手写字面量**(HTML 2287/2228/3272-3274),`sigColor` 只把已定信号映射成颜色 —— 即「值→信号」根本没建。下面建全它,并把「signal 永远 derive」从口号变成**编译期保证**。

### 六层链(自底向上)

| 层 | 输入 | 规则 | 存/派生 |
|---|---|---|---|
| **L0 Observation** 原始事件 | `MetricSource`(SourceKind: GatewayLog/Ci/GitPr/Telemetry/Connector/**Manual**) | 纯摄入,append-only,永不改。Connector 失败**不写**观测(沉默≠绿,下游可检测为 stale)。**值唯一的诞生地** —— 没有 Observation 就建不出值(Manual 是显式来源,不是「无来源」) | stored-source |
| **L1 MeasuredValue** 归一标量 | 最新 Observation + `MetricShape`(Percent/Count/Ratio/DurationMs/RawNumber) | 确定性解析:出 `f64 + unit + as_of + SourceKind`。`5/7`→0.71 且留 (5,7) 供显示;`842ms`→ms。最新观测比 cadence 窗口旧 → `stale=true`;无观测 → `Missing` | derived-cached(键=metric_id,新观测即失效) |
| **L2 值-比-目标**(★缺失层 / 产品核心) | MeasuredValue + 解析后 `Target`(comparator∈{≥≤<>=}+阈值+amber 带,或定性 {清零/全覆盖/↑/跟踪}) | 确定性阈值函数(见下)→ `MetricEvaluation{signal,hit,distance}`。`hit=(signal==Green)`。Missing/不可解析定性 → **Unknown**(灰,**绝不绿**)。Manual 照常 derive,但带 `provenance=Manual` | derived(纯函数) |
| **L3 StageMetric.signal** 单 KPI | L2 的 MetricEvaluation | 透传。今天持久化的 `stage_metric.signal` → 改为 `Derived<Signal>` **写穿缓存**。失效:新观测 / 改 target / 改 MetricShape | derived-cached |
| **L4 Routine.signal** 环节级 | 该 Routine watches 的所有 StageMetric.signal | **worst-of**:任一 Red→Red;否则任一 Amber→Amber;否则(有 Unknown 且零 Green)→Unknown(没绿指标+缺数据≠健康);否则→Green | derived-cached |
| **L5 OpStage health** 侧栏/环节轴 | Routine.signal(L4) + phase(正交元数据) | `health()=routine.signal` 直给点色(对原型 2671/2720)。phase 只驱动徽章色,**不是** health。overview-attention:health≠Green 或有进行中 session 才浮出 | derived(selector 纯投影) |
| **L6 Project.signal + 周健康** 项目卡 | 7 个 OpStage 的 routine.signal | 与 L4 同款 worst-of(含 Unknown)= 现有 `derive_signal()` 扩 Unknown。`weekly_signal` = 周五边界对 derive 的快照,落 `weekly_review`;用户那一下点击 → 可审计 **override 标注**(记原因,绝不静默盖红),非真相源 | derived-cached |

### 阈值模型(L2 细则)

- **解析**:`≥/>/≤/</=` 前缀数值 → `Target::Threshold{cmp,value,unit}`;≥/> = HigherIsBetter,≤/< = LowerIsBetter,= = Exact。裸 `100%`/`7/7` → 隐式下限(≥)。定性:`清零`→DriveToZero、`全覆盖`→FullCoverage、`↑`→DirectionUp(无固定阈值,Green iff WoW 末>前,否则 Amber,**单凭方向永不 Red**)、`跟踪`→TrackOnly ⇒ **Unknown**。
- **判定**(HigherIsBetter,目标 T,amber 带 β 默认 0.10·T 可每指标覆盖):`value≥T`→Green;`T·(1−β)≤value<T`→Amber;`value<T·(1−β)`→Red。LowerIsBetter 镜像;Exact 同值→Green、±容差→Amber。**β 带是每指标存储配置(默认 10%),诚实可调,非魔法。**
- **退化**:`Missing`→Unknown(灰,标「无数据」);`stale=true`→**signal 封顶 Amber** + 标「数据过期」(新鲜的绿不许盖死掉的源);不可解析 target→Unknown + 编辑器 lint(不静默)。
- **∴ `Signal` 必须加第四态 `Unknown`** —— 三态无法诚实表达「无数据」,而那正是「绝不编造」的要害。

### derive/persist 矛盾的消解 = derived-cached(非 stored-source)

signal 概念上永远是 (MeasuredValue, Target) 的纯函数,但每帧重算整条 L0→L6 浪费,且 signal 列对廉价读/排序/历史有用。故 **signal 列降级为写穿缓存**,只由 `bw-store::recompute_signals(project_id)` 这唯一路径写,**没有任何 setter**。

- **失效触发**:① 新 Observation → 该 StageMetric(L2/L3)→ Routine(L4)→ Project(L6);② 改 target/amber_band;③ 改 MetricShape;④ Manual `UpsertManualValue`;⑤ 周复盘边界(仅快照)。
- **类型层强制**:缓存列 NULLABLE + `derived_rev/derived_at`;NULL 或 rev 过期 → 读时**惰性重算**,绝不把缓存当权威。
- **封口 `Derived<Signal>`**:阈值/归约函数收进 `bw-core::derive`,`Signal` 经 `Derived<T>`(私有字段 + `mod sealed`)暴露,**唯一构造者**是 `evaluate_metric()` 与 `reduce_worst_of()`。外部能 `.get()` 读,**不能** `Derived::new(Signal::Green)`。→「健康永远 derive、绝不手设」成**编译期保证**。
- **删 `SetWeeklySignal`**,换 `AnnotateWeeklyReview{human_override:Option<Signal>, reason:String}`;override 单列存,UI 必须 derived 与 override **并显**;MVP 里 override 只能更悲观,更乐观必须带原因。

### MVP 怎么落(无 Connector)

首个桌面构建**没有 Connector**。最简诚实派生:① 向导(步 4/5/7)与环节面板里,用户把当前值录成 **Manual 来源 Observation**(`SourceKind::Manual`)—— 真数据,只是手填;② 信号**仍**由 L2 全程 derive,**应用从不问用户选绿黄红,绝不存引擎没算的信号**;③ 每个 Manual 指标常驻 `手填 · 未接入度量源` + `as_of`,过期可见衰减(stale→amber 封顶);④ `weekly_signal` = derive 快照,人工 override 记原因。

→ **诚实的那条线**:标注的是**值的来源**(Manual),而**健康本身仍 derive**,从不让用户编造健康。L0→L6 链与类型在接 Connector 时**一字不改** —— 只把 L0 产出者从「Manual 命令」换成 `Connector::pull()`(Tier D)。这正是纵切先证脊椎要的:**派生缝先在 Manual 数据上证透**。

### 开放设计问题(P0 拍板)

- amber 带:扁平 10% 对 `99.9%` 可用率是错的(会让 ≥89.9% 都绿)→ 需 `amber_band: enum{RelPct(f32), AbsPoints(f32)}` 每指标。
- `↑` 方向目标能否永不 Red?走平 N 周该不该升级 Amber→Red?
- `Routine.watches`→StageMetric 用名字串还是 metric_id FK?建议 FK(改名安全)。
- 一窗口多观测聚合(latest/mean/min/max/p95)?延迟用 p95、可用率用 min、计数用 latest → 需每指标 `aggregation` 列(MVP 预留,Tier D 实现)。

---

## 3. 状态管理(与 UI 栈解耦)

`bw-app` 内核用**命令-事件 + 单一可订阅状态**模型,与任何 UI 栈都能接:

```rust
pub enum Command {                         // UI → 内核(意图)
    OpenProject(ProjectId),
    CreateProject { name: String, kind: String },
    SetWizardStep { project: ProjectId, step: u8 },
    UpdateNorthStar { project: ProjectId, value: String, def: String },
    UpdateLeadingTarget { project: ProjectId, metric_idx: usize, target: String, driver: String },
    SetWeeklySignal { project: ProjectId, signal: Signal },
    CompleteWizard(ProjectId),             // step7 完成 → app 视图
    RunWorkflow { project: ProjectId, workflow: WorkflowId },
    SendSessionMessage { session: SessionId, text: String },
    PromoteWorkflowToStatic(WorkflowId),   // dynamic→static 沉淀
    StartOptimizeSession { stage_id: (ProjectId, u8), title: String },
    SetPanel { panel: Panel },
    SetScope { scope: Scope },
    SetHub(HubId),
    BackToProjects,
}

pub enum Event {                           // 内核 → UI(事实,已发生)
    ProjectUpdated(ProjectId),
    SessionMessageAdded { session: SessionId, message: Message },
    WorkflowPromoted { workflow: WorkflowId },
    WorkflowRunProgress { workflow: WorkflowId, phase_idx: usize, status: String },
    RoutineFeedAppended { project: ProjectId, stage_n: u8, item: FeedItem },
    HubDataRefreshed,
}

pub struct AppState {
    pub view: ViewMode,
    pub hub: HubId,
    pub panel: Panel,
    pub scope: Scope,
    pub projects: Vec<Project>,
    pub wizard_step: u8,
    pub active_project: Option<ProjectId>,
    pub active_session: Option<SessionId>,
    pub wf_detail_id: Option<WorkflowId>,
    pub workflows: Vec<WorkflowSpec>,
    pub hub_cards: Vec<HubCard>,
    pub rail_open: bool,
    pub view_mode: ContentMode,
    pub composer_text: String,
}

impl App {
    pub async fn dispatch(&mut self, cmd: Command) -> Result<()>;
    pub fn subscribe(&self) -> impl Stream<Item = Event>;
    pub fn snapshot(&self) -> &AppState;
}
```

**不同 UI 栈如何接**:
- **Dioxus(signals)**:把 `Event` 流 push 进 `Signal<ViewModel>`,组件细粒度订阅。
- **Leptos**:同样 signal 驱动;`ui::selector(state) -> ViewModel` 复用。
- **Tauri IPC**:`Command` = `#[tauri::command]`,`Event` = `app.emit()`;前端消费 ViewModel JSON。

三种接法都只写在 `app-desktop/app-web`,内核不感知。

---

## 4. 视图模型 selector(对照原型 buildApp)

`ui::selector(state) -> ViewModel` 是**纯函数**,把 `AppState` 投影成 UI 可直接消费的 DTO。对应原型 `buildApp()` 里的 10 个关键派生规则:

| 原型规则 | Rust selector |
|---|---|
| `sigColor(s)` → 颜色 hex | `signal_color(s: Signal) -> &'static str` |
| `phaseStyle(p)` → bg/color | `phase_style(p: StagePhase) -> (bg, color)` |
| 健康概览左栏过滤 | `overview_attention(state) -> (Vec<AttentionItem>, String)` — 只露出「进行中」session + signal≠green 的环节 |
| `opOverall` 总进度 | `project_overall_progress(stages: &[OpStage]) -> u8` — 7条平均 |
| sparkline SVG | `sparkline_path(trend: &[f32], w, h) -> SvgPath` |
| WoW 涨跌 | `wow_delta(trend: &[f32]) -> WowDir` — 最后两点差 |
| 工作流目录树 | `workflow_tree(wf: &WorkflowSpec, sess: &Session) -> Vec<TreeRow>` |
| commit timeline | `version_commits(stages: &[OpStage]) -> Vec<Commit>` — create=feat·已合并,optimize=fix·PR待验收 |
| 产物画廊 | `artifact_gallery(stages: &[OpStage]) -> Vec<GalleryItem>` |
| HubCard items | `hub_items_preview(card: &HubCard) -> Vec<String>` |

**selector 输出的 ViewModel 结构** 对应原型 `renderVals()` 返回对象的全部字段,UI 只消费 ViewModel,不直接读 AppState。

---

## 5. 持久化(本地优先)

`bw-store`,**SQLite via `sqlx`**(编译期校验 SQL + 异步;桌面单文件 `workbench.db`)。
**所有 `signal`/`hit` 列均为写穿缓存**(NULLABLE,只由 `recompute_signals` 写,带 `derived_rev/derived_at`);指标值的真相源是 append-only 的 `observation` 表(§2.5)。

```sql
-- 项目
project(id, name, kind, desc, phase, cold_step, north_star, ns_def,
        weekly_signal TEXT NULL,           -- 派生缓存;权威 = weekly_review 周快照
        signal_derived_rev INT NULL, signal_derived_at TIMESTAMP NULL,
        created_at, updated_at)

-- 指标(hit 是派生缓存:NULLABLE,只由 recompute 写;值的真相在 observation)
metric(id, project_id FK, role TEXT['leading'|'lagging'], name, def,
       current_val, current_source_kind, current_as_of, current_stale BOOL DEFAULT 0,
       target_cmp, target_value REAL NULL, target_unit,
       target_amber_band REAL DEFAULT 0.10, target_qual,
       hit BOOL NULL, signal_derived_rev INT NULL,
       last_target, driver, pos INT)  -- pos 保序

-- 控制点环节(无持久 health 列;health = routine.signal 的纯投影 L5)
op_stage(id, project_id FK, n INT, phase, progress, trend JSON,
         method JSON, owns, accept, control)
stage_metric(id, stage_id FK, name, trend JSON,
             value_magnitude REAL NULL, value_unit, value_ratio_num INT NULL, value_ratio_den INT NULL,
             value_as_of, value_source_kind NOT NULL, value_stale BOOL DEFAULT 0,
             target_kind['threshold'|'qualitative'], target_cmp, target_value REAL NULL,
             target_unit, target_amber_band REAL DEFAULT 0.10, target_qual,
             display_val, display_target,                    -- 保 UI 原样,免重解析
             signal TEXT NULL CHECK(signal IN('green','amber','red','unknown') OR signal IS NULL),
             signal_derived_rev INT NULL, signal_derived_at TIMESTAMP NULL)  -- signal = 写穿缓存

-- 指标值的唯一来源(append-only,enforce「度量内建」;Manual 也落这,source_kind='manual')
observation(id, metric_id FK, ts TIMESTAMP, source_kind TEXT NOT NULL, raw JSON,
            source_run_id TEXT NULL, created_at)   -- INDEX(metric_id, ts DESC)

-- 周复盘快照 + 人工 override(替代原型手设 weeklySignal;override 与 derived 分列,并显可审计)
weekly_review(id, project_id FK, week_of DATE, derived_signal TEXT NOT NULL,
              human_override TEXT NULL, override_reason TEXT NULL, created_at)

-- 工作流
workflow(id, project_id FK NULL, kind, name, prompt, goal,
         stage_ref INT, phases JSON, agents JSON, skills JSON, loop_config JSON,
         maturity, version INT, uses INT, scope, origin)

-- 会话
session(id, stage_id FK, kind TEXT['create'|'optimize'], title, snippet, status,
        created_at)
message(id, session_id FK, seq INT, role TEXT['b'|'a'], text, created_at)

-- 定时任务观测记录
routine_feed(id, stage_id FK, seq INT, time_label, level TEXT, text, created_at)

-- Hub 数据(快照,periodically refresh)
hub_skill(id, name, desc, category, source, maturity, uses INT)
hub_agent(id, name, role, skills JSON, model, runs INT, adoption_rate REAL)
hub_routine(id, name, maturity, version, goal, phases JSON, loop_config JSON,
            agent, uses INT)
hub_cron(id, task, frequency, last_run, next_run, project_id FK NULL, status)
hub_connector(id, name, type, status, last_sync)
hub_knowledge(id, name, type, chunks INT, used_by JSON, updated_at)
hub_activity(id, routine, agent, project_id FK NULL, duration_s, iters INT,
             result, ran_at)
```

**同步留口**:每张表加 `updated_at` + `rev`;`bw-store` 暴露 `SyncCursor`,Phase 3 接云端时不改 schema。WASM/Web 端把 `Store` trait 换成 IndexedDB adapter 或远端 REST。

---

## 6. 工作流执行引擎

放 `bw-engine`。**核心思路**:把 workflow 的阶段化执行建模成一张**执行图 + append-only 事件流**,由可替换的 `Executor` trait 驱动;桌面/测试用 `MockExecutor`,Phase 2 换 `AnthropicExecutor`。

```rust
#[async_trait]
pub trait Executor: Send + Sync {
    async fn run_phase(&self, phase: &PhaseNode, ctx: &RunCtx) -> Result<PhaseOutput>;
}

pub struct PhaseNode {
    pub name: String,
    pub prompt: String,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub max_iter: u8,
    pub retries: u8,
}

pub struct PhaseOutput {
    pub text: String,    // 回复文本(追加到 session.msgs)
    pub done: bool,      // loop 是否达成 goal
    pub gaps: Vec<String>,
}

pub struct Engine<E: Executor> {
    executor: E,
}

impl<E: Executor> Engine<E> {
    pub async fn run_workflow(
        &self,
        spec: &WorkflowSpec,
        on_event: impl Fn(RunEvent) + Send,
    ) -> Result<RunSummary> {
        // 1. 按 spec.phases 依序执行每个 PhaseNode
        // 2. 每轮产出 → on_event(RunEvent::PhaseCompleted { idx, output })
        // 3. loop:若 output.done → 完成;否则 gaps 重跑(≤loop_config.max_iter)
        // 4. 完成 → RunSummary { phases_run, final_output }
    }
}

pub enum RunEvent {
    PhaseStarted { idx: usize, name: String },
    PhaseCompleted { idx: usize, output: PhaseOutput },
    WorkflowDone { summary: RunSummary },
    WorkflowFailed { error: String },
}
```

**真实执行 = 同事团队(经 `Executor` trait 接入,不在我们交付内)**:`AnthropicExecutor`(`reqwest` 调 Anthropic Messages API 或起 **Claude Code 子进程**让子 agent 真用 Skill/Read/Bash/MCP)**由同事实现 —— 我们不写它**。我们聚焦创建 + 管理。

**∴ `Executor` 是跨团队接口,按契约管理**:
- 我们交付 `Executor` trait + `PhaseNode`/`PhaseOutput`/`RunEvent` 类型 + **`MockExecutor` 参考实现** + **一致性测试套件**;同事的真实现照此契约编码、过同一套测试即可热插拔。
- **契约在 P1 冻结**并版本化,好让两队并行;改契约 = 跨队事件,不可单方改。
- 我们全程跑在 MockExecutor 上 —— UI 与引擎结构对真假实现零改动。这正是 `Executor` trait 的价值,如今它还是一条**团队缝**。

---

## 7. 其余子系统(`bw-providers`)

| 子系统 | trait / 形态 | 说明 |
|---|---|---|
| **Connectors** | `trait Connector { async fn pull(&self)->Vec<Observation> }` | git/PR、CI/eval、网关日志、竞品源;产出 `MeasuredValue` 喂指标(度量内建的真实来源)。注册表 + 配置。 |
| **Cron/定时** | `trait Scheduler { fn schedule(Cadence, JobId) }` + tokio 定时 | 驱动 Routine 监测(单轮执行 + 按 Cadence 周期再触发);失败 N 次 → 推通知。 |
| **Skills/Agents Hub** | `trait HubRegistry { fn list_skills(); fn list_agents() }` | 装载本地 `.claude/skills/*` + 远端 Hub;给子 agent 注入 skill_listing。 |
| **Knowledge** | `trait MemoryStore { fn recall(); fn remember() }` | 项目知识、历史运行;向量检索 Phase 3。 |
| **通知** | `Inbox` 聚合(AppState 内)+ `PushNotifier` trait | 汇聚 monitor 告警、session 完成、connector 同步失败等。绿色隐身,只红/黄出声。 |

---

## 8. 分阶段落地

> **路线图已迁至 [`00 §7`](00-PLAN.md) + [`04`](04-effort-and-mvp.md)**:反转为 **P0 基座 → P1 架构脊椎(headless 证透)→ P2 纵切 UI(证 Event→Signal 桥)→ P3 铺屏**,之后 Tier B–E 加法叠加。MVP 单人 ≈ 67–122 天(M1 走通脊椎)/ ~100–190 天(M2 保真)。下表是**原四阶段**,保留作能力映射参考 —— 注意原 Phase 0「先还原所有屏幕」已被反转,且原 Phase 2/3 的真执行/Connector/Web 现为 Tier C/D/E。

| 阶段 | 内容 | 可验收产物 | 主要风险 |
|---|---|---|---|
| **Phase 0 · UI 壳 + mock** | workspace + 全部 crate 骨架;`bw-core` 实体 + signal 推导;`ui::selector` + `MockExecutor`;Dioxus 桌面还原所有屏幕(三视图 + 5panel×8scope + 9 Hub 全屏) | Mac+Win 双平台跑起来;导航全通;selector 驱动 SVG sparkline 正确渲染;`cargo test` 覆盖 signal 推导 / selector 派生规则 | RSX 内联 CSS 保真度;中文字体加载;SVG sparkline 数学复刻 |
| **Phase 1 · 本地持久化 + 真实领域** | `bw-store` SQLite + 迁移;命令-事件总线接库;项目/指标/会话/工作流 CRUD;向导 7 步真写库 | 关机重开数据还在;向导完成 → 运营视图真读库渲染;session 消息持久;promote dynamic→static 真写 | JSON 字段演化/迁移策略;`Manual` source → UI 强警示 |
| **Phase 2 · 真实执行 + Claude 接入** | `bw-providers::AnthropicExecutor`(或 Claude Code 子进程);会话 chat 真调 Claude;routine cadence 调度真运行;connector 先接一个(git) | session 消息真回;routine feed 真追加;promote 后下次 run 用 static 版本 | API 成本与限流;Claude Code 子进程在 Windows 稳定性;connector 鉴权流 |
| **Phase 3 · Connectors/Cron + Web 端** | 多个 Connector 真喂指标;Cron 驱动所有 Routine;`app-web`(WASM 复用 bw-app/ui)部署 | telemetry 驱动 signal 变色;Web 端复用 80% 跑同样所有屏幕;CJK 字体 subset 化 | WASM 下 providers 不能直连 → 需瘦后端;Web/桌面行为一致性 |

**最关键架构风险**

1. **UI 栈过早耦合** —— 若领域/引擎不慎渗入 UI 类型,桌面/Web 复用成本爆炸。对策:`bw-core/engine/app/ui` 严守零 UI 依赖;CI 加「内核 crate 不得依赖任何 UI crate」约束检查。
2. **Selector 正确性** —— `buildApp()` 的 12 个 `showXxx` 条件、健康概览过滤逻辑、sparkline 数学、commit timeline 映射规则如果 selector 算错,UI 呈现就错。对策:selector 是纯函数 → 大量单元测试,Phase 0 就验透。
