//! `adopted_probe` —— 「这个仓被 buddy 接管过没有」这一问的读回证据。
//!
//! ```bash
//! cargo run -p v4-engine --example adopted_probe -- <owner/repo> [分支]
//! ```
//!
//! 接入屏点「下一步」时问的就是这一句:去远端取 `.bw/project.toml`。
//! 三种答案必须分得开,分不开就会出人命:
//!
//! | 远端回什么 | 该报 | 报错了会怎样 |
//! |---|---|---|
//! | 文件在 | 接管过,名片铺底、只读 | —— |
//! | **仓根本不在**(也是 404) | **找不到这个仓** | 地址敲错一个字母也会被放行 |
//! | 文件不在,仓在 | 还没接管过,请填 | —— |
//! | **分支不存在**(也是 404) | **没查成** | 会被当成「还没接管过」,人填一遍就盖掉仓里真正的名片 |
//!
//! 后两行是这个例子存在的理由:GitHub 三种情况都回 404。分支那种靠正文里
//! 「No commit found for the ref」认出来,仓不在那种得再问一次 `gh repo view`。要 `gh auth login` 过;
//! 不碰 claude、不碰网关,只读,重复跑没有副作用。
fn main() {
    let mut args = std::env::args().skip(1);
    let Some(slug) = args.next() else {
        eprintln!("用法:cargo run -p v4-engine --example adopted_probe -- <owner/repo> [分支]");
        std::process::exit(2);
    };
    let git_ref = args.next().unwrap_or_default();
    let Some((owner, repo)) = slug.split_once('/') else {
        eprintln!("项目地址要写成 owner/repo,收到的是「{slug}」");
        std::process::exit(2);
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("起 tokio 运行时");
    // 和接入屏点「下一步」时走的是同一套判断:先读文件,读不到再问一次
    // 「这个仓在不在」—— 仓不在也是 404,不复查就会被说成「还没接管过」。
    let answer = rt.block_on(async {
        match v4_engine::github::fetch_project_toml(owner, repo, &git_ref).await {
            Ok(None) => match v4_engine::github::probe_repo(&slug).await {
                Ok(_) => Ok(None),
                Err(e) => Err(format!("找不到这个仓:{e}")),
            },
            Ok(some) => Ok(some),
            Err(e) => Err(e.to_string()),
        }
    });
    let shown = if git_ref.trim().is_empty() {
        "(没给,按 main 查)"
    } else {
        git_ref.trim()
    };
    print!("[ADOPTED_PROBE] {slug} 分支={shown} → ");
    match answer {
        Ok(Some(raw)) => println!(
            "接管过 · 远端那份 .bw/project.toml 读到了({} 字节)",
            raw.len()
        ),
        Ok(None) => println!("还没接管过 · 仓里没有 .bw/project.toml"),
        Err(e) => println!("没查成,所以不知道接管过没有 · 原话:{e}"),
    }
}
