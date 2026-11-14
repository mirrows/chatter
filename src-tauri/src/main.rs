#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! 桌面端入口。
//!
//! release 构建使用 Windows GUI 子系统（不弹出控制台窗口）；debug 构建保留控制台，
//! 便于 `npm run tauri dev` 时直接查看日志。

use std::fs::OpenOptions;
use std::path::PathBuf;

/// 日志初始化：默认输出到 stderr。
///
/// release 下没有控制台可写，因此设置环境变量 `CHATTERS_LOG=1` 时改为追加写入
/// `<配置目录>/chatters.log`（与 Tauri `app_config_dir()` 一致），用于现场排障。
fn init_logging() {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if std::env::var_os("CHATTERS_LOG").is_some() {
        if let Some(path) = log_file_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match OpenOptions::new().create(true).append(true).open(&path) {
                Ok(file) => {
                    builder.target(env_logger::Target::Pipe(Box::new(file)));
                }
                Err(e) => {
                    eprintln!("failed to open log file {}: {e}", path.display());
                }
            }
        }
    }
    let _ = builder.try_init();
}

/// 日志文件路径：与 Tauri 的 `app_config_dir()` 保持一致（`%APPDATA%\com.chatters.app`）。
fn log_file_path() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(all(unix, not(target_os = "macos")))]
    let base = std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"));

    base.map(|dir| dir.join("com.chatters.app").join("chatters.log"))
}

fn main() {
    init_logging();
    if let Err(e) = chatters_lib::run() {
        eprintln!("chatters exited with error: {e}");
        std::process::exit(1);
    }
}
