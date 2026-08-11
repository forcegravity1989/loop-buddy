> **30 秒导读(归档横幅,2026-08-11)**:本文是 vNext 切片设计事实源的**归档正本**,原生于执行会话的暂存目录,切片五收官时归档进仓。文末(或配套文件)的「主控裁决」是定案;实施中的偏差以任务报告与 commit 正文为准。现状以 `plan/23-opc-stitching-rebuild.md` 进度实况表为准。

# 切片二 · 连接器地基 —— Rust 设计稿

> **30 秒导读**:这份稿子给切片二的实现者看,是「照着写就能开工」的粒度。它落实 `plan/23-opc-stitching-rebuild.md` §3(连接器)与 §4(agentcli 层)的接缝要求,只设计**地基**——统一登记分发、四个按能力拆的小接口、一份最小机器契约,外加把现有 `gh` / `codehub-cli` / 脚本采集三家收编进来。**不重写任何上游逻辑**:v1 的 `github.rs` / `codehub.rs` 整体搬过来,外面包一层薄壳。写作日期 2026-08-10,现在作数。
>
> **术语提前说清**(避免黑话):**连接器**=BW 对外借力的一种成熟命令行工具的薄适配;**能力**=连接器能干的一类事(探活/执行/采集/Issue·MR 操作);**探活**=真的连上去问一句、拿到回答才算连上(装了不算);**登记**=把「这个项目用哪些连接器」记在一张表里;**契约**=适配器交回内核的数据必须长成的样子。

---

## 0. 这一片要解决的三个问题

1. 旧工程里,编排层直接调 `github::open_pr(...)`、直接 `tokio::process::Command::new("gh")`,连接器的知识散在调用点。新骨架明令禁止(plan/23 §6:「编排层不准出现连接器字符串分支或直接进程调用」)。
2. v1 已有一个雏形 `bw-engine/src/remote.rs`——provider 二选一的枚举分发。它解决了「一处 match」,但没解决:能力声明(某家不支持某动作时如何如实报)、超时取消、防重、错误分类。切片二要把它升级成有契约的注册表。
3. 验收线是硬的:**删除任何一种连接器,内核编译与其余连接器不受影响**。这条只能靠编译单元与 feature 边界成立,不能靠自觉。

---

## 1. 放哪:推荐新开一个薄 crate `next/crates/bw-connector`

### 结论

```
next/crates/
  bw-core         已就位(切片一 A 已移植,零改写)
  bw-connector    ← 本切片新建:契约 + 注册表 + 三家适配器(各自 feature 门)
  bw-engine       切片一/三建:agentcli 层(PTY)、采证器、指标正本管道
  bw-store        切片一建
  bw-app          切片一/四建:编排,只认 bw-connector 的接口
```

### 为什么不是 `bw-engine/src/connector/`

三条理由,按份量排:

1. **依赖方向与重量**。v1 的 `bw-engine` 为了终端栈带着 `portable-pty` / `conpty-oxide` 两个原生依赖(见 v1 `crates/bw-engine/Cargo.toml`),切片三还要往里搬 PTY 会话管理。编排层 `bw-app` 只需要「连接器接口」,不需要 PTY。契约放独立 crate,`bw-app` 依赖 `bw-connector`(可以 `default-features = false`,一个适配器都不编),编译面积和依赖面积都小一圈。
2. **验收线可执行**。「删掉任一连接器不影响其余」在独立 crate 里就是一条命令:`cargo check -p bw-connector --no-default-features --features gh`。塞在 `bw-engine` 里,这条 check 会连带把 PTY、采证、指标管道一起编,验收信号被噪音淹没。
3. **禁令可门禁化**。新增守卫脚本 grep `bw-app` 里的 `std::process::Command` / `tokio::process`,命中即失败。前提是 `bw-app` 的 Cargo.toml 里根本不该出现进程调用类依赖——独立 crate 让这条边界是「依赖图上的事实」,不是「代码评审时的记忆」。

### 为什么不是「每家连接器一个 crate」

这是单人桌面工具,不是集成平台(plan/23 §0 第四轮:做减法是纲)。三家适配器一共不到一千行,拆三个 crate 只增加 Cargo.toml 的维护量,换不来额外的隔离——feature 门已经给到同样的编译隔离。**一个 crate + 每家一个 feature** 是这个体量下的正确刻度。

### crate 内部结构

```
next/crates/bw-connector/
  Cargo.toml
  src/
    lib.rs          // 门面:pub use 契约与注册表;#[cfg] 导出各家构造器
    contract.rs     // §4 的全部类型:协议版本、能力、编号、错误分类、调用上下文
    caps.rs         // §3 的四个小接口 + 基座 trait Connector
    registry.rs     // §2 的 ConnectorRegistry
    adapters/
      mod.rs
      gh.rs         // feature = "gh"       —— 包 v1 github.rs
      codehub.rs    // feature = "codehub"  —— 包 v1 codehub.rs
      script.rs     // feature = "script"   —— 包 .bw/connectors.toml 脚本采集
    upstream/       // 搬过来的 v1 原文,除去 `use crate::workspace` 的路径修正外零改写
      github.rs
      codehub.rs
```

