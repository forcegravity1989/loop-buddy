//! 从仓里**看得出来**的那部分开发约定。
//!
//! 铺底第 1 步不起 agent,所以这里只写「读文件就能确定」的东西:构建系统是
//! 什么、怎么建怎么测、顶层目录有哪些。看不出来的一律留一句明说「还没填」,
//! **绝不猜一条命令写上去** —— 写错一条构建命令比留空更糟,人照着跑一次失败
//! 才发现是编的。
//!
//! 剩下那半(每个目录是干什么的、这个项目特有的规矩、常见坑)要 agent 读一遍
//! 仓才写得出来,那是规范铺底第 2 步的活,见 `docs/LEFTOVERS.md` V4A-4。

use std::path::Path;

/// 「怎么建、怎么跑、怎么测」那一节的正文。
pub fn build_commands(workspace: &Path) -> String {
    let mut found: Vec<String> = Vec::new();
    let has = |rel: &str| workspace.join(rel).exists();

    if has("Cargo.toml") {
        found.push(
            "**Rust / cargo**\n\n\
             ```bash\n\
             cargo build          # 建\n\
             cargo test           # 测\n\
             cargo clippy --all-targets -- -D warnings   # 静态检查\n\
             cargo fmt --all --check                      # 格式\n\
             ```"
            .into(),
        );
    }
    if has("package.json") {
        found.push(package_json_section(workspace));
    }
    if has("pyproject.toml") {
        found.push(
            "**Python / pyproject**\n\n\
             ```bash\n\
             pip install -e .     # 装\n\
             pytest               # 测(这个仓用不用 pytest 没核实,以 pyproject 里写的为准)\n\
             ```"
            .into(),
        );
    }
    if has("go.mod") {
        found.push("**Go**\n\n```bash\ngo build ./...\ngo test ./...\n```".into());
    }
    if has("Makefile") {
        found.push("**有 `Makefile`** —— `make help` 或直接读它看有哪些目标。".into());
    }

    // CI 配置是「这个项目认什么算过」的最硬证据,有就指出来。
    for ci in [
        (".github/workflows", "GitHub Actions(`.github/workflows/`)"),
        (".gitlab-ci.yml", "GitLab CI(`.gitlab-ci.yml`)"),
    ] {
        if has(ci.0) {
            found.push(format!(
                "**门禁以 CI 为准**:{} 里写的那几步就是「算不算过」的判据,\
                 提 MR 之前在本机跑一遍同样的命令。",
                ci.1
            ));
        }
    }

    if found.is_empty() {
        "(还没填 —— buddy 在这个仓里没认出构建系统:没有 `Cargo.toml`、\
         `package.json`、`pyproject.toml`、`go.mod`、`Makefile`。**不猜一条命令写在这里**,\
         请你补上,或者等规范铺底第 2 步让 agent 读一遍仓来补。)"
            .into()
    } else {
        found.join("\n\n")
    }
}

fn package_json_section(workspace: &Path) -> String {
    let scripts = std::fs::read_to_string(workspace.join("package.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("scripts").cloned())
        .and_then(|s| s.as_object().cloned())
        .map(|m| m.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if scripts.is_empty() {
        return "**Node / package.json** —— 里面没有 `scripts` 段,建与测的命令请你补上。".into();
    }
    let lines: Vec<String> = scripts.iter().map(|k| format!("npm run {k}")).collect();
    format!(
        "**Node / package.json**,`scripts` 里现有这几条:\n\n```bash\n{}\n```",
        lines.join("\n")
    )
}

/// 「目录导览」那一节的正文。只列顶层目录,**不替每个目录编一句用途** ——
/// 那要读过里面的代码才知道,是第 2 步的活。
pub fn layout(workspace: &Path) -> String {
    let mut dirs: Vec<String> = std::fs::read_dir(workspace)
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| !n.starts_with('.') && n != "target" && n != "node_modules")
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    if dirs.is_empty() {
        return "(这个仓根目录下还没有子目录。)".into();
    }
    let rows: Vec<String> = dirs
        .iter()
        .map(|d| format!("| `{d}/` | (还没填) |"))
        .collect();
    format!(
        "顶层目录是 buddy 扫出来的,**每个是干什么的还没填** —— 那要读过里面的代码\
         才写得出来,是规范铺底第 2 步的活。\n\n| 目录 | 装什么 |\n|---|---|\n{}",
        rows.join("\n")
    )
}
