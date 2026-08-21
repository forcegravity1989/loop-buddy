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
pub fn which_on_path(exe: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(exe))
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
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
