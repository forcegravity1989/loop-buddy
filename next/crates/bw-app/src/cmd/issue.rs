//! 活聚合的命令用例(design-s5-hexpanel.md §4.2 `cmd/issue.rs`)。**四条**
//! 在本片范围内:①活状态的显式转移——「完成永远由人点」这条铁律的编排
//! 层落点;②活的所属阶段(切片五E 主控补充要求,见 [`set_stage`] 文
//! 档);③④开工/取消([`run_issue`]/[`cancel_run`],next 切片七C,
//! design-s7-real-project.md §1.1)。
//!
//! **`run_issue`/`cancel_run` 不是 `bw_app::Command` 总线上的两条命令,是
//! 编排层的两个自由函数**——切片五C 就把这条判断写清楚了(见下方历史注
//! 记):它们要驱动 `run::RunManager`,而 `RunManager` 是独立于 `App` 的
//! 另一个存储连接持有者;并进 `Command` 总线意味着 `App` 要么反过来持有
//! `RunManager`、要么在两者之间转发,两条路都会碰切片四钉下的「两个各
//! 自独立的存储连接持有者」边界。壳(切片七D)与 `issue_kickoff` 指挥器
//! 都直接调这两个函数,不经 `App::dispatch`——同一段代码,两个调用方,
//! 不是两份平行实现。
//!
//! （切片五C 历史注记,原文如下,现状见上:「`RunIssue`/`CancelRun`(设
//! 计稿草案里同属活聚合)不在这里……设计稿原文自己也说这份 `Command` 草
//! 案『实施时按真实需要收敛』——这就是一次收敛」。）

use std::path::Path;

use bw_core::{IssueId, IssueStatus, RunId, StageKind};
use bw_store::{IssueStore, SqliteStore};

use crate::run::{RunManager, StartRun};
use crate::AppError;

