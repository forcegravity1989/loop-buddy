# GitHub 为主体的创建引导流

日期:2026-07-22
状态:待用户复核

## 1. 目标与范围

把创建项目的起点从"本地目录"换成"GitHub 仓":引导流第一步决定仓从哪来(新建或接入已有),后续项目的方法论设置(周期/北极星/指标/阶段)不变。

现状(见调研):`Command::CreateProject` 今天只会 mint 一个**本地** git 目录(`bw_engine::provision_git_workspace`),或绑定一个已存在的**本地**路径;没有任何 GitHub API/CLI 集成。`plan/09-aihot-practice-run.md` 曾设计过"建项目=真在 GitHub 开仓"(墙 A),但 `iterations/PRACTICE-AIHOT.md` 记录了明确的不做决定——理由是无监督地在用户熟睡时改变真实账号可见状态,风险不对称。**本次是用户在 `/goal` 里显式推翻那个决定**,把它设为当前主线目标;不是自主夜间动作,是用户在场驱动的交互式创建流,风险类别不同。

机器上 `gh` CLI 已安装且已登录(`forcegravity1989`,scope 含 `repo`)——沿用 `plan/09` 早先设计的 shell-out 路线,不新建 OAuth/token 存储。

### 非目标(本次明确不做)

- 不做 GitHub OAuth/token 管理——完全依赖用户机器上已有的 `gh auth login`。
- 不做 GitHub issue/PR/CI 的持续同步或每日统计(那是 `plan/09` 墙 C 的 `github_stats.rs`/`cron_task.mode='pull_github'`,独立的后续功能)。
- 不做"事后补挂 GitHub"的专门重试 UI——新建仓失败后软降级为本地仓,用户可以手动 `git remote add` 或后续走已有的 `Command::SetWorkspace` 路径;这次不建一个新的重试面板。
- 不做多仓/mono-repo 项目支持。
- 不做仓库可见性之外的仓库配置(license/gitignore 模板等)——`gh repo create` 默认行为即可。

## 2. 引导流结构

```
Card::Repo(新)→ Card::Intent(改动小)→ Card::Questions(不变)→ Card::Drafting(不变)→ Card::Review(不变)
```

### 2.1 `Card::Repo`——仓从哪来

纯本地 UI 状态,提交前不触发任何后端调用(和今天"墙"页的 `+` 新建按钮先翻本地信号、不派发 `CreateProject` 是同一惯例)。

- **切换:新建仓 / 接入已有仓**
- **新建仓**:一个可见性开关,private(默认)/ public。
- **接入已有仓**:一个下拉,数据来自新命令 `Command::ListGithubRepos` → `bw_engine::github::list_repos(30)`(`gh repo list --json nameWithOwner,isPrivate,updatedAt --limit 30`)。切到"接入已有仓"时触发一次加载;结果落在 `Vm` 顶层新字段 `github_repos: Vec<GithubRepoSummary>`(不进 `CreateVm`,因为这时项目还不存在)——沿用 `Command::LoadVersionLog`/`LoadArtifacts` 的"显式加载"惯例,不是每次 rebuild 都打 GitHub API。
- "下一步" 只把选择存进本组件的本地信号,推进到 `Card::Intent`。

### 2.2 `Card::Intent`(在今天字段基础上小改)

今天:项目名称 / 项目类型 / 一句话意图,提交即 `k.send(Command::CreateProject{ workspace: None, .. })` 并立即前进到 Questions(乐观推进,不等结果)。

变动:
- 若上一步选了"新建仓":新增一个 slug 预览/编辑输入——项目名称栏每次输入时用一个纯函数 `slugify(name)`(小写化 ASCII、非 ASCII/空白段折成单个连字符、掐头去尾)实时刷新建议值,用户可手改。这是唯一必须新增的字段:GitHub 仓名要求 ASCII+连字符,而项目显示名允许中文,两者独立(用户已确认),不能事后静默转写而不给用户看到最终仓名。
- 若上一步选了"接入已有仓":一行只读回显"将接入 owner/repo ↗",不需要 slug 字段(仓名已固定)。
- "开始 ↑" 的行为不变——仍是乐观推进,立即 `on_created.call(())`,不等待、不加 spinner。把 `Card::Repo` 收集到的选择一并塞进新的 `Command::CreateProject.github` 字段。