`upstream/` 这个目录名是刻意的:它在告诉后来者「这里面的东西不归你改,要改去上游 CLI 或去 adapters 层」。plan/23 §6 的「不把已包装的成熟 CLI 改写成原生实现」在目录结构上就有了物理提醒。

> 一个待办依赖:`upstream/github.rs` 里 `open_pr` 等函数调用了 v1 的 `workspace::stage_commit_push` / `git_in`。这些是**内建工作区函数**(§5 明确不进连接器接口),但适配器内部要用。切片二把 v1 `workspace.rs` 里被引用到的那几个函数一并搬进 `bw-connector/src/upstream/workspace.rs`,或者切片一先把 `workspace.rs` 落到 `bw-engine` 再由 `bw-connector` 依赖它。倾向后者(采证器也要用),但取决于切片一的落法——列进开放问题。

---

## 2. 统一登记与分发:`ConnectorRegistry`

### 登记条目只有三样

按任务约束,不做四层身份。一条登记 = 种类 + 项目绑定身份 + 本机配置引用。

```rust
// contract.rs

/// 连接器种类。按**能力家族**分,不按厂商分(plan/23 §3)。
/// 字符串形态只在存库/读文件时出现,进内存立刻收敛成这个枚举——
/// 编排层永远看不到 "github" 这种裸串。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ConnectorKind {
    /// 仓连接器:gh
    GithubRepo,
    /// 仓连接器:codehub-cli
    CodehubRepo,
    /// 采集连接器:项目仓里的采集脚本(`.bw/connectors.toml` 是正本)
    Script,
    /// 执行连接器:agent 类 CLI,由 agentcli 层实现(切片三填,本片只留位)
    AgentCli { cli: String },
}

/// 项目绑定身份 —— 「这条连接器代表这个项目在上游的哪个位置」。
/// 三家都能装得下:
///   - github: host = "github.com"(gh 全局,不用但保留对称), path = "owner/repo"
///   - codehub: host = "green"/"yellow"/内源域名,   path = "org/repo"
///   - script:  host = "",                          path = 工作区根的绝对路径
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectBinding {
    pub project: bw_core::ProjectId,
    pub host: String,
    pub path: String,
}

/// 本机配置引用 —— **只存引用,不存值**(plan/23 §3 纪律:凭证只放系统钥匙串
/// 或本机配置,不进项目仓不进日志)。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigRef {
    /// 靠这个 CLI 自己的登录态(`gh auth login` / `codehub-cli auth login`)。
    /// 值就是 PATH 上的可执行名。BW 一个字节的凭证都不碰。
    CliLogin { bin: String },
    /// 采集脚本:相对工作区根的脚本路径 + 运行命令 + 输出文件。
    /// 三个字段直接对应 `.bw/connectors.toml` 的 script/command/output。
    Script { script: String, command: String, output: String },
    /// agentcli:注册表里的那一行(切片三的 TuiAgentConfig 的 slug)。
    AgentRegistryRow { slug: String },
}

#[derive(Clone, Debug)]
pub struct ConnectorEntry {
    pub id: bw_core::ConnectorId,
    pub name: String,          // 项目内唯一,脚本连接器沿用 .bw/connectors.toml 的 name
    pub kind: ConnectorKind,
    pub binding: ProjectBinding,
    pub config: ConfigRef,
}
```

### 注册表本体

```rust
// registry.rs

/// 一条登记 + 它对应的活体适配器。
struct Registered {
    entry: ConnectorEntry,
    conn: Arc<dyn Connector>,
}

#[derive(Default)]
pub struct ConnectorRegistry {
    items: Vec<Registered>,
}

impl ConnectorRegistry {
    /// 装载入口。composition root(桌面壳 / headless 指挥器)调它。
    /// 注册表本身不知道 "gh" 这个词——是谁把 GhConnector 塞进来的,谁负责 feature 门。
    pub fn register(&mut self, entry: ConnectorEntry, conn: Arc<dyn Connector>) { … }

    /// 全量列举(界面「连接器」区、探活巡检用)。
    pub fn entries(&self) -> impl Iterator<Item = &ConnectorEntry> { … }

    /// 按项目 + 能力路由。返回的是**引用切片**,不是单个:
    /// 采集连接器天然多条(一个项目可有多个采集脚本)。
    pub fn probes(&self, p: ProjectId)     -> Vec<(&ConnectorEntry, &dyn Probe)> { … }
    pub fn collectors(&self, p: ProjectId) -> Vec<(&ConnectorEntry, &dyn Collect)> { … }
    pub fn executors(&self, p: ProjectId)  -> Vec<(&ConnectorEntry, &dyn Execute)> { … }

    /// 仓连接器是**每项目至多一条**(一个项目的活提到哪个仓,不能有歧义)。
    /// 找到零条 → Err(NotConnected);找到多条 → Err(Ambiguous),绝不「取第一条」蒙混。
    pub fn issue_ops(&self, p: ProjectId) -> Result<(&ConnectorEntry, &dyn IssueOps), RoutingError> { … }
}

#[derive(Debug, thiserror::Error)]
pub enum RoutingError {
    #[error("项目未绑定{0}能力的连接器")]
    NotConnected(Capability),
    #[error("项目绑定了 {n} 条{cap}连接器,无法判定用哪条")]
    Ambiguous { cap: Capability, n: usize },
}
```

