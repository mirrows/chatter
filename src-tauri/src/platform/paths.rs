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

/// 默认下载目录。
///
/// - 桌面：`<home>/Downloads/Chatters`（无法定位主目录时回退当前目录）；
/// - Android：无用户可见下载目录，返回相对子目录 `download/chatter`，
///   由 `lib.rs` 用 `app_data_dir()` 拼成固定私有目录（scoped storage 下无法直写公共 Downloads）。
pub fn download_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // 与 lib.rs 的接收目录保持一致：`<app_data>/download/chatter`。
        PathBuf::from("download").join("chatter")
    }
    #[cfg(not(target_os = "android"))]
    {
        let mut base = home_dir().unwrap_or_else(|| PathBuf::from("."));
        base.push("Downloads");
        base.push("Chatters");
        base
    }
}

/// Android 固定的接收目录：`<app_data>/download/chatter`（v3）。
///
/// 抽成函数便于在桌面端单测路径拼装（真机上无法写公共 Downloads，只能落应用私有目录）。
pub fn android_download_dir(app_data: &std::path::Path) -> PathBuf {
    app_data.join("download").join("chatter")
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

    /// 正常路径：Android 接收目录固定为 `<app_data>/download/chatter`。
    #[test]
    fn android_download_dir_is_fixed() {
        let dir = android_download_dir(std::path::Path::new("/data/user/0/com.chatters.app/files"));
        assert_eq!(
            dir,
            PathBuf::from("/data/user/0/com.chatters.app/files/download/chatter")
        );
    }
}
