//! Resolve the on-disk `claude` CLI the same way the Windows installer does.
//!
//! `bin\claude.exe` is copied by npm `postinstall` from an optional package —
//! a machine can have a working `%APPDATA%\npm\claude.cmd` and still lack the
//! PE. Buddy's Issue terminal is ConPTY (`CreateProcess`); a `.cmd` is not a
//! Win32 image, so spawn must go through `cmd.exe /c` (see [`crate::win_cmd`]).

use std::path::{Path, PathBuf};

/// 在 `PATH` 里找一个可执行文件,找到就给完整路径。
///
/// **只看文件在不在,不起子进程** —— 探活条是开屏就要出来的,起几个子进程去
/// 问版本号会把启动拖住;而且「装没装」这个问题本身用不着跑它。跑得起来、
/// 登录态对不对是另一个问题,探不了就说探不了,不拿「文件在」冒充「能用」。
///
/// **Windows 上要按 `PATHEXT` 接扩展名**(2026-08-21 修):那边的程序落在盘上
/// 叫 `gh.exe`、`codehub-cli.exe`、`claude.cmd`,只按裸名字 `dir.join("gh")`
/// 永远找不到。这条以前只被拿来找 `claude`,而且是候选链最后一条兜底 ——
/// Windows 上前面那两条 npm 路径早命中了,轮不到它,所以一直没暴露;新壳的
/// 环境条是第一个拿它去找 gh / cursor-agent / codehub-cli 的地方,不修就是
/// 三盏恒亮的假红灯。**假的红灯比没有灯更坏。**
pub fn which_on_path(exe: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let names = exe_file_names(exe);
    for dir in std::env::split_paths(&path) {
        for name in &names {
            let cand = dir.join(name);
            if cand.is_file() {
                return Some(cand.display().to_string());
            }
        }
    }
    None
}

/// 一个命令名在本平台上可能的文件名,按查找顺序。
///
/// Windows:`PATHEXT` 里的每个扩展名各接一遍,**接了扩展名的排在裸名字前面**
/// —— `CreateProcess` 认的是带扩展名那个,先撞上一个没有扩展名的同名文件再把
/// 它的路径交出去,下游 spawn 会失败。已经带着 `PATHEXT` 里某个扩展名的(比如
/// 显式传进来的 `claude.exe`)照原样找,不再往后接第二遍。
#[cfg(windows)]
fn exe_file_names(exe: &str) -> Vec<String> {
    let raw = std::env::var("PATHEXT").unwrap_or_default();
    let raw = if raw.trim().is_empty() {
        ".COM;.EXE;.BAT;.CMD".to_string()
    } else {
        raw
    };
    let exts: Vec<&str> = raw
        .split(';')
        .map(str::trim)
        .filter(|e| !e.is_empty())
        .collect();
    let upper = exe.to_ascii_uppercase();
    if exts
        .iter()
        .any(|e| upper.len() > e.len() && upper.ends_with(&e.to_ascii_uppercase()))
    {
        return vec![exe.to_string()];
    }
    let mut names: Vec<String> = exts.iter().map(|e| format!("{exe}{e}")).collect();
    names.push(exe.to_string());
    names
}

/// macOS / Linux:可执行位就是可执行位,没有扩展名这回事。
#[cfg(not(windows))]
fn exe_file_names(exe: &str) -> Vec<String> {
    vec![exe.to_string()]
}

/// Candidate paths in installer/app order. First existing file wins.
///
/// **`PATH` 必须在列表里**(2026-08-20 修):在这条之前,列表里只有 Windows 那
/// 两个 npm 路径,而它们都从 `APPDATA` 拼出来 —— macOS/Linux 上 `APPDATA` 根本
/// 不存在,于是候选列表是空的,`resolve_claude_binary` 恒返回 `None`。起进程那
/// 条路有「退回裸名字 claude、交给系统按 PATH 找」兜底,所以一直能跑;**探活那
/// 条路没有兜底**,于是项目墙的环境条对着一台装好了 claude 的 mac 说「本机路径
/// 里找不到 claude」。假的红灯比没有灯更坏。
pub fn claude_binary_candidates(explicit: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    push_unique(&mut out, explicit.map(str::trim).filter(|s| !s.is_empty()));
    if let Ok(env) = std::env::var("BW_CLAUDE_BIN") {
        push_unique(&mut out, Some(env.trim()).filter(|s| !s.is_empty()));
    }
    if let Some(p) = npm_claude_exe() {
        push_owned(&mut out, p.to_string_lossy().into_owned());
    }
    if let Some(p) = npm_claude_cmd() {
        push_owned(&mut out, p.to_string_lossy().into_owned());
    }
    // 放最后:装过 Windows 那个 npm 包的机器仍然优先用它,其余机器靠这一条。
    push_unique(&mut out, which_on_path("claude").as_deref());
    out
}

/// First candidate that is a real file. `None` → caller falls back to `"claude"`.
pub fn resolve_claude_binary(explicit: Option<&str>) -> Option<String> {
    claude_binary_candidates(explicit)
        .into_iter()
        .find(|c| Path::new(c).is_file())
}

fn npm_root() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|a| a.join("npm"))
}

fn npm_claude_exe() -> Option<PathBuf> {
    Some(
        npm_root()?
            .join("node_modules")
            .join("@anthropic-ai")
            .join("claude-code")
            .join("bin")
            .join("claude.exe"),
    )
}

fn npm_claude_cmd() -> Option<PathBuf> {
    Some(npm_root()?.join("claude.cmd"))
}

fn push_unique(out: &mut Vec<String>, value: Option<&str>) {
    if let Some(v) = value {
        push_owned(out, v.to_string());
    }
}

fn push_owned(out: &mut Vec<String>, value: String) {
    if !out.iter().any(|x| x == &value) {
        out.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_put_explicit_first() {
        let got = claude_binary_candidates(Some(r"D:\tools\claude.exe"));
        assert_eq!(
            got.first().map(String::as_str),
            Some(r"D:\tools\claude.exe")
        );
    }

    #[test]
    fn candidates_skip_blank_explicit() {
        let got = claude_binary_candidates(Some("   "));
        assert!(got.iter().all(|c| !c.trim().is_empty()));
    }
}
