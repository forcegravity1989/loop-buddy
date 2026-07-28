//! **build_aihot_fixture — 从真实日常库生成随仓发布的 aihot 样板间 DB。**
//!
//! `examples/aihot/bw-aihot.db` 不是手工捏的、也不是 mock 造的:它是**真实
//! 日常库的一份裁剪副本**。这个 example 就是那把裁剪刀,留在仓里是为了让
//! "样板间是怎么来的" 可复核、可重跑,而不是一次性手改 DB 留下无法追溯的产物。
//!
//! 它做四件事(全部走真实 `Command` 层,不裸改 SQL):
//!
//! 1. **复制**源库到输出路径——源库(用户日常库)只读,绝不就地改。
//! 2. **裁剪**:除「aihot 日报」外的项目全部 `Command::DeleteProject`。这是
//!    隐私边界:日常库里还有用户自己的其它项目,不进公开仓。
//! 3. **接仓**:清掉只在原作者机器上成立的 `workspace_path`,记上真实公开仓
//!    `github_remote`。样板间因此诚实地处在"有仓、还没克隆到本地"的状态——
//!    使用者点一次「克隆仓库」(`bw_engine::github::clone_repo`,产品已有能力)
//!    就能把带完整 32 次提交历史的真实工作区落到本地。
//! 4. **确认技能库自足**:对仓内 vendor 的两个官方库各跑一次
//!    `Command::ImportSkillLibrary`。因为幂等键是 `(name, official_library)`,
//!    正常结果应是 **imported=0 / skipped=全部**——这恰好**证明**仓内 vendor
//!    的副本与日常库里那批技能同名同源,不是另一批东西。
//!
//! 最后 `VACUUM`:SQLite 删行只是标记空闲页,原始字节仍留在文件里。样板间要
//! 公开发布,被删项目的 Issue 标题/产物路径必须真正从文件中消失,不能靠
//! "查询查不到" 糊弄过去。
//!
//! 用法(默认源库=本机日常库,默认输出=仓内样板间):
//! ```text
//! cargo run -p bw-app --example build_aihot_fixture
//! cargo run -p bw-app --example build_aihot_fixture -- <源库> <输出库>
//! ```

use bw_app::{App, Command, Event};
use bw_core::model::CONNECTOR_KIND_GIT_REPO;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

const KEEP_PROJECT: &str = "aihot 日报";
/// 样板间项目的真实公开仓。样板间**不再夹带一份工作区文件副本**:那份副本
/// 没有 git 历史(嵌套 `.git` 会变 gitlink),而且真仓每多一次提交它就旧一分。
/// 记 `github_remote` 才是单一事实源,也才是产品自己的形态——plan/13
/// 「GitHub 主体化」整套(issue=GitHub issue、验收=merge)都建在这个字段上,
/// 之前它是空的,等于样板间在展示产品的降级形态。
const GITHUB_REMOTE: &str = "forcegravity1989/aihot";
const DEFAULT_OUT: &str = "examples/aihot/bw-aihot.db";
/// 仓内 vendor 的官方库(见 `examples/skill-libraries/README.md`)。用
/// `CARGO_MANIFEST_DIR` 解析,不再是 `/Users/<某人>/.claude/plugins/cache/...`
/// 这种只在一台机器上成立的绝对路径——那正是"别人 clone 下来什么都没有"的根因。
const VENDORED_LIBS: [(&str, &str); 2] = [
    (
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/skill-libraries/mattpocock-skills/skills"
        ),
        "mattpocock-skills",
    ),
    (
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/skill-libraries/superpowers/skills"
        ),
        "superpowers",
    ),
];