### 装载来源(谁生成 `ConnectorEntry`)

| 种类 | 来源 | 何时刷新 |
|---|---|---|
| GithubRepo / CodehubRepo | project 行的 `provider` / `remote_host` / `remote_path`(v1 `Remote::for_project` 的三元组,原样沿用) | 项目加载时 |
| Script | 工作区里的 `.bw/connectors.toml` 正本,经解析器读入(v1 `connectors_file.rs` 搬过来) | 项目加载时 + merge 后同步 |
| AgentCli | agentcli 层的静态注册表一行(切片三) | 启动时 |

**解析器不是连接器**:`connectors_file.rs`(读 `.bw/connectors.toml`)只是把正本变成 `ConnectorEntry`,它自己不实现四个接口。它住在 `bw-connector/src/adapters/script.rs` 旁边(`script_source.rs`)还是住在指标域,见开放问题。

### 探活状态放哪

「装了 ≠ 连上了」的状态是**推导投影**,不是持久事实:注册表里存的是登记(静态),探活结果是每次问出来的(动态)。首版建议 `probe()` 的结果只进内存缓存 + 「待人处理」投影,不落 `connector.status` 列——理由是落了列就要管过期,而过期的绿是这个仓库最反对的东西。但 v1 表里已有 `status` / `last_sync` 列,取舍见开放问题。

---

## 3. 按能力拆的小接口(明确禁止一个大 trait)

### 基座 trait:只管身份与能力上转

```rust
// caps.rs
use async_trait::async_trait;

/// 所有连接器的基座。**它自己不含任何业务方法**——业务全在四个小接口里。
/// 基座只回答两件事:你是谁、你能干哪几类事。
pub trait Connector: Send + Sync {
    /// 契约协议版本。适配器写死 `contract::PROTOCOL`;
    /// 注册时不匹配当前版本的一律拒绝登记(而不是运行时才炸)。
    fn protocol(&self) -> u32 { contract::PROTOCOL }

    fn kind(&self) -> &ConnectorKind;
    fn binding(&self) -> &ProjectBinding;

    // ─── 能力声明 = 手写上转,不用 Any,不用 downcast ───
    fn as_probe(&self)     -> Option<&dyn Probe>    { None }
    fn as_execute(&self)   -> Option<&dyn Execute>  { None }
    fn as_collect(&self)   -> Option<&dyn Collect>  { None }
    fn as_issue_ops(&self) -> Option<&dyn IssueOps> { None }

    /// **provided 方法,由上面四个推导出来** —— 声明与实现不可能分叉,
    /// 因为 `as_probe` 返回 `Some(self)` 编译器要求 `Self: Probe`。
    /// 界面上「这条连接器支持什么」直接读它,不需要第二处维护。
    fn capabilities(&self) -> CapabilitySet {
        let mut s = CapabilitySet::EMPTY;
        if self.as_probe().is_some()     { s = s.with(Capability::Probe); }
        if self.as_execute().is_some()   { s = s.with(Capability::Execute); }
        if self.as_collect().is_some()   { s = s.with(Capability::Collect); }
        if self.as_issue_ops().is_some() { s = s.with(Capability::IssueOps); }
        s
    }
}
```

#### 声明机制的取舍(三选一,推荐第三)

| 方案 | 好处 | 坏处 | 判 |
|---|---|---|---|
| **trait 组合**(`trait RepoConnector: Probe + IssueOps`) | 编译期最强 | 注册表要存 `Arc<dyn ?>`,能力组合有 2⁴ 种,存不下;运行时也查不到「支持什么」 | ✗ |
| **能力枚举 + `dyn Any` downcast** | 灵活,加能力不动基座 | 丢类型、downcast 失败只能运行时报;`dyn Any` 与 `#[async_trait]` 的对象安全性纠缠;声明与实现会分叉(声明了 Probe 却没实现,编译期不拦) | ✗ |
| **枚举 + 手写上转方法(推荐)** | 对象安全、零 `Any`、零 unsafe;**声明由实现推导,分叉不可能**;新增能力=基座加一个 default 方法,老适配器不动 | 加第五个能力要动基座一次(可接受:能力家族是有限且慢变的) | ✓ |

### 四个小接口