**刻意选择**:不新增 `Event::ProjectRepoReady`、不新增每卡片的错误信号、不加载入态。理由:整个代码库里没有任何一个创建流步骤会阻塞卡片推进等结果——今天的本地 `provision_git_workspace` 已经是"立即前进+后台 toast 报告成败"模式(`bw-app/src/lib.rs:1634-1658` 的 `Event::ConnectorSynced{ok:false,..}` 分支),kernel 的 dispatch 循环本来就能吞得下几秒钟的阻塞调用(`RunWorkflow` 同理)。GitHub 版本的仓创建/克隆(1-3 秒网络调用)完全落在同一个已验证的模式里,复用它比新建一整套"进行中/失败态"UI 便宜得多,也和现有代码风格一致。

### 2.3 Questions / Drafting / Review

原样不动。

## 3. `bw-engine/src/github.rs`(新模块)

和 `workspace.rs`(唯一的"写"子进程模块)平级,同样是 `tokio::process::Command` shell 出 `gh`,同样的错误映射惯例(`thiserror`,把 stderr 摘要成 Rust 错误)。

```rust
pub struct GithubRepoRef { pub owner: String, pub repo: String, pub html_url: String, pub private: bool }
pub struct GithubRepoSummary { pub owner: String, pub repo: String, pub private: bool, pub updated_at: String }

#[derive(thiserror::Error, Debug)]
pub enum GithubError {
    #[error("gh 未安装或不在 PATH")]           NotInstalled,
    #[error("gh 未登录:{0}")]                  NotAuthenticated(String),
    #[error("gh 命令失败:{0}")]                Command(String),
}

pub async fn is_gh_ready() -> Result<(), GithubError>;                 // `gh auth status` 探活
pub async fn list_repos(limit: u32) -> Result<Vec<GithubRepoSummary>, GithubError>;
    // gh repo list --json nameWithOwner,isPrivate,updatedAt --limit <limit>
pub async fn create_repo(slug: &str, private: bool, dest_root: &Path) -> Result<GithubRepoRef, GithubError>;
    // cwd=dest_root, `gh repo create <slug> --private|--public --clone`
    // (gh repo create 的 --clone 把仓克隆到 CWD 下的 ./<slug>;调用方保证 dest_root/<slug> 就是 workspace_slug() 算出的同一路径)
pub async fn clone_repo(owner_repo: &str, dest: &Path) -> Result<GithubRepoRef, GithubError>;
    // `gh repo clone <owner>/<repo> <dest>`
```

不做 GitHub 侧命名合法性校验——`gh repo create` 自己会拒绝非法名并给出清楚的 stderr,直接把它透传成 `detail` 即可,不重复造一遍 GitHub 的校验规则。

## 4. `bw-app` 改动

### 4.1 `Command::CreateProject` 新增字段

```rust
pub enum GithubOrigin {
    New { slug: String, private: bool },
    Existing { owner: String, repo: String },
}

CreateProject {
    id: ProjectId, name: String, kind: String, desc: String,
    workspace: Option<String>,        // 不变:绑定已有本地路径的旧路径,保留
    github: Option<GithubOrigin>,     // 新增
}
```

`workspace: Some(path)` 与 `github: Some(_)` 互斥(UI 层保证——Repo 卡片是新流程的唯一入口,不会同时激活两条路径);`github: None` 时行为与今天完全一致(本地 mint 或不设置)。

### 4.2 handler 分支(`bw-app/src/lib.rs:1596` 一带)

