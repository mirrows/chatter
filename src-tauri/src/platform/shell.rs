//! 系统外壳命令（v2 / M6）：用默认程序打开文件、在文件管理器中定位。
//!
//! 从 `commands.rs` 抽离，便于按平台实现与单元测试。
//!
//! v3：Windows 端改用 `ShellExecuteW` 直接交给系统外壳。
//! 旧实现是 `cmd /C start "" <path>`，缺点有二：
//! 1. 应用是 GUI 进程，拉起 `cmd` 时会闪出一个控制台黑框；
//! 2. 每个文件都要额外启动 `cmd.exe`（几十毫秒起步），大文件/连点更明显。
//!
//! 现在没有中间进程，也没有控制台窗口，返回即交给资源管理器。

use std::path::Path;
use std::process::Command;

/// 用系统默认程序打开文件或文件夹。
pub fn open_path(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        shell_execute(path)
    }
    #[cfg(not(target_os = "windows"))]
    {
        open_command(path).spawn().map(|_| ())
    }
}

/// 构造「用默认程序打开」的命令（macOS / Linux 使用）。
pub fn open_command(path: &Path) -> Command {
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg(path);
        command
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(path);
        command
    }
    #[cfg(windows)]
    {
        // Windows 走 `ShellExecuteW`（见 `open_path`）；这里仅保留兜底实现供测试。
        let mut command = Command::new("explorer");
        command.arg(path);
        command
    }
}

/// Windows：调用 `ShellExecuteW` 打开路径（无控制台窗口、无中间进程）。
#[cfg(target_os = "windows")]
fn shell_execute(path: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let file: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: 两个宽字符串均已 NUL 结尾，其余参数按 Win32 文档传空/默认值。
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };

    // 返回值 <= 32 表示失败，数值即 Win32 错误码。
    let code = result as isize;
    if code <= 32 {
        Err(std::io::Error::from_raw_os_error(code as i32))
    } else {
        Ok(())
    }
}

/// 在文件管理器中定位文件的命令。
pub fn reveal_command(path: &Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("explorer");
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        command.arg(arg);
        command
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = Command::new("open");
        command.arg("-R").arg(path);
        command
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let mut command = Command::new("xdg-open");
        command.arg(path.parent().unwrap_or(path));
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：按平台选择默认程序（Windows 兜底为 explorer）。
    #[test]
    fn commands_use_platform_defaults() {
        let path = Path::new("C:/tmp/report.pdf");
        let open = open_command(path);
        let reveal = reveal_command(path);
        #[cfg(target_os = "windows")]
        {
            assert_eq!(open.get_program().to_string_lossy(), "explorer");
            assert_eq!(reveal.get_program().to_string_lossy(), "explorer");
        }
        #[cfg(target_os = "macos")]
        {
            assert_eq!(open.get_program().to_string_lossy(), "open");
            assert_eq!(reveal.get_program().to_string_lossy(), "open");
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            assert_eq!(open.get_program().to_string_lossy(), "xdg-open");
            assert_eq!(reveal.get_program().to_string_lossy(), "xdg-open");
        }
    }

    /// 边界：定位命令的目标是文件本身（Windows）或父目录（Unix）。
    #[test]
    fn reveal_targets_expected_path() {
        let path = Path::new("C:/tmp/report.pdf");
        let reveal = reveal_command(path);
        #[cfg(target_os = "windows")]
        {
            let arg = reveal
                .get_args()
                .next()
                .map(|a| a.to_string_lossy().to_string())
                .unwrap_or_default();
            assert!(arg.contains("report.pdf"), "arg = {arg}");
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            let arg = reveal
                .get_args()
                .next()
                .map(|a| a.to_string_lossy().to_string())
                .unwrap_or_default();
            assert!(arg.ends_with("/tmp") || arg.ends_with("tmp"), "arg = {arg}");
        }
    }

    /// 异常：不存在的路径由系统返回错误（Windows 走 ShellExecuteW 的错误码）。
    #[cfg(target_os = "windows")]
    #[test]
    fn open_missing_path_returns_error() {
        let result = open_path(Path::new("C:/chatters-no-such-file-9f3a/report.pdf"));
        assert!(result.is_err(), "missing path must not report success");
    }
}