```rust
// ─── ① 探活 ───────────────────────────────────────────────
#[async_trait]
pub trait Probe: Send + Sync {
    /// 真的连上去问一句。成功返回一行人话详情(如 `owner/repo · private · 最近推送 2026-08-09`)
    /// 与结构化身份;失败按 §4 的分类如实报。**绝不因为「CLI 装了」就返回成功。**
    async fn probe(&self, cx: &CallCtx) -> ConnResult<ProbeReport>;
}

pub struct ProbeReport {
    pub reachable: bool,       // 恒 true;false 走 Err,这里留给「连上了但只读」这类将来
    pub identity: String,      // 上游认定的身份,如 "owner/repo" / "org/repo" / 脚本绝对路径
    pub detail: String,        // 一行人话,直接可上界面
}

// ─── ② 执行(切片三 agentcli 层填,本片只定型)─────────────
#[async_trait]
pub trait Execute: Send + Sync {
    /// 起一次执行。**只起,不等**——等是运行管理器的事(切片四)。
    async fn start(&self, cx: &CallCtx, spec: ExecSpec) -> ConnResult<ExecTicket>;
    /// **轮询**当前状态(plan/23 §6:不建订阅/游标/重放)。
    async fn poll(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<ExecState>;
    /// 取消。幂等:已结束的 ticket 取消 = Ok。
    async fn cancel(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<()>;
}

// ─── ③ 采集 ───────────────────────────────────────────────
#[async_trait]
pub trait Collect: Send + Sync {
    /// 跑一次采集,交回**原始 JSON**。按字段路径取值(`collect_query`)是指标域的事,
    /// 连接器不解释语义——沿用「采集器从不解释」的老规矩。
    /// 采不到就 Err,**绝不返回 0 冒充「采到了 0」**(假零是本仓库的红线)。
    async fn collect(&self, cx: &CallCtx, req: CollectReq) -> ConnResult<CollectOut>;
}

pub enum CollectReq {
    /// 脚本连接器:跑脚本 → 读 output 文件 → 整份 JSON 回来
    ScriptRun,
    /// 仓连接器:一条计数查询(gh `search/issues` total_count / codehub list length)
    RemoteCount { query: String, today: time::Date },
}

pub struct CollectOut {
    pub value: serde_json::Value,  // ScriptRun 给整份 JSON;RemoteCount 给 Number
    pub source_hint: String,       // 「这个数从哪来的」——证据链要点开原始出处
}

// ─── ④ Issue·MR 操作 ──────────────────────────────────────
#[async_trait]
pub trait IssueOps: Send + Sync {
    async fn create_issue(&self, cx: &CallCtx, idem: IdemKey, title: &str, body: &str)
        -> ConnResult<WriteOutcome<u32>>;
    async fn issue_state(&self, cx: &CallCtx, number: u32) -> ConnResult<IssueState>;
    async fn close_issue(&self, cx: &CallCtx, idem: IdemKey, number: u32) -> ConnResult<()>;

    /// 提 MR/PR:内部会 stage+commit+push 活分支(用**内建**工作区函数,
    /// 那些函数不出现在本接口上,见 §5)。永不 merge。
    async fn open_change(&self, cx: &CallCtx, idem: IdemKey, req: OpenChangeReq)
        -> ConnResult<WriteOutcome<u32>>;
    async fn change_state(&self, cx: &CallCtx, number: u32) -> ConnResult<ChangeState>;
    /// 找某分支上已开的 MR/PR(飘移巡检:队友自己开的 PR 也要认出来)。
    async fn open_change_for_branch(&self, cx: &CallCtx, branch: &str) -> ConnResult<Option<u32>>;
    /// **人点的那一下**。只能由编排层的显式合入命令调,任何执行路径不许调。
    async fn merge_change(&self, cx: &CallCtx, idem: IdemKey, number: u32) -> ConnResult<()>;

    /// 门禁检查结果。v1 两家都没有对应实现 —— 默认如实返回「不支持」,
    /// 谁先补上谁 override,**不给假空列表**(空列表会被读成「检查全过」)。
    async fn checks(&self, _cx: &CallCtx, _number: u32) -> ConnResult<Vec<CheckRun>> {
        Err(ConnError::Unsupported { cap: Capability::IssueOps, op: "checks" })
    }
}

/// 上游状态归一化:`OPEN`/`CLOSED`/`opened`/`closed` 这些原生词只活在适配器里。
pub enum IssueState { Open, Closed }
pub enum ChangeState { Open, Merged, Closed }
pub struct CheckRun { pub name: String, pub conclusion: CheckConclusion, pub url: String }
pub enum CheckConclusion { Passed, Failed, Running, Unknown }
```

---

## 4. 最小机器契约

契约钉在 trait 层。**外部工具的原生输出管不了**——契约约束的是适配器归一化后交回内核的东西。这是防「各连接器各自重新长出字符串分支」的闸。