```
match (bound_local_path, github) {
    (Some(path), _)        => 今天的逻辑不变(校验 .git 存在,set_workspace)
    (None, Some(New{slug,private})) => {
        尝试 github::create_repo(slug, private, &workspace_root)
        Ok(r)  => set_workspace + set_github_remote("owner/repo") + 建 git-repo 连接器(本地路径)+ 建 github-repo 连接器(config="owner/repo")
        Err(e) => 软降级:回退到今天的 provision_workspace() 本地 mint;
                  Event::ConnectorSynced{ ok:false, detail:"GitHub 建仓失败,已改建本地仓:{e}" }
    }
    (None, Some(Existing{owner,repo})) => {
        尝试 github::clone_repo("owner/repo", dest)
        Ok(r)  => 同上,记两个连接器
        Err(e) => 不做本地 mint 兜底(硬拿一个跟用户选的仓无关的空仓出来伪装成"已接入",比"暂不挂仓库"更不诚实)。
                  workspace_path 留空(等同今天"未配置 workspaces_root"的既有状态,项目照常以 Mock 执行器运行)。
                  Event::ConnectorSynced{ ok:false, detail:"接入 {owner}/{repo} 失败:{e}" }
    }
    (None, None) => 今天的逻辑不变
}
```

两种失败都走已有的 `Event::ConnectorSynced` → `UiNote::ConnectorSynced` → 全局 toast 管线(`main.rs:187-189`),不新增 Event 变体。

### 4.3 `Command::ListGithubRepos`(新,只读)

```rust
ListGithubRepos,
```
Handler:调 `github::list_repos(30)`,结果写入 `AppState.github_repos: Vec<GithubRepoSummary>`(进程内缓存,不落库——这是 GitHub 侧实时数据的直通读,不是 BW 自己的派生 Signal,不适用"观测只追加"那套持久化规则),`emit(Event::ProjectsChanged)` 触发下一次 `build_vm` 把它带进 `Vm.github_repos`。

## 5. 数据模型 / schema

- `project` 表新增列 `github_remote TEXT NOT NULL DEFAULT ''`(空 = 未挂 GitHub)。**双守卫**(CLAUDE.md 铁律,不是可选项):`schema.sql` 里加进 `CREATE TABLE`,**并且**在 `sqlite.rs` 里加一条 `add_column_if_missing(&pool, "project", "github_remote", "TEXT NOT NULL DEFAULT ''")`——旧库不delete-and-recreate,必须能直接开旧库不崩。
- `ProjectRow`(`bw-store/src/lib.rs`)、两条 `SELECT`(`get_project`/`list_projects`)、`project_row()` 解码函数(`sqlite.rs:2096-2122`)三处同步加 `github_remote` 字段——这正是 memory 里记过的"project_id 只进了 schema,读侧三处有一处没接上"那类坑,这次显式三处一起改,不留半截。
- 新 store 方法 `set_github_remote(id, owner_repo: &str)`,与既有 `set_workspace` 同构(`UPDATE project SET github_remote=?, updated_at=?, rev=rev+1 WHERE id=?`)。
- `bw-core/src/model.rs` 新增 `pub const CONNECTOR_KIND_GITHUB_REPO: &str = "github-repo";`,和现有两个 live kind 并列写进那句文档注释。这个新 kind **不**接 `SyncConnector` 真探针——按"其余都是诚实标注未同步的引用条目"的既有默认,起步就是引用条目,持续同步是 `plan/09` 墙 C 的另一件事。
- `bw-core::model::Project` 域结构体经调研证实是**未接入的孤立代码**(没有任何地方从 `ProjectRow` construct 出它),不碰它——真正的读侧链路是 `ProjectRow → AppState.projects → app-desktop 自己的 VM`,新字段照这条真链路走,不去修一个死代码结构体制造"看起来接上了但其实没用"的假象。
- `app-desktop/src/kernel.rs`:`OpVm` 新增 `pub github_remote: String`,在 `build_vm` 里 `github_remote: row.github_remote.clone()`(`kernel.rs:933` 一带,workspace_path 同款写法)。
- 显示:`op.rs` 的 `WorkspaceConfig` 面板(`op.rs:1077-1163`,今天展示/编辑 `workspace_path` 的地方)在其旁边加一行只读的 "GitHub:owner/repo" 徽记(`github_remote` 非空时);为空时不显示任何"未挂"提示——旧项目和"新建仓失败后软降级"的项目在这一点上表现一致,不额外造一个持久化的"待办"标记(对应非目标里放弃的重试面板)。不改 `ui/vm.rs` 的 `ProjectCardVm`/项目墙卡片——`workspace_path` 今天也不上墙卡片,新字段跟随同一惯例,收紧范围。