fn default_source_db() -> String {
    std::env::var("HOME")
        .map(|h| format!("{h}/Library/Application Support/BuildersWorkbench/workbench.db"))
        .unwrap_or_else(|_| "workbench.db".into())
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let src = args.next().unwrap_or_else(default_source_db);
    let out = args.next().unwrap_or_else(|| DEFAULT_OUT.to_string());

    if !std::path::Path::new(&src).is_file() {
        eprintln!("源库不存在:{src}");
        std::process::exit(1);
    }
    println!("源库(只读):{src}");
    println!("输出:{out}");

    // 1 · 复制。源库是用户的真实日常库,全程只读。
    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent).expect("create output dir");
    }
    let _ = std::fs::remove_file(&out);
    // SQLite 的 -wal/-shm 若残留会把旧事务带进新副本,一并清掉。
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{out}{suffix}"));
    }
    std::fs::copy(&src, &out).expect("copy source db");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&out).await.expect("open output db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.expect("boot");
    let mut sub = app.subscribe();

    // 2 · 裁剪:只留 aihot。
    let projects = store.list_projects().await.expect("list_projects");
    let keep = projects
        .iter()
        .find(|p| p.name == KEEP_PROJECT)
        .unwrap_or_else(|| panic!("源库里找不到项目「{KEEP_PROJECT}」:{src}"));
    let keep_id = keep.id;
    println!("\n── 裁剪 ──");
    for p in projects.iter().filter(|p| p.id != keep_id) {
        app.dispatch(Command::DeleteProject(p.id))
            .await
            .unwrap_or_else(|e| panic!("删除项目「{}」失败:{e}", p.name));
        println!("  已删除:{}", p.name);
    }

    // 3 · 接上真实公开仓,并清掉那条只在原作者机器上成立的本地路径。
    //     顺序要紧:先清 workspace_path 再记 github_remote —— 让样板间的状态
    //     诚实地是"有仓、还没克隆到本地",而不是指着一个别人机器上不存在的目录。
    app.dispatch(Command::OpenProject(keep_id))
        .await
        .expect("open kept project");
    app.dispatch(Command::SetWorkspace {
        path: String::new(),
        allow_commands: false,
    })
    .await
    .expect("clear the origin machine's local workspace path");
    // 紧跟着清 workspace_path:同一个漏洞的第二半。样板间的诚实状态是
    // "有仓、还没克隆到本地"——既然没有本地工作区,就不该留一条"本地代码仓"
    // connector。留着它(config 是原作者机器上的绝对路径,例如
    // `/Users/gravity/Library/Application Support/BuildersWorkbench/workspaces/
    // aihot-b7971eca`)和留一个别人机器上不存在的 workspace_path 是同一类问题,
    // 只是更安静——UI 上没人会看见这条路径,但 `sqlite3` 一读就穿帮。
    // `github-repo` connector(`config = owner/repo`)不碰:它在任何机器上都
    // 成立,必须保留。
    //
    // 这一刀没有放进上面的 `Command::SetWorkspace("")`:那是"临时清空工作区"
    // 这个看起来无害的产品动作,给它加上"顺带删 connector 及其 last_sync 历史"
    // 的副作用会是静默的、意料之外的。样板间发布前的擦除是裁剪刀自己的有意
    // 动作(与本文件末尾的 `VACUUM` 同类),该留在裁剪刀里,不该混进产品命令
    // 的语义。
    let connectors = store.list_connectors().await.expect("list_connectors");
    for c in connectors
        .iter()
        .filter(|c| c.project_id == Some(keep_id) && c.kind == CONNECTOR_KIND_GIT_REPO)
    {
        println!("  删除本地仓 connector:{}(config={})", c.name, c.config);
        store
            .delete_connector(c.id)
            .await
            .unwrap_or_else(|e| panic!("删除本地仓 connector 失败:{e}"));
    }
    store
        .set_remote(keep_id, "github.com", GITHUB_REMOTE)
        .await
        .expect("record the real public repo");
    println!("\n── 代码仓 ──");
    println!("  github_remote -> {GITHUB_REMOTE}");
    println!("  workspace_path -> (空:等使用者自己克隆,不假装本地有)");

    // 4 · 用仓内 vendor 的副本各导一次,验证自足。
    println!("\n── 技能库(仓内 vendor 副本)──");
    for (root, library) in VENDORED_LIBS {
        if !std::path::Path::new(root).is_dir() {
            eprintln!("  vendor 目录缺失:{root}");
            std::process::exit(1);
        }
        app.dispatch(Command::ImportSkillLibrary {
            root_path: root.to_string(),
            official_library: library.to_string(),
            project_id: None,
        })
        .await
        .unwrap_or_else(|e| panic!("导入 {library} 失败:{e}"));
        let (imported, skipped) = drain_import(&mut sub);
        // imported>0 不是错误,只是说明源库里本来没有(或不全)这批技能——
        // 如实打印,不断言,让运行者自己看见真实差异。
        println!("  {library}: imported={imported} skipped={skipped}");
    }

    // 释放连接后再 VACUUM:删掉的行必须真正离开文件。
    drop(app);
    drop(store);
    vacuum(&out).await;

    readback(&out).await;
}