```rust
// contract.rs

/// 协议版本。改动契约类型的形状(增删字段、改语义)必须 +1;
/// 注册时 `conn.protocol() != PROTOCOL` 一律拒绝登记,不做兼容层
/// (CLAUDE.md:不为向后兼容留旧路径)。
pub const PROTOCOL: u32 = 1;

/// 能力名。既是路由键,也是「不支持」错误里说得清的那个词。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability { Probe, Execute, Collect, IssueOps }

/// 手搓的位集(不引 bitflags 依赖,四个位不值得一个 crate)。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapabilitySet(u8);
impl CapabilitySet {
    pub const EMPTY: Self = Self(0);
    pub const fn with(self, c: Capability) -> Self { Self(self.0 | 1 << c as u8) }
    pub const fn has(self, c: Capability) -> bool { self.0 & (1 << c as u8) != 0 }
}

/// 请求编号 —— 一次调用一个,进 stderr 诊断日志、进运行记账、进「待人处理」条目。
/// 出问题时能把「界面上这条红」和「当时那次 gh 调用」对上。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestId(pub uuid::Uuid);

/// 防重编号 —— 只有**可重试的写操作**要带(create_issue / open_change /
/// merge_change / close_issue)。读操作和采集不带。
///
/// 由调用方(运行管理器)**确定性**生成,同一件活的同一个动作永远同一个值,
/// 例:`issue-42/open-change`。重试时编号不变,这是「同一请求重发不做两遍」的锚。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdemKey(pub String);

/// 写操作的结构化结果。**这是防重的落地形态,也是唯一诚实的形态**:
/// gh / codehub-cli 都没有幂等头,唯一可靠的做法是「先读回、再决定」。
/// v1 `open_pr` 的 `PrOpened::{Created, Adopted}` 就是这个模式的先例,契约把它推广。
pub enum WriteOutcome<T> {
    /// 这次调用真的创建了它
    Created(T),
    /// 上游已经有了(可能是上一次重试建的,也可能是队友自己建的)——
    /// **号码是真读回来的**,绝不从错误文本里抠、绝不猜
    AlreadyExisted(T),
}

/// 结构化失败分类。适配器负责把 gh/codehub 的原生错误映射到这七类。
#[derive(Debug, thiserror::Error)]
pub enum ConnError {
    #[error("该连接器不支持{cap:?}的 {op} 操作")]
    Unsupported { cap: Capability, op: &'static str },
    /// CLI 不在 PATH、没登录、host 不可达 —— 「装了≠连上了」的失败面
    #[error("未连接:{0}")]
    NotConnected(String),
    #[error("超时({0:?} 未回)")]
    Timeout(std::time::Duration),
    #[error("已取消")]
    Canceled,
    /// 连上了,上游明确说不行(权限不足、分支不存在、MR 已合……)
    #[error("上游拒绝:{message}")]
    UpstreamRejected { message: String },
    /// 连上了、也回了,但回的东西解析不了 —— **这一类绝不降级成「没数据」**,
    /// 它是真故障,必须进「待人处理」
    #[error("输出不可解析:{raw}")]
    Unparsable { raw: String },
    #[error("其它:{0}")]
    Other(String),
}

pub type ConnResult<T> = Result<Ok<T>, Fail>;

/// 成功也带元数据 —— 证据链要能点开原始出处。
pub struct Ok<T> { pub req: RequestId, pub took: std::time::Duration, pub value: T }
/// 失败也带编号 —— 「待人处理」的一条要能追回是哪次调用。
pub struct Fail { pub req: RequestId, pub took: std::time::Duration, pub err: ConnError }
```

### 超时与取消(tokio)

```rust
/// 每次调用的上下文。**四个小接口的每个方法首参都是它**——这是统一的,不给例外。
pub struct CallCtx {
    pub req: RequestId,
    /// 相对超时。`None` = 用 kind 的默认档(见下表)。
    pub timeout: Option<std::time::Duration>,
    /// 取消令牌(`tokio_util::sync::CancellationToken`)。
    pub cancel: CancellationToken,
}
```

**语义,三条,适配器不许各自发明**:

1. **超时由契约层统一实现**,适配器**不各自写 timeout**。`bw-connector` 提供包装器:
   ```rust
   pub(crate) async fn guarded<T, F>(cx: &CallCtx, kind_default: Duration, fut: F) -> ConnResult<T>
   where F: Future<Output = Result<T, ConnError>> { /* tokio::select! { _ = cancel.cancelled() => Canceled,
                                                        _ = sleep(d)      => Timeout(d),
                                                        r = fut           => r } */ }
   ```
   适配器只写业务 future,超时/取消这两条边由包装器兜。
2. **取消 = 真杀子进程**。`tokio::process::Child` 在 select 落选分支被 drop 时,配 `.kill_on_drop(true)` 保证子进程不留残。取消返回 `ConnError::Canceled`,不返回成功也不返回超时。
3. **写操作超时后状态未知,必须如实标注**。`open_change` 超时不代表 MR 没建成——契约不假装知道。运行管理器拿到写操作的 `Timeout`,应带着同一个 `IdemKey` 重试;重试走 read-before-write 会得到 `AlreadyExisted`,账就对上了。**超时绝不当失败记账**(这是「同一件活绝不重复记账」在连接器边界上的体现)。

默认超时档(可被 `CallCtx.timeout` 覆盖):

| 操作类 | 默认 | 理由 |
|---|---|---|
| 探活 | 10s | 快失败,界面上要立刻显灰而不是转圈 |
| 读(issue_state / change_state / open_change_for_branch) | 20s | 一次 API 往返 |
| 采集 · RemoteCount | 30s | 搜索接口慢一档 |
| 采集 · ScriptRun | 180s | 项目脚本可能跑 Playwright/SSO,给宽 |
| 写(create_issue / open_change / merge_change) | 60s | 含 push,大仓要时间 |

---

## 5. 收编映射:现有函数怎么归位

### v1 `github.rs`(`gh`)

