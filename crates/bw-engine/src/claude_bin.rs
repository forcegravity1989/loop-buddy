//! Resolve the on-disk `claude` CLI the same way the Windows installer does.
//!
//! `bin\claude.exe` is copied by npm `postinstall` from an optional package —
//! a machine can have a working `%APPDATA%\npm\claude.cmd` and still lack the
//! PE. Buddy's Issue terminal is ConPTY (`CreateProcess`); a `.cmd` is not a
//! Win32 image, so spawn must go through `cmd.exe /c` (see [`crate::win_cmd`]).

use std::path::{Path, PathBuf};

/// Candidate paths in installer/app order. First existing file wins.
/// Bare names such as `claude` are not listed — those stay a PATH fallback.
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