## 6. 失败处理一览

| 场景 | 处理 |
|---|---|
| `gh` 未安装/未登录 | `create_repo`/`clone_repo`/`list_repos` 统一返回 `GithubError::NotInstalled`/`NotAuthenticated`;新建仓走软降级本地 mint;接入已有仓卡在"未挂"态;`list_repos` 失败则下拉列表为空+一条 toast |
| 新建仓重名/网络抖动 | 软降级本地 mint,project 正常建立,toast 说明 |
| 接入已有仓克隆失败 | 不兜底 mint,`workspace_path` 留空(等同"未配置执行器"),toast 说明,项目正常建立 |
| 所有情形 | `CreateProject` 本身**从不失败**(项目行永远先落库)——延续既有"创建不破"的产品哲学 |

## 7. 验证计划

按 CLAUDE.md 核心纪律:不写单元测试,E2E = 深链启动 + sqlite 读回 + `/code-review`。

1. `cargo fmt --all --check` / `cargo clippy --workspace --exclude app-desktop -- -D warnings` / 两个 wasm32 check / `guard-kernel-ui-free.sh` / `cargo check -p app-desktop` —— 门禁全过。
2. 真实一次"新建仓"全流程:临时 scratch DB + `BW_WORKSPACES` 指向临时目录,深链启动到创建流,通过界面驱动出一个真实、明显标记为测试用途的仓(如 `bw-onboarding-e2e-<timestamp>`,private)。**这一步会在用户真实 GitHub 账号上创建一个可见的新仓——执行前会在对话里明确报告仓名并等待确认,完成验证后会问是否要我 `gh repo delete` 清理。**
3. `sqlite3` 读回 `project.github_remote`、`connector` 表里 `kind='github-repo'` 的那一行,核对与真实创建的仓一致。
4. 真实一次"接入已有仓"流程:选一个仓库列表里已有的、无害的真实仓验证 `clone_repo` 路径(优先选一个已经存在、可以安全重新克隆到临时目录的仓,不新建)。
5. 故意验证失败路径:传一个不存在的 owner/repo 触发 `clone_repo` 失败,确认 toast 文案+`workspace_path` 留空、项目仍正常出现在墙上。
6. `/code-review` 过一遍新增代码。

## 8. 范围内文件改动清单

- `crates/bw-engine/src/github.rs`(新)+ `crates/bw-engine/src/lib.rs` 导出
- `crates/bw-core/src/model.rs`:`CONNECTOR_KIND_GITHUB_REPO` 常量
- `crates/bw-app/src/lib.rs`:`Command::CreateProject.github` 字段、`GithubOrigin` 类型、handler 分支、新 `Command::ListGithubRepos` + handler、`AppState.github_repos`
- `crates/bw-store/src/lib.rs` + `sqlite.rs` + `schema.sql`:`github_remote` 列(双守卫)、`ProjectRow`、两条 SELECT、`project_row()`、`set_github_remote`
- `crates/app-desktop/src/kernel.rs`:`OpVm.github_remote`、顶层 `Vm.github_repos`、`build_vm` 里的赋值
- `crates/app-desktop/src/screens/create.rs`:`Card::Repo`(新组件)、`IntentCard` 的 slug 预览/接入回显
- `crates/app-desktop/src/screens/op.rs`:`WorkspaceConfig` 面板加一行 GitHub 徽记
- `crates/app-desktop/src/main.rs`:`Create { .. }` 调用点多传 `github_repos: v.github_repos.clone()`