| v1 函数 | 归入 | 说明 |
|---|---|---|
| `probe_repo` | **Probe**`::probe` | 一行人话直接进 `ProbeReport.detail` |
| `create_issue` | **IssueOps**`::create_issue` | 加 read-before-write:先按标题查同名 open issue?→ 见开放问题 |
| `issue_state` | **IssueOps**`::issue_state` | `OPEN`/`CLOSED` → `IssueState` |
| `close_issue` | **IssueOps**`::close_issue` | 已关再关是 no-op 成功,天然幂等 |
| `open_pr` (+ `adopt_existing_pr`) | **IssueOps**`::open_change` | `PrOpened::{Created,Adopted}` 原样映射到 `WriteOutcome::{Created,AlreadyExisted}`——**这是契约里 `WriteOutcome` 的设计来源** |
| `pr_state` | **IssueOps**`::change_state` | `OPEN`/`MERGED`/`CLOSED` → `ChangeState` |
| `open_pr_for_branch` | **IssueOps**`::open_change_for_branch` | `Ok(None)` = 没有,不是错 |
| `merge_pr` | **IssueOps**`::merge_change` | 只从编排层的显式合入命令进来 |
| `collect_github_count` (+ `expand_query` / `days_ago_iso`) | **Collect**`::collect(RemoteCount)` | 滚动窗口宏 `@{7d}` 的展开留在适配器内,不进契约 |
| `create_repo` / `clone_repo` / `list_repos` | **不进四个接口** | 接入期一次性动作,见下 |
| `push_head` / `current_branch` / `push_current_branch` / `sync_default_branch` / `origin_remote_url` / `remote_matches` / `reconcile_local_remote` / `checkout_issue_branch` / `issue_branch` | **不进接口** | 本地 git 操作 = 内建工作区函数 |
| `current_login` / `gh_json_field` / `spawn_err` / `stderr_text` | 适配器内部 | 私有辅助 |

### v1 `codehub.rs`(`codehub-cli`)

一一对称,同一个 `IssueOps` 的另一实现:`probe`→Probe;`create_issue`/`create_mr`/`open_mr_for_branch`/`merge_mr`→IssueOps;`collect_count`→Collect;`clone_repo`/`create_repo`/`list_repos`/`resolve_personal_namespace_id`→不进接口。

**一处诚实差异要保住**:codehub 的 `create_mr` 在 MR 已存在时是**如实失败**(v1 注释明确:没有 `Adopted` 路径)。收编后它返回 `ConnError::UpstreamRejected`,**不许**为了「和 github 长得一样」硬造一个 `AlreadyExisted`。契约允许两家在同一方法上给出不同的诚实结果——这正是契约的意义:形状统一,事实不粉饰。

### `remote.rs` 的处置

v1 `remote.rs` 是本设计的直接前身(provider 二选一的枚举分发)。收编后**它整个消失**:它的 `for_project` 变成注册表的装载逻辑,它的每个方法变成 `IssueOps` / `Collect` 的一个方法,它的 `match` 变成注册表的路由。不留兼容壳(CLAUDE.md:不为向后兼容留旧路径)。

### 脚本连接器(`.bw/connectors.toml`)→ Collect

- 正本格式不动(`docs/connectors-toml-format.md` 继续作数),v1 `connectors_file.rs` 解析器整体搬。
- 一条 `[[connector]]` → 一个 `ConnectorEntry { kind: Script, config: ConfigRef::Script { script, command, output } }`,身份仍是 `(project_id, name)`。
- `Collect::collect(ScriptRun)` = 在工作区根跑 `command script` → 读 `output` 文件 → 整份 JSON 回来。**只读 output 文件、丢弃 stdout** 这条老规矩原样保留(2026-08-06 真实事故:脚本只 print 不落盘,指标永远 Unknown)。`output` 为空 → 直接 `ConnError::NotConnected("未配置 output,采集必然采不到")`,在探活阶段就说清,不等到采集时静默失败。
- `Probe`:脚本连接器**也实现探活**——检查命令在 PATH、脚本文件存在、output 目录可写。这三条不过就不算「连上了」。
- 绝对路径脚本 → `ConnError::UpstreamRejected`(v1 已有此校验,原样保留)。

### 明确不进接口的两类(边界要写死)

1. **本地工作区读取 = 内建采证**(plan/23 §3 词表 V1 定义)。`evidence.rs`(git/docs/测试真状态)、`git_log.rs`(commit 列表)是 `bw-engine` 的内建函数,不是连接器,不走 `CallCtx`,不进注册表。
2. **本地 git 写操作 = 内建工作区函数**。`stage_commit_push` / `checkout_issue_branch` / `clone` 这些。注意这条**不禁止适配器内部使用它们**——`open_change` 内部就要 stage+commit+push。边界是:**它们不出现在四个接口的方法签名上**,编排层不能绕过连接器直接调它们来完成一次「提 MR」。

**建仓 / 克隆 / 列仓的处置(倾向,非定论)**:这三个是项目**接入期**的一次性动作,不是项目日常运转的能力。硬塞进 `IssueOps` 会让这个接口从「一件活的生命周期」变成「什么都装」。首版建议留在 `bw-engine` 的接入期自由函数里(与工作区 provisioning 同族),等切片七真实项目切换时再判断是否值得开第五个能力 `Provision`。列进开放问题。