fn drain_import(sub: &mut tokio::sync::broadcast::Receiver<Event>) -> (u32, u32) {
    loop {
        match sub.try_recv() {
            Ok(Event::SkillLibraryImported {
                imported, skipped, ..
            }) => return (imported, skipped),
            Ok(_) => continue,
            Err(e) => panic!("没等到 Event::SkillLibraryImported:{e}"),
        }
    }
}

/// `VACUUM` 重建整个文件,把被删行占据的空闲页真正回收——公开发布前的隐私底线,
/// 不是体积优化的顺手之举。
async fn vacuum(db: &str) {
    use sqlx::Connection;
    let mut conn = sqlx::SqliteConnection::connect(&format!("sqlite:{db}"))
        .await
        .expect("connect for vacuum");
    sqlx::query("VACUUM")
        .execute(&mut conn)
        .await
        .expect("vacuum");
    conn.close().await.expect("close after vacuum");
}

/// 读回为证:每个数字都从产出的文件里重新查出来,不复用上面流程中的内存值。
async fn readback(db: &str) {
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(db).await.expect("reopen fixture"));
    let projects = store.list_projects().await.expect("list_projects");
    let skills = store.list_skills().await.expect("list_skills");
    let agents = store.list_agents().await.expect("list_agents");

    println!("\n╔══ 样板间读回 ══╗");
    println!("  项目:{}", projects.len());
    for p in &projects {
        let issues = store
            .list_issues(p.id, None, None)
            .await
            .expect("list_issues");
        println!(
            "    ·{} · issue={} · github_remote={} · workspace_path={}",
            p.name,
            issues.len(),
            if p.remote_path.trim().is_empty() {
                "(空)"
            } else {
                &p.remote_path
            },
            if p.workspace_path.trim().is_empty() {
                "(空 · 待克隆)"
            } else {
                &p.workspace_path
            }
        );
    }
    let mut by_lib: std::collections::BTreeMap<String, (u32, u32)> = Default::default();
    for s in &skills {
        let lib = match &s.source {
            bw_core::model::HubSource::Official { official_library } => official_library.clone(),
            other => format!("{other:?}"),
        };
        let e = by_lib.entry(lib).or_insert((0, 0));
        e.0 += 1;
        if !s.content.trim().is_empty() {
            e.1 += 1;
        }
    }
    println!("  技能:{}(按来源库 · 总数/带正文)", skills.len());
    for (lib, (n, with_body)) in &by_lib {
        println!("    ·{lib}: {n} / {with_body}");
    }
    println!("  智能体:{}", agents.len());
    let size = std::fs::metadata(db).map(|m| m.len()).unwrap_or(0);
    println!("  文件大小:{:.1} KB", size as f64 / 1024.0);
    println!("╚════════════════╝");
    println!("\n独立核验:sqlite3 {db} \"SELECT name, workspace_path FROM project;\"");
}
