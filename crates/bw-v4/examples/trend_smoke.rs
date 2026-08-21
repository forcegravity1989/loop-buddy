//! `trend_smoke` —— 近 N 周走势的读回证明。
//!
//! ```bash
//! cargo run -p bw-v4 --example trend_smoke -- <仓路径> [owner/repo] [周数]
//! ```
//!
//! 证明的是设计 14 篇里「A 类 · 可回溯」那条判据成立:**这些数一个都没存过**,
//! 每次都是现问 git(和远端)算出来的,所以过去任意一周的值随时都能重算。
//!
//! 不碰本机库、不碰 claude。给了 `owner/repo` 才会连 GitHub 查合入的 PR 数与
//! 未处理 issue 数;不给就只算 git 那一条,远端那两列如实留空。
//!
//! 每一行都附上自己复算的命令 —— 数字对不上就是代码错了,不是「大概差不多」。

use bw_v4::model::{Project, ProjectId};
use bw_v4::{isoweek, trend};
use std::path::PathBuf;

/// 没采到显示「—」,不显示 0。
fn dash(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 起任何线程之前先把本机时区定住 —— 周是按本机时区算的。
    isoweek::init_local_offset();
    let args: Vec<String> = std::env::args().collect();
    let Some(ws) = args.get(1).map(PathBuf::from) else {
        eprintln!("用法:cargo run -p bw-v4 --example trend_smoke -- <仓路径> [owner/repo] [周数]");
        std::process::exit(2);
    };
    let remote = args.get(2).cloned().unwrap_or_default();
    let weeks: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);

    let project = Project {
        id: ProjectId::new(),
        slug: "trend-smoke".into(),
        name: "trend-smoke".into(),
        workspace_path: ws.display().to_string(),
        provider: if remote.is_empty() {
            String::new()
        } else {
            "github".into()
        },
        remote_host: String::new(),
        remote_path: remote.clone(),
        signal: None,
        weekly_signal: None,
        signal_derived_at: None,
        sort_order: 0.0,
        created_at: 0,
        updated_at: 0,
    };

    println!("仓:{}", ws.display());
    println!(
        "远端:{}",
        if remote.is_empty() {
            "(没给,远端那列会留空)".into()
        } else {
            remote.clone()
        }
    );
    println!("本周:{}\n", isoweek::current_week());

    let t = trend::recent_weeks(&ws, &project, weeks).await;

    println!(
        "{:<10} {:>8} {:>10} {:>14}",
        "周", "提交", "合入的PR", "未处理issue"
    );
    for p in &t.points {
        // **没采到显示「—」,不显示 0** —— 0 是一个真实的数值,「没采到」不是。
        println!(
            "{:<10} {:>8} {:>10} {:>14}",
            p.week,
            dash(p.commits),
            dash(p.merged_prs),
            dash(p.open_issues)
        );
    }
    if !t.git_note.is_empty() {
        println!("\n提交那列:{}", t.git_note);
    }
    if !t.remote_note.is_empty() {
        println!("\n远端那两列:{}", t.remote_note);
    } else if remote.is_empty() {
        println!("\n远端那两列:没给 owner/repo,这次没问远端");
    }

    // **复算命令必须是代码真正在跑的那条。** 绝不能给 `--since/--until`:
    // 那正是被修掉的错误窗口(多算下周一一整天,而且 `--since` 会提前停止
    // 遍历),照它复算出来的数和上面这张表对不上,读的人会以为代码错了、
    // 回头把已经修对的窗口改回去。
    println!("\n自己复算(每一行都该对得上):");
    for p in &t.points {
        let Ok((since, until)) = bw_v4::git::week_window(&p.week) else {
            continue;
        };
        println!(
            "  {} 提交:git -C {} log --pretty=format:'%ct %P' | awk -v s={since} -v u={until} '$1>=s && $1<u' | wc -l",
            p.week,
            ws.display()
        );
        if !remote.is_empty() {
            println!(
                "  {} 合入的PR:gh pr list -R {} --state merged --json mergedAt --limit 1000,数 mergedAt 落在 [{since}, {until}) 的条数",
                p.week, remote
            );
            println!(
                "  {} 未处理issue:gh issue list -R {} --state all --json createdAt,closedAt --limit 1000,数 createdAt < {until} 的减去 closedAt < {until} 的",
                p.week, remote
            );
        }
    }
    Ok(())
}