---

## 6. 给 agentcli 层留的口(只留接缝,不设计内部)

切片三的 agentcli 层(PTY 会话栈)**在 `bw-engine` 里实现**,因为 PTY 依赖重。它挂进同一个注册表的方式:

```rust
// bw-engine/src/agentcli/connector.rs(切片三写,本片只定型)
pub struct AgentCliConnector { /* 注册表行 + 会话管理器句柄,内部结构切片三定 */ }

impl bw_connector::Connector for AgentCliConnector {
    fn kind(&self)    -> &ConnectorKind { &self.kind }      // AgentCli { cli: "claude" }
    fn binding(&self) -> &ProjectBinding { &self.binding }  // project + 工作区根
    fn as_execute(&self) -> Option<&dyn Execute> { Some(self) }
    // 其余三个能力用 default —— 执行连接器不探仓、不采集、不动 Issue
    // (探活是否要实现见开放问题:`claude --version` 算不算探活)
}
```

`bw-engine` → `bw-connector` 单向依赖,不循环。composition root:

```rust
// 桌面壳 / headless 指挥器的组装处
#[cfg(feature = "gh")]      reg.register(gh_entry,      Arc::new(GhConnector::new(binding)));
#[cfg(feature = "codehub")] reg.register(codehub_entry, Arc::new(CodehubConnector::new(binding)));
#[cfg(feature = "script")]  for e in script_entries { reg.register(e, Arc::new(ScriptConnector::new(..))); }
// 切片三加这一行,注册表代码零改动:
reg.register(agent_entry, Arc::new(AgentCliConnector::new(..)));
```

**契约层为 agentcli 预留的、本片就要定死的只有三个类型**(内部一律不碰):

```rust
/// 起一次执行要给的最小信息。PTY、hook、session.jsonl、prompt 注入模式
/// 全部是 agentcli 层内部的事,**一个字都不进契约**。
pub struct ExecSpec {
    pub workspace: std::path::PathBuf,
    pub branch: String,
    /// 要注入的正文块(技能正文 / 蒸馏块 / 目录块)。契约只知道「有一段文本要送进去」,
    /// 不知道它是走 argv 还是走系统提示词——那是注册表那一行的事。
    pub inject: Vec<InjectBlock>,
    pub budget_usd: Option<f64>,
}
pub struct InjectBlock { pub label: String, pub body: String }

/// 一次执行的句柄。**上游会话号是核心**:agent CLI 的 resume 靠它。
pub struct ExecTicket { pub req: RequestId, pub upstream_session: Option<String> }

/// 轮询回来的状态。刻意只有五档 —— 过程细节留在上游 CLI 里(plan/23 §2)。
pub enum ExecState {
    Running,
    /// 干完了。**最远只能推到「评审中」** —— 契约层面就没有「完成」这个档,
    /// 「完成」永远由人点(产品铁律,在类型上就断掉这条路)。
    Finished { ok: bool, summary: String },
    Canceled,
    /// 上游会话还在,但 BW 这边重启了 —— 如实标注,不假活
    Orphaned,
    Unknown,
}
```

> **刻意的类型断路**:`ExecState` 里没有 `Done`。执行连接器再成功也只产出 `Finished`,把 Issue 推到「评审中」;从「评审中」到「完成」的唯一入口是 `bw-core` 状态机的显式转移。产品铁律在契约类型上就成立,不靠纪律。

---

## 7. 验收对齐:「删掉任一连接器不影响其余」怎么成立

### 编译单元与 feature 边界

```toml
# next/crates/bw-connector/Cargo.toml
[features]
default  = ["gh", "codehub", "script"]
gh       = []
codehub  = []
script   = ["dep:toml"]     # 只有脚本连接器要解析 .bw/connectors.toml

[dependencies]
bw-core     = { workspace = true }
async-trait = { workspace = true }
thiserror   = { workspace = true }
serde       = { workspace = true }
serde_json  = { workspace = true }
uuid        = { workspace = true }
time        = { workspace = true }
tokio       = { workspace = true, features = ["process", "time", "fs"] }
tokio-util  = { workspace = true }            # CancellationToken
toml        = { workspace = true, optional = true }
```

`src/adapters/mod.rs` 只有三行 `#[cfg(feature = "…")] pub mod …;`,`lib.rs` 的 `pub use` 同样带 cfg。**`contract.rs` / `caps.rs` / `registry.rs` 里不出现任何一家的名字**——注册表存的是 `Arc<dyn Connector>`,不是枚举 arm;删掉 gh 不需要动注册表一个字符。

### 门禁命令(进 CI,与切片一门禁并列)

```bash
cargo check -p bw-connector --no-default-features                        # 只剩契约+注册表 → 全删也编译过
cargo check -p bw-connector --no-default-features --features gh          # 只剩 gh
cargo check -p bw-connector --no-default-features --features codehub     # 只剩 codehub
cargo check -p bw-connector --no-default-features --features script      # 只剩脚本
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features   # 内核不受影响(它压根不依赖 bw-connector)
./scripts/guard-no-direct-process.sh                                     # 新增:编排层不准直接进程调用
```