/// 「完成永远由人点」在编排层的落点(design-s4-runmanager.md §3.6):
/// ①读当前状态 ②查 `bw_core::IssueStatus::can_transition_to` ③合法才落
/// 到 `bw_store::IssueStore::transition_issue_status` 的比较并置写。
///
/// 从 `App::transition_issue`(切片四B)原样搬过来——这是「按聚合拆」的
/// 第一个真实搬迁:此前这条用例的正文直接写在 `App` 结构体的方法体里,
/// 现在挪进它所属的聚合模块,`App::transition_issue` 变成一行转发
/// (`lib.rs`「只做路由」)。
pub async fn transition(store: &SqliteStore, id: IssueId, to: IssueStatus) -> Result<(), AppError> {
    let Some(row) = store.get_issue(id).await? else {
        return Err(AppError::IssueNotFound(id));
    };
    if !row.status.can_transition_to(to) {
        return Err(AppError::IllegalTransition {
            from: row.status,
            to,
        });
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let wrote = store
        .transition_issue_status(id, row.status, to, now)
        .await?;
    if !wrote {
        return Err(AppError::TransitionRaced {
            from: row.status,
            to,
        });
    }
    Ok(())
}

/// `issue.stage` 的第一条生产写入方(切片五E 主控补充要求,plan/23 §10
/// 第 16 条清偿)——`Command::SetIssueStage` 的落地。切片五B 建了这一列
/// (design §2.1),切片五C/D 建了读它的两处消费者(五角色责任卡的活数
/// 分组、六段/待人处理的推导),但没有任何命令能写它;`hex_readback` 只
/// 能绕过 store 直接 `UPDATE issue SET stage = ?` 造数,每一行都标注非
/// 生产路径产出。这条命令补上正经的出生路径:①活必须存在②纯机械写,
/// 不判断「这件活该不该归到这个阶段」这类业务合法性——同
/// `cmd::project::set_active_stage`「合法性住用例层、存储层只管写」的既
/// 有分工,这里合法性检查只有「活存在」这一条,因为「一件活归哪个阶
/// 段」不像状态机转移那样有一张合法转移表,人显式指哪一阶段就是哪一阶
/// 段(包括显式退回 `None`=未归类)。
pub async fn set_stage(
    store: &SqliteStore,
    id: IssueId,
    stage: Option<StageKind>,
) -> Result<(), AppError> {
    if store.get_issue(id).await?.is_none() {
        return Err(AppError::IssueNotFound(id));
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    store.set_issue_stage(id, stage, now).await?;
    Ok(())
}

/// 环境变量 `BW_CLAUDE_MAX_BUDGET_USD`——真执行器的预算上限配置(v1 旧约
/// 定原样沿用,design §1.2「预算上限从哪来」)。**没配就是不封顶**,不悄
/// 悄给一个 BW 自己拍的数字;解析失败(不是合法的浮点数)同样按「没配」
/// 处理——如实不拦这次开工,错的是环境变量的内容,不该让开工连带失败。
fn budget_from_env() -> Option<f64> {
    std::env::var("BW_CLAUDE_MAX_BUDGET_USD")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
}

/// 主检出根的解析(design §1.2 签名注释「`workspaces_root`:深链变量
/// `BW_WORKSPACES` 的第一个真实消费者」,plan/23 §10 第 20 条清偿)。
/// `project.root_path` 绝对路径 → 原样用(真实项目导入进来的通常就是这
/// 种——旧工程的 `workspace_path` 本来就是绝对路径,§2.2①);相对路径 →
/// 接在 `workspaces_root` 后面解析成绝对路径,与旧壳「auto-provisioned
/// project repos live under `BW_WORKSPACES`」的既有约定同一个精神(新工
/// 程没有自动开仓这条路,但一个相对路径不该悄悄按进程当前工作目录解
/// 析,那样"同一个项目在哪个目录下启动壳"都会算出不同的路径)。
fn resolve_main_workspace(root_path: &str, workspaces_root: &Path) -> std::path::PathBuf {
    let p = Path::new(root_path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspaces_root.join(p)
    }
}

/// 开工用例(design-s7-real-project.md §1.1/§1.2)——编排层的一个自由函
/// 数,不是 `bw_app::Command` 总线上的一条命令(见本文件顶部模块文档)。
/// 六步里前两步(读活/项目、造工作树)在这里;③④⑤⑥全部转给
/// [`RunManager::start`](design §3.4 的既有五步),这个函数自己不重复那
/// 一层判断,只负责把它需要的两样东西(工作区路径、分支名)真的准备
/// 好、纯机械地转交。
///
/// **每一步失败都如实停在原地,不重试、不换路径**(design §1.2):
/// - 活/项目不存在 → [`AppError::IssueNotFound`]/[`AppError::ProjectNotFound`]
/// - 项目没有配本地检出根 → [`AppError::ProjectNoCheckoutRoot`](一条运行
///   行都不会落库)
/// - 造工作树失败 → [`AppError::WorktreeProvision`](同样不会有运行行落
///   库——工作区是 `RunManager::start` 的入参,没造出来就轮不到它)
/// - `RunManager::start` 的三种如实拒绝(已经有交付运行在跑/工作区被占/
///   连接器零条或多条)→ [`AppError::Run`] 透传
///
/// **注入块本片放空**(design §1.2「本片放空,并如实标注」):复利链(蒸
/// 馏出的技能正文、标准文件、目录块)在新工程里还没有任何一片建过,注
/// 入块里今天没有真实内容可放;造一段占位文本塞进去,就是给上游会话喂
/// BW 自己编的东西。空注入块比假内容好。
pub async fn run_issue(
    store: &SqliteStore,
    manager: &RunManager,
    workspaces_root: &Path,
    issue: IssueId,
    connector_name: Option<String>,
) -> Result<RunId, AppError> {
    // ① 读活 → 读它所属项目 → 项目没有配本地检出根 → 如实拒绝。
    let issue_row = store
        .get_issue(issue)
        .await?
        .ok_or(AppError::IssueNotFound(issue))?;
    let project = store
        .get_project(issue_row.project_id)
        .await?
        .ok_or(AppError::ProjectNotFound(issue_row.project_id))?;
    if project.root_path.trim().is_empty() {
        return Err(AppError::ProjectNoCheckoutRoot(project.id));
    }
    let main_workspace = resolve_main_workspace(&project.root_path, workspaces_root);

    // ② 造工作树:按活编号在主检出的兄弟目录造一棵、切到这件活自己的分
    // 支——`bw_workspace::provision::provision_issue_worktree` 函数体不
    // 碰(design §1.1「工作区供给谁供给」)。活编号存的是 `i64`(调用方分
    // 配,恒正——`IssueStore::create_issue` 文档),这里的转换失败只可能
    // 是历史脏数据(负数/超出 u32 范围),按同一条「造工作树失败,如实回
    // 原文」处理,不是一个需要单独区分的第五种失败形态。
    let issue_number = u32::try_from(issue_row.number).map_err(|_| {
        AppError::WorktreeProvision(format!(
            "活编号 {} 超出工作树分支命名能表示的范围(u32)",
            issue_row.number
        ))
    })?;
    let workspace =
        bw_workspace::provision::provision_issue_worktree(&main_workspace, issue_number)
            .await
            .map_err(|e| AppError::WorktreeProvision(e.to_string()))?;
    let branch = bw_workspace::provision::issue_branch(issue_number);

    // ③④⑤⑥ 交给运行管理器开工——design §3.4 的既有五步全在
    // `RunManager::start` 内部,这里只递交它要的两样东西。
    let run = manager
        .start(StartRun {
            issue,
            connector_name,
            workspace,
            branch,
            inject: Vec::new(),
            budget_usd: budget_from_env(),
        })
        .await?;
    Ok(run)
}

/// 取消用例(design §1.1/§3.5②)——薄转发,合法性/幂等语义全部住在
/// [`RunManager::cancel`](design §3.5②「取消是幂等的,重复点不改任何字
/// 段」)。这里不重复判断,函数体只做「透传 + 错误类型转换」这一件事。
pub async fn cancel_run(manager: &RunManager, run: RunId) -> Result<(), AppError> {
    manager.cancel(run).await?;
    Ok(())
}
