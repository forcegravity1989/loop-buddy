//! Windows GUI 父进程（`windows_subsystem = "windows"`）下，子控制台进程
//! 默认会闪出一个 cmd 窗口。探测 / git / codehub-cli 都不需要那扇窗。
//! ConPTY 路径不要走这里——伪控制台自己提供 TTY。
//!
//! `.cmd` / `.bat` 不是 PE。`CreateProcess` 直接打会 `ERROR_BAD_EXE_FORMAT`。
//! 本模块的 tokio/std 入口在 Windows 上改走 `cmd.exe /c`。ConPTY 用
//! [`is_windows_script`] 自己包一层，不要套 `CREATE_NO_WINDOW`。

use std::ffi::OsStr;
use std::path::Path;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// True when `program` is a Windows batch script (not a PE image).
pub fn is_windows_script(program: impl AsRef<OsStr>) -> bool {
    Path::new(program.as_ref())
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

pub fn tokio_cmd(program: impl AsRef<OsStr>) -> tokio::process::Command {
    #[cfg(windows)]
    {
        if is_windows_script(&program) {
            let mut cmd = tokio::process::Command::new("cmd.exe");
            cmd.arg("/c");
            cmd.arg(program);
            cmd.creation_flags(CREATE_NO_WINDOW);
            return cmd;
        }
    }
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn std_cmd(program: impl AsRef<OsStr>) -> std::process::Command {
    #[cfg(windows)]
    {
        if is_windows_script(&program) {
            use std::os::windows::process::CommandExt;
            let mut cmd = std::process::Command::new("cmd.exe");
            cmd.arg("/c");
            cmd.arg(program);
            cmd.creation_flags(CREATE_NO_WINDOW);
            return cmd;
        }
    }
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_ext_is_cmd_or_bat() {
        assert!(is_windows_script(
            r"C:\Users\x\AppData\Roaming\npm\claude.cmd"
        ));
        assert!(is_windows_script("run.BAT"));
        assert!(!is_windows_script(
            r"C:\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe"
        ));
        assert!(!is_windows_script("claude"));
    }
}
