//! 系统外壳命令（v2 / M6）：用默认程序打开文件、在文件管理器中定位。
//!
//! 从 `commands.rs` 抽离，便于按平台实现与单元测试（只验证命令行构造，不实际拉起进程）。

use std::path::Path;
use std::process::Command;

/// 用系统默认程序打开文件的命令。
pub fn open_command(path: &Path) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(path);
        command
    }
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

    /// 正常路径：按平台选择默认程序。
    #[test]
    fn commands_use_platform_defaults() {
        let path = Path::new("C:/tmp/report.pdf");
        let open = open_command(path);
        let reveal = reveal_command(path);
        #[cfg(target_os = "windows")]
        {
            assert_eq!(open.get_program().to_string_lossy(), "cmd");
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
}
