//! Android 原生能力桥（v2/M9）。
//!
//! Rust 无法直接调用 `WifiManager` / SAF / 前台服务，这里通过 Tauri 的移动插件机制
//! 调用 Kotlin 侧 `ChattersAndroidPlugin`：
//! - `acquireMulticastLock`：持有 `MulticastLock`，否则 Wi-Fi 省电会过滤 UDP 广播；
//! - `openFile` / `saveFile`：用 FileProvider + `ACTION_VIEW` 打开、用 SAF
//!   （`ACTION_CREATE_DOCUMENT`）导出到用户选择的目录；
//! - `startForegroundService` / `stopForegroundService`：传输期间保持进程不被挂起。
//!
//! 说明：`run_mobile_plugin` 是**阻塞**调用（内部等待 Kotlin 回调），
//! 因此在异步命令里必须放到 `spawn_blocking` 中执行。

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::sync::OnceLock;
use tauri::plugin::PluginHandle;
use tauri::Wry;

/// Kotlin 插件在 Tauri 中的注册名（须与 `plugin::Builder::new` 一致）。
pub const PLUGIN_NAME: &str = "chatters-android";
/// Kotlin 插件所在包名。
pub const PLUGIN_PACKAGE: &str = "com.chatters.app";
/// Kotlin 插件类名。
pub const PLUGIN_CLASS: &str = "ChattersAndroidPlugin";

static BRIDGE: OnceLock<PluginHandle<Wry>> = OnceLock::new();

/// 保存插件句柄（由 Tauri 插件 setup 调用一次）。
pub fn init(handle: PluginHandle<Wry>) {
    let _ = BRIDGE.set(handle);
}

/// 是否已就绪（未就绪时所有能力静默降级）。
pub fn is_ready() -> bool {
    BRIDGE.get().is_some()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveFileRequest<'a> {
    path: &'a str,
    file_name: &'a str,
}

#[derive(Serialize)]
struct PathRequest<'a> {
    path: &'a str,
}

#[derive(Serialize)]
struct ForegroundRequest<'a> {
    title: &'a str,
    body: &'a str,
}

#[derive(Deserialize)]
struct SavedFile {
    path: Option<String>,
}

/// 通用调用：把 JSON 负载发给 Kotlin，反序列化返回值。
fn call<T: DeserializeOwned>(command: &str, payload: impl Serialize) -> Result<T, String> {
    let handle = BRIDGE
        .get()
        .ok_or_else(|| "android bridge not initialized".to_string())?;
    handle
        .run_mobile_plugin::<T>(command, payload)
        .map_err(|e| e.to_string())
}

/// 自检：确认 Kotlin 插件已正确注册（返回 `pong`）。
pub fn ping() -> Result<String, String> {
    #[derive(Deserialize)]
    struct Pong {
        value: String,
    }
    call::<Pong>("ping", serde_json::json!({})).map(|p| p.value)
}

/// 持有 Wi-Fi 组播锁（广播发现必需）。
pub fn acquire_multicast_lock() -> Result<(), String> {
    call::<serde_json::Value>("acquireMulticastLock", serde_json::json!({})).map(|_| ())
}

/// 释放 Wi-Fi 组播锁（应用退出或停止发现时调用）。
pub fn release_multicast_lock() -> Result<(), String> {
    call::<serde_json::Value>("releaseMulticastLock", serde_json::json!({})).map(|_| ())
}

/// 用系统默认程序打开文件（FileProvider + `ACTION_VIEW`）。
pub fn open_file(path: &str) -> Result<(), String> {
    call::<serde_json::Value>("openFile", PathRequest { path }).map(|_| ())
}

/// SAF 导出：让用户选择目标位置并复制，成功返回目标 Uri，取消返回 `None`。
pub fn save_file(path: &str, file_name: &str) -> Result<Option<String>, String> {
    match call::<SavedFile>("saveFile", SaveFileRequest { path, file_name }) {
        Ok(saved) => Ok(saved.path),
        // Kotlin 侧取消时 reject("cancelled")，这里按“用户取消”处理而不是错误。
        Err(e) if e.to_lowercase().contains("cancel") => Ok(None),
        Err(e) => Err(e),
    }
}

/// SAF 选中的 Uri 复制到应用缓存目录，返回本地绝对路径（Android 发送图片/文件用）。
///
/// `pick_file` 在 Android 上拿不到文件系统路径（只有 `content://` Uri），
/// Rust 无法直接读取 SAF 内容，因此由 Kotlin 侧复制到私有缓存后再发送。
pub fn import_file(uri: &str) -> Result<String, String> {
    #[derive(Serialize)]
    struct ImportRequest<'a> {
        uri: &'a str,
    }
    #[derive(Deserialize)]
    struct Imported {
        path: String,
    }
    call::<Imported>("importFile", ImportRequest { uri }).map(|imported| imported.path)
}

/// 传输期间启动前台服务（常驻通知，避免被系统挂起）。
pub fn start_foreground(title: &str, body: &str) -> Result<(), String> {
    call::<serde_json::Value>("startForegroundService", ForegroundRequest { title, body })
        .map(|_| ())
}

/// 停止前台服务。
pub fn stop_foreground() -> Result<(), String> {
    call::<serde_json::Value>("stopForegroundService", serde_json::json!({})).map(|_| ())
}

/// 是否有可用的原生桥（供命令层判断走 Android 分支还是桌面分支）。
pub fn available() -> bool {
    is_ready()
}