「内核编译不受影响」这条是**依赖图上的事实**,不需要专门验证:`bw-core` 不依赖 `bw-connector`,方向是 `bw-connector → bw-core`。上面第五条只是把这个事实钉在 CI 上。

新增守卫脚本 `scripts/guard-no-direct-process.sh`(与 `guard-kernel-ui-free.sh` 同族):

```bash
# 编排层与内核里不准出现直接进程调用 —— 对外能力只准走连接器接口(plan/23 §6)
grep -rn 'std::process::Command\|tokio::process' next/crates/bw-core/src next/crates/bw-app/src \
  && { echo "✗ 编排层/内核出现直接进程调用"; exit 1; }
```

### 行为验收(headless 指挥器,不写单元测试)

新增 `next/crates/bw-connector/examples/probe_all.rs`:读一个真实项目的登记 → 逐条 `probe()` → 打印 `种类 · 身份 · 能力集 · 探活结果`,失败按七类分类如实打印。验收动作 =

```bash
cargo run -p bw-connector --example probe_all -- <db-path> <项目名>
sqlite3 <db> "SELECT kind, name, project_id FROM connector WHERE project_id = '…';"   # 登记读回
```

指挥器**必须包含一条故意失败的路径**(把 `gh` 从 PATH 临时移开 / 指一个不存在的 host),证明失败真的落成 `NotConnected` 而不是被吞成绿。

---

## 8. 开放问题(拿不准的,不硬拍)

1. **`workspace.rs` 落在哪?** 适配器内部要用 `stage_commit_push` / `git_in`,采证器也要用。是切片一先把它落到 `bw-engine` 然后 `bw-connector` 依赖 `bw-engine`(会让依赖方向变成 connector→engine,与「engine 实现 Execute 后 engine→connector」形成循环),还是把这几个 git 函数单独落到 `bw-connector/src/upstream/workspace.rs` 各留一份?倾向后者(避免循环),但要接受一份 git 辅助代码有两个副本。
2. **建仓 / 克隆 / 列仓归哪?** 首版倾向留在接入期自由函数(不进四个接口),但这样「接入一个新项目」这条路就绕过了连接器接口——与「对外能力只准走连接器接口」的禁令是否算冲突?或者开第五个能力 `Provision`?
3. **`checks` 切片二就补吗?** v1 两家都没实现。`gh pr checks --json` 是现成子命令,包一层不算自建;但切片二的范围是「地基」,补 checks 算不算超纲?
4. **`create_issue` 要不要 read-before-write?** `open_change` 有天然的分支查重锚点(按 head 分支查),`create_issue` 没有——按标题查重不可靠(标题可重名)。若不做,`IdemKey` 对 `create_issue` 就只是日志关联,不是真防重。是接受这个不对称(如实标注哪些操作真防重、哪些只是可追溯),还是靠本机去重表?
5. **`IdemKey` 落不落本机去重表?** 落表要新 schema(`connector_write_log`),换来跨进程重启的防重;不落表则防重只在单次会话内 + 靠 read-before-write。这个仓库有「重启后遗留清理」的需求(切片四),可能倾向落表。
6. **错误文本→分类的映射允许到什么程度?** v1 `open_pr` 靠 `stderr.contains("already exists")` 认领幂等——这是真管用的先例,但也正是「字符串分支」。契约是否允许适配器内部保留少量这类映射(集中在一个 `classify(stderr) -> ConnError` 函数里、每条都要注明验证日期),还是一律归 `UpstreamRejected` 交给人?
7. **探活结果落不落 `connector.status` / `last_sync` 列?** 落了要管过期(过期的绿是红线),不落则每次开界面都要现探(慢)。折中是「落列 + 带时间戳 + 超过 N 分钟一律显示 Unknown」,但 N 取多少没依据。
8. **一个项目能否同时绑多个仓连接器?** 本设计对 `IssueOps` 判定「多于一条 = `Ambiguous` 报错」。这是保守选择;若将来真有 github + 内源 codehub 双推的项目,要改成主/副?
9. **`.bw/connectors.toml` 解析器住哪?** 放 `bw-connector`(它生产 `ConnectorEntry`)还是放指标域(它和 `.bw/metrics.toml` 是一对)?两个都说得通。
10. **agentcli 连接器要不要实现 `Probe`?** `claude --version` 能跑通算不算「连上了」?按「装了≠连上了」的严格口径,版本号只证明装了。是否要更强的探活(起一次最小会话)?代价是每次探活真花钱。
11. **通信连接器(通知出口)将来是第五个能力,还是复用 `Execute`?** 本片不做,但要不要现在就在 `Capability` 枚举里占位?(倾向不占位——空枚举项会诱导人往里填东西,做减法是纲。)
12. **`CollectOut.value` 对 `RemoteCount` 给 `Value::Number` 是否别扭?** 两种采集请求的返回形状差别很大(整份 JSON vs 一个数)。是否该拆成两个方法,还是保持一个方法 + 一个 enum 返回?
