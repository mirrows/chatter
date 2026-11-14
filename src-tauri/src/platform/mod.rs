//! 平台能力层（v2 / M6）：把平台相关的差异集中到一处。
//!
//! 目的：让业务模块（config / commands / transfer）不再直接依赖 Windows 语义，
//! 为 macOS / Linux / Android 移植做准备。

pub mod paths;
pub mod shell;

#[cfg(target_os = "android")]
pub mod android;

/// 判断文件名是否可在当前平台安全落盘。
///
/// 通用规则（rules.md R5.2）：
/// - 空名、`.`、`..` 一律拒绝；
/// - 含路径分隔符（`/`、`\`）或 `..` 一律拒绝（防路径穿越）；
/// - 含控制字符一律拒绝。
///
/// 平台规则：
/// - Windows：额外拒绝 `: < > " | ? *`，以及结尾的 `.` / 空格（会被系统截断）；
/// - macOS / Linux：`:` 等字符合法，不做额外限制。
pub fn file_name_allowed(name: &str) -> bool {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return false;
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return false;
    }
    if trimmed.chars().any(char::is_control) {
        return false;
    }
    #[cfg(windows)]
    {
        if trimmed
            .chars()
            .any(|c| matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return false;
        }
        if trimmed.ends_with('.') || trimmed.ends_with(' ') {
            return false;
        }
    }
    true
}

/// 当前平台是否支持系统托盘（Android 无托盘，改为通知）。
pub fn tray_supported() -> bool {
    !cfg!(target_os = "android")
}

/// 当前运行平台标识（与前端 `LocalDevice.platform` 对应）。
///
/// 前端据此隐藏平台能力差异的入口（如 Android 无「打开所在文件夹」）。
pub fn name() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 通用规则：路径穿越、空名、控制字符必须拒绝（R5.2）。
    #[test]
    fn rejects_traversal_and_empty_names() {
        for raw in [
            "../evil.txt",
            "..\\evil.txt",
            "dir/file.txt",
            "..",
            ".",
            "",
            "   ",
        ] {
            assert!(!file_name_allowed(raw), "{raw:?} should be rejected");
        }
        assert!(!file_name_allowed("bad\u{0}name"));
    }

    /// 正常路径：普通中文/英文名通过。
    #[test]
    fn accepts_plain_names() {
        assert!(file_name_allowed("photo.png"));
        assert!(file_name_allowed("报告 v2.pdf"));
    }

    /// 平台差异：Windows 拒绝 `:` 与结尾点/空格，Unix 允许。
    #[test]
    fn platform_specific_rules() {
        #[cfg(windows)]
        {
            assert!(!file_name_allowed("a:b.txt"));
            assert!(!file_name_allowed("report."));
            assert!(!file_name_allowed("what?.txt"));
            // 结尾空格会被 trim 掉，因此不视为非法名。
            assert!(file_name_allowed("report "));
        }
        #[cfg(not(windows))]
        {
            assert!(file_name_allowed("a:b.txt"));
            assert!(file_name_allowed("report."));
        }
    }

    /// 托盘能力：桌面端支持，Android 不支持。
    #[test]
    fn tray_support_matches_platform() {
        #[cfg(target_os = "android")]
        assert!(!tray_supported());
        #[cfg(not(target_os = "android"))]
        assert!(tray_supported());
    }

    /// 平台标识：与实际编译 target 一致。
    #[test]
    fn platform_name_matches_target() {
        #[cfg(target_os = "android")]
        assert_eq!(name(), "android");
        #[cfg(windows)]
        assert_eq!(name(), "windows");
        #[cfg(target_os = "macos")]
        assert_eq!(name(), "macos");
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
        assert_eq!(name(), "linux");
    }
}
