//! `trend_smoke` —— 近 N 周走势的读回证明。
//!
//! ```bash
//! cargo run -p bw-v4 --example trend_smoke -- <仓路径> [owner/repo] [周数]
//! ```
//!
//! 证明的是设计 14 篇里「A 类 · 可回溯」那条判据成立:**这些数一个都没存过**,
//! 每次都是现问 git(和远端)算出来的,所以过去任意一周的值随时都能重算。
//!
//! 不碰本机库、不碰 claude。给了 `owner/repo` 才会连一次 GitHub 查合入的 PR
//! 数;不给就只算 git 那两条,远端那列如实留空。
//!
//! 每一行都附上自己复算的命令 —— 数字对不上就是代码错了,不是「大概差不多」。

use bw_v4::model::{Project, ProjectId};
use bw_v4::{isoweek, trend};
use std::path::PathBuf;

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
        "{:<10} {:>8} {:>8} {:>10}",
        "周", "提交", "合入", "合入的PR"
    );
    for p in &t.points {
        println!(
            "{:<10} {:>8} {:>8} {:>10}",
            p.week,
            p.commits,
            p.merges,
            // **没采到显示「—」,不显示 0** —— 0 是一个真实的数值,「没采到」不是。
            p.merged_prs
                .map(|n| n.to_string())
                .unwrap_or_else(|| "—".into())
        );
    }
    if !t.remote_note.is_empty() {
        println!("\n远端那列:{}", t.remote_note);
    } else if !t.remote_attempted {
        println!("\n远端那列:没给 owner/repo,这次没问远端");
    }

    // 复算命令用 `rev-list --count`,**不要用 `log --pretty=format:%H | wc -l`**
    // —— `format:%H` 最后一行不带换行符,`wc -l` 每次都少数一条。真踩过:
    // 手工复算出来的数比代码少 1,差点当成代码的 off-by-one 去改。
    println!("\n自己复算(每一行都该对得上):");
    for p in &t.points {
        let Some((mon, next_mon)) = isoweek::week_bounds(&p.week) else {
            continue;
        };
        let sun = next_mon - time::Duration::days(1);
        println!(
            "  {} 提交:git -C {} rev-list --count HEAD --since={} --until={}",
            p.week,
            ws.display(),
            mon,
            next_mon
        );
        println!(
            "  {} 合入:git -C {} rev-list --count --merges HEAD --since={} --until={}",
            p.week,
            ws.display(),
            mon,
            next_mon
        );
        if !remote.is_empty() {
            println!(
                "  {} 合入的PR:gh api -X GET search/issues -f q=\"repo:{} is:pr is:merged merged:{}..{}\" --jq .total_count",
                p.week, remote, mon, sun
            );
        }
    }
    Ok(())
}
