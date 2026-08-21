//! `metrics_smoke` —— 指标采集的读回证明。
//!
//! ```bash
//! cargo run -p bw-v4 --example metrics_smoke -- <仓路径> [周数]
//! ```
//!
//! 证明的是设计第 10 章那条判据真的落地了:**可回溯的指标一个字都不存**,
//! 每次都是把时间窗传给脚本、现算出来的,所以过去任意一周的值随时都能重算。
//!
//! 不碰本机库、不碰 claude。它读的是 `<仓>/.bw/metrics.toml`,起的是那份文件
//! 里 `collect.run` 指着的脚本。
//!
//! **每一条都附上自己复算的命令** —— 数字对不上就是代码错了,不是「大概差不多」。

use bw_v4::app::collect;
use bw_v4::isoweek;
use bw_v4::repo::metrics_file::{self, MetricClass};
use std::path::PathBuf;

fn class_label(c: MetricClass) -> &'static str {
    match c {
        MetricClass::Retro => "A · 可回溯",
        MetricClass::PointInTime => "B · 不可回溯",
        MetricClass::Manual => "C · 手填",
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 起任何线程之前先把本机时区定住 —— 周是按本机时区算的。
    isoweek::init_local_offset();
    let args: Vec<String> = std::env::args().collect();
    let Some(ws) = args.get(1).map(PathBuf::from) else {
        eprintln!("用法:cargo run -p bw-v4 --example metrics_smoke -- <仓路径> [周数]");
        std::process::exit(2);
    };
    let weeks: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(4);

    println!("仓:{}", ws.display());
    println!("本周:{}\n", isoweek::current_week());

    // 先把定义原样摊一遍 —— 采出来的数对不对,首先取决于定义写没写对。
    let file = match metrics_file::read(&ws) {
        Err(e) => {
            println!("指标文件读不了:{e}");
            println!("(整块指标该显示灰 + 这句原话,而不是「没有指标」)");
            std::process::exit(1);
        }
        Ok(None) => {
            println!("这个仓还没有 .bw/metrics.toml —— 指标是空的(不是 0)");
            return Ok(());
        }
        Ok(Some(f)) => f,
    };
    println!(
        "schema_version = {} · 共 {} 条",
        file.schema_version,
        file.all().len()
    );
    for d in file.all() {
        println!(
            "  {:<24} {}  目标={}  run={:?}",
            d.name,
            class_label(d.collect.class()),
            if d.target.is_empty() {
                "(未设)"
            } else {
                &d.target
            },
            d.collect.run
        );
    }

    println!("\n采一遍({weeks} 周):\n");
    let readouts = collect::collect_all(&ws, weeks).await?;
    for r in &readouts {
        println!("── {} · {}", r.name, class_label(r.class));
        if !r.error.is_empty() {
            // **没采到就说没采到,不打一个 0。**
            println!("   没采到:{}", r.error);
            continue;
        }
        if r.points.is_empty() {
            println!("   手填 —— 这一类不由 buddy 采");
            continue;
        }
        let cells: Vec<String> = r
            .points
            .iter()
            .map(|p| {
                // **没采到显示「—」,不显示 0。**
                format!(
                    "{}={}",
                    p.week,
                    p.value.clone().unwrap_or_else(|| "—".into())
                )
            })
            .collect();
        println!("   {}", cells.join("  "));
        println!(
            "   现值:{}",
            r.current.clone().unwrap_or_else(|| "—".into())
        );
    }

    println!("\n自己复算(把 buddy 传的窗口原样敲一遍,数字应该一模一样):");
    for d in file.all() {
        if d.collect.run.is_empty() {
            continue;
        }
        println!(
            "  {}:cd {} && {} --since <首周周一> --until <末周的下周一> --granularity week",
            d.name,
            ws.display(),
            d.collect.run.join(" ")
        );
    }
    Ok(())
}
