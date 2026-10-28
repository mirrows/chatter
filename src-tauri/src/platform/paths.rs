//! 平台路径解析（v2 / M6）。
//!
//! 统一入口，避免各模块自行读取 `USERPROFILE` / `HOME`。

use std::path::PathBuf;

/// 用户主目录：Windows 读 `USERPROFILE`，macOS/Linux 读 `HOME`。
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// 默认下载目录：`<home>/Downloads/Chatters`；无法定位主目录时回退当前目录。
///
/// Android 端不使用该目录（v2/M9 走应用私有目录 + SAF 导出）。
pub fn download_dir() -> PathBuf {
    let mut base = home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.push("Downloads");
    base.push("Chatters");
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：主目录可定位时下载目录以其为根。
    #[test]
    fn download_dir_is_under_home() {
        let dir = download_dir();
        if let Some(home) = home_dir() {
            assert!(dir.starts_with(&home), "{dir:?} 应位于 {home:?} 下");
        }
        assert!(dir.ends_with("Chatters"));
    }
}
