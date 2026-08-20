//! `prompt_smoke` —— 「开工时给 agent 的是索引,不是正文」的读回证据。
//!
//! ```bash
//! cargo run -p bw-v4 --example prompt_smoke -- <工作目录>
//! ```
//!
//! 验四件事:
//!
//! 1. buddy 自带的技能**摊在 buddy 自己的目录**里,不在用户的仓里。
//! 2. 系统提示词里给的那条路径**真的是个文件**(不是编出来的路径)。
//! 3. 提示词里**没有技能正文** —— 拿全文的第一段去提示词里找,找不到。
//! 4. 规范索引**只列仓里真有的件**:先在一个空仓上跑一遍(一条都不列),
//!    再铺几份进去跑一遍(列出来的正好是铺进去的那几份)。
//!
//! 不碰网络、不碰 claude、不建库。重复跑不产生重复数据。

use bw_v4::app::App;
use bw_v4::model::{Issue, IssueId, IssueKind, IssueOrigin, IssueStatus, ProjectId};
use bw_v4::V4Store;
use std::path::PathBuf;
use std::sync::Arc;

fn say(step: &str, detail: &str) {
    println!("[PROMPT_SMOKE] {step}:{detail}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bw_v4::isoweek::init_local_offset();
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("用法:cargo run -p bw-v4 --example prompt_smoke -- <工作目录>");
        std::process::exit(2);
    }));
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo)?;

    let store = V4Store::open(root.join("v4.db").to_str().unwrap()).await?;
    let app = App::new(
        store,
        root.join("workspaces"),
        Arc::new(bw_engine::MockInteractiveExecutor::default()),
    )
    .with_asset_root(root.join("assets"));

    // ── 1 · 技能摊在 buddy 自己的目录 ─────────────────────────
    let skills_dir = app.ensure_skill_assets().expect("技能没摊出来");
    let packs = bw_v4::standard::skills::all();
    let on_disk = packs
        .iter()
        .filter(|p| skills_dir.join(&p.rel).is_file())
        .count();
    say(
        "步骤 1 · 技能摊在哪",
        &format!(
            "{} · 编在二进制里 {} 份,落到硬盘 {} 份",
            skills_dir.display(),
            packs.len(),
            on_disk
        ),
    );
    say(
        "步骤 1 · 用户的仓里有没有 .claude/skills/",
        &format!(
            "{}(该是 false —— buddy 不往用户仓里塞自己的技能)",
            repo.join(".claude/skills").exists()
        ),
    );

    // ── 2 · 空仓:规范索引一条都不该列 ────────────────────────
    // 逐字段拼一张活。`Issue` 故意没有 `Default` —— 一张没有项目的活不该存在,
    // 为了写个例子给领域模型开这个口子不值。
    let issue = Issue {
        id: IssueId::new(),
        project_id: ProjectId::new(),
        number: 1,
        remote_number: 0,
        title: "更新指标 + 制定本周计划".into(),
        body: String::new(),
        status: IssueStatus::Todo,
        branch: String::new(),
        pr_number: 0,
        week_of: String::new(),
        version: String::new(),
        tool: String::new(),
        kind: IssueKind::Ops,
        origin: IssueOrigin::Human,
        workflow: bw_v4::app::OPS1_WORKFLOW.to_string(),
        category: None,
        sort_order: 0.0,
        metric_key: String::new(),
        created_at: 0,
        updated_at: 0,
        settled_at: None,
    };
    let skill = bw_v4::app::skill_pointer(&skills_dir, &issue.workflow).expect("挑不到剧本");
    let empty = bw_v4::app::agent_system_prompt(&issue, &repo, Some(&skill));
    say(
        "步骤 2 · 空仓的提示词里列了几份规范件",
        &format!(
            "{}(该是 0 —— 铺底还没跑,列一个不存在的路径不如不列)",
            empty.matches("\n- `").count()
        ),
    );

    // ── 3 · 铺几份进去,再看索引 ──────────────────────────────
    let laid = [".bw/PROJECT.md", ".bw/AGENTS.md", "CLAUDE.md"];
    for rel in laid {
        let p = repo.join(rel);
        std::fs::create_dir_all(p.parent().unwrap())?;
        std::fs::write(&p, "占位\n")?;
    }
    let prompt = bw_v4::app::agent_system_prompt(&issue, &repo, Some(&skill));
    say(
        "步骤 3 · 铺了 3 份之后列了几份",
        &format!(
            "{} · 三份都在里面={}",
            prompt.matches("\n- `").count(),
            laid.iter().all(|r| prompt.contains(&format!("`{r}`")))
        ),
    );

    // ── 4 · 提示词里给的路径是真文件,且**没有正文** ────────────
    say(
        "步骤 4 · 提示词里那条技能路径",
        &format!(
            "{} · 真是个文件={}",
            skill.path,
            std::path::Path::new(&skill.path).is_file()
        ),
    );
    let body = std::fs::read_to_string(&skill.path)?;
    // 拿正文里最长的一行去提示词里找 —— 找得到就说明正文被塞进来了。
    let longest = body.lines().max_by_key(|l| l.len()).unwrap_or("");
    say(
        "步骤 4 · 正文有没有被塞进提示词",
        &format!(
            "提示词 {} 字 / 技能正文 {} 字 · 正文最长那行出现在提示词里={}(该是 false)",
            prompt.chars().count(),
            body.chars().count(),
            !longest.is_empty() && prompt.contains(longest)
        ),
    );

    println!("\n──────── 完整的系统提示词(开工时原样送给 agent)────────\n{prompt}");
    Ok(())
}
