//! Windows GUI 父进程（`windows_subsystem = "windows"`）下，子控制台进程
//! 默认会闪出一个 cmd 窗口。探测 / git / codehub-cli 都不需要那扇窗。
//! ConPTY 路径不要走这里——伪控制台自己提供 TTY。

use std::ffi::OsStr;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn tokio_cmd(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

pub fn std_cmd(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
