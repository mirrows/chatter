//! Chatters Tauri 后端入口。
//!
//! 当前进度：
//! - M2-a：配置 + UDP 设备发现（`list_devices` / `get_local_device` + `device://update`）；
//! - M2-b：TCP 会话 + 文字消息 ACK（`send_text` + `message://new` / `session://update`）；
//! - M3-a：图片/文件传输主流程（`send_file` / `accept_file` / `cancel_transfer` + `transfer://update`）。
//!
//! 后续里程碑按 rules.md 逐个功能叠加。

pub mod autoclean;
pub mod commands;
pub mod config;
pub mod discovery;
pub mod errorlog;
pub mod platform;
pub mod protocol;
pub mod session;
pub mod share;
pub mod storage;
pub mod transfer;

use commands::{
    apply_device_update, AppState, DeviceUpdate, IncomingMessageDto, SessionUpdate, TransferDto,
    DEVICE_UPDATE_EVENT, MESSAGE_NEW_EVENT, SESSION_UPDATE_EVENT, TRANSFER_UPDATE_EVENT,
};
use session::{start_session, SessionConfig, SessionEvent, DEFAULT_ACK_TIMEOUT};
use std::path::Path;
#[cfg(not(target_os = "android"))]
use std::path::PathBuf;
use storage::{HistoryStore, StoredMessage, StoredTransfer};
#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem};
#[cfg(desktop)]
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use transfer::TransferInfo;
use transfer::{TransferConfig, TransferDirection, TransferEvent, TransferManager, TransferState};

/// 读取配置文件；文件不存在时把默认配置落盘。
///
/// 首次启动落盘是为了让 `device_id` 稳定（F2）：若只在内存里生成默认值，
/// 每次启动都会变成新设备，历史会话与续传记录将无法对应。
fn load_or_init_config(path: &Path) -> config::AppConfig {
    match config::load(path) {
        Ok(cfg) => {
            if !path.exists() {
                if let Err(e) = config::save(path, &cfg) {
                    log::warn!("failed to persist default config: {e}");
                }
            }
            cfg
        }
        Err(e) => {
            // 配置损坏时不阻塞启动（用户仍可发现设备），降级为默认值并记录原因。
            log::warn!("failed to load config, falling back to defaults: {e}");
            config::AppConfig::default()
        }
    }
}

/// 窗口不在前台时弹出系统通知（F51）；正文不含消息明文（R5.1）。
fn notify_if_unfocused(app: &tauri::AppHandle, body: &str) {
    let focused = app
        .get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false);
    if focused {
        return;
    }
    if let Err(e) = app
        .notification()
        .builder()
        .title("Chatters")
        .body(body)
        .show()
    {
        log::warn!("failed to show notification: {e}");
    }
}

/// 传输相关的系统通知文案（F51）：仅接收侧需要提醒，且不含文件路径（R5.1）。
fn transfer_notification(info: &TransferInfo) -> Option<&'static str> {
    match (info.direction, info.state) {
        (TransferDirection::Receive, TransferState::Pending) => {
            Some("收到文件，请在会话中确认接收")
        }
        (TransferDirection::Receive, TransferState::Done) => Some("文件接收完成"),
        _ => None,
    }
}

/// Runs the Tauri application.
///
/// Error handling: `tauri::Error` is returned to the caller (main), which
/// currently logs and exits non-zero.
pub fn run() -> Result<(), tauri::Error> {
    let builder = tauri::Builder::default();

    // 单实例必须在其它插件之前注册（F51：再次启动时聚焦已有窗口）。
    // Android 由系统保证单实例，无需该插件。
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));

    let builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());

    // Android 原生能力插件（M9）：MulticastLock / SAF / 前台服务。
    #[cfg(target_os = "android")]
    let builder = builder.plugin(
        tauri::plugin::Builder::<tauri::Wry, ()>::new(platform::android::PLUGIN_NAME)
            .setup(|_app, api| {
                let handle = api
                    .register_android_plugin(
                        platform::android::PLUGIN_PACKAGE,
                        platform::android::PLUGIN_CLASS,
                    )
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                platform::android::init(handle);
                Ok(())
            })
            .build(),
    );

    // 开机自启仅桌面端可用（F50）。
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_autostart::init(
        tauri_plugin_autostart::MacosLauncher::LaunchAgent,
        None,
    ));

    // 关闭窗口时最小化到托盘（F51）；真正退出走托盘菜单。移动端无关闭请求。
    #[cfg(desktop)]
    let builder = builder.on_window_event(|window, event| {
        if let tauri::WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = window.hide();
        }
    });

    builder
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let config_path = config_dir.join("config.json");
            let cfg = load_or_init_config(&config_path);
            // 日志只记录 ID，不输出消息内容或文件路径（rules.md R5.1）。
            log::info!("chatters starting with device_id={}", cfg.device_id);

            // 历史存储（M4）：JSON 文件，启动时加载。
            let history =
                tauri::async_runtime::block_on(HistoryStore::open(config_dir.join("history.json")));

            // TCP 监听需要在 manage 之前就绪，否则首条 send_text 可能找不到会话中心。
            let (hub, mut session_events, _session_task) =
                tauri::async_runtime::block_on(start_session(SessionConfig::new(
                    &cfg.device_id,
                    &cfg.nickname,
                    cfg.tcp_port,
                    DEFAULT_ACK_TIMEOUT,
                )))?;

            // 传输中心：接收目录取配置里的下载目录，图片自动接收（F34）。
            // Android 无用户可见下载目录（scoped storage）：统一固定为
            // `<app_data>/download/chatter`，不再使用相对路径，避免清空历史时定位不到文件。
            #[cfg(target_os = "android")]
            let receive_dir =
                crate::platform::paths::android_download_dir(&app.path().app_data_dir()?);
            #[cfg(not(target_os = "android"))]
            let receive_dir = PathBuf::from(cfg.download_dir.clone());
            let (transfer_tx, mut transfer_rx) = tokio::sync::mpsc::channel::<TransferEvent>(64);
            let transfers = TransferManager::new(
                TransferConfig::new(receive_dir.clone(), true),
                hub.clone(),
                transfer_tx,
            );
            // 空闲超时巡检（M3-b）：必须在 Tokio 运行时内启动。
            let maintenance = transfers.clone();
            tauri::async_runtime::spawn(async move {
                maintenance.spawn_maintenance();
            });
            let transfer_router = transfers.clone();
            #[cfg(target_os = "android")]
            let transfer_watch = transfers.clone();
            // 发送方向的历史需要记住源文件，供重新进入会话后预览图片（v3）。
            let transfer_source = transfers.clone();

            // 发现服务句柄：先创建，注入 AppState 后再启动发现任务，
            // 便于发送失败/切回前台时主动触发重新发现（v2/M9 真机问题）。
            let discovery = discovery::DiscoveryHandle::default();

            // 错误日志（v3）：按天归档，启动时清理超过 7 天的目录。
            let error_log = std::sync::Arc::new(errorlog::ErrorLog::open(
                app.path().app_data_dir()?.join("logs"),
            ));
            let state = AppState::new(
                cfg.clone(),
                config_path.clone(),
                hub,
                transfers,
                history.clone(),
                discovery.clone(),
                error_log,
                receive_dir.clone(),
            );
            let devices = state.devices.clone();
            app.manage(state);

            // 定时清理（v3）：开启后每次启动删除超过保留天数的接收文件，
            // 并同步移除指向这些文件的历史记录，避免出现"点开必失败"的条目。
            if cfg.auto_clean_enabled {
                let clean_dir = receive_dir.clone();
                let clean_days = cfg.auto_clean_days;
                let clean_history = history.clone();
                tauri::async_runtime::spawn(async move {
                    let report = autoclean::clean(&clean_dir, clean_days).await;
                    if report.files == 0 {
                        return;
                    }
                    let removed = report.removed.clone();
                    let dropped = clean_history
                        .remove_transfers(|item| {
                            item.direction == "receive"
                                && item
                                    .path
                                    .as_ref()
                                    .map(|path| removed.contains(&std::path::PathBuf::from(path)))
                                    .unwrap_or(false)
                        })
                        .await
                        .unwrap_or(0);
                    log::info!(
                        "auto clean: {} files, {} bytes, {} history records",
                        report.files,
                        report.bytes,
                        dropped
                    );
                });
            }

            // 托盘（F51）：显示主窗口 / 退出；双击图标亦显示窗口。
            // Android 无托盘（M9 改为纯通知），因此整块仅在桌面端编译。
            #[cfg(desktop)]
            {
                let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
                let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
                let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
                match app.default_window_icon().cloned() {
                    Some(icon) if platform::tray_supported() => {
                        TrayIconBuilder::with_id("main-tray")
                            .icon(icon)
                            .tooltip("Chatters")
                            .menu(&tray_menu)
                            .show_menu_on_left_click(true)
                            .on_menu_event(|app, event| match event.id.as_ref() {
                                "show" => {
                                    if let Some(window) = app.get_webview_window("main") {
                                        let _ = window.show();
                                        let _ = window.set_focus();
                                    }
                                }
                                "quit" => app.exit(0),
                                _ => {}
                            })
                            .build(app)?;
                    }
                    _ => {
                        log::warn!(
                            "tray unavailable on this platform or icon missing, tray disabled"
                        )
                    }
                }
            }

            // Android（M9）：持有 Wi-Fi 组播锁，否则省电策略会过滤 UDP 广播，
            // 导致「设备列表一直为空」。失败只降级为手动 IP 添加，不阻塞启动。
            #[cfg(target_os = "android")]
            match platform::android::acquire_multicast_lock() {
                Ok(()) => log::info!("multicast lock acquired"),
                Err(e) => {
                    log::warn!("failed to acquire multicast lock, discovery may miss peers: {e}")
                }
            }

            // 传输事件 → 前端 transfer://update（含待确认文件）+ 历史落库。
            let transfer_handle = app.handle().clone();
            let transfer_history = history.clone();
            #[cfg(target_os = "android")]
            let mut foreground_running = false;
            tauri::async_runtime::spawn(async move {
                while let Some(TransferEvent::Updated(info)) = transfer_rx.recv().await {
                    let payload = TransferDto::from(&info);
                    if let Err(e) = transfer_handle.emit(TRANSFER_UPDATE_EVENT, payload) {
                        log::warn!("failed to emit transfer update: {e}");
                    }
                    if let Err(e) = transfer_history
                        .upsert_transfer(StoredTransfer {
                            transfer_id: info.transfer_id.clone(),
                            peer_id: info.peer_id.clone(),
                            direction: info.direction.as_str().to_string(),
                            kind: info.kind.as_str().to_string(),
                            filename: info.filename.clone(),
                            size: info.size,
                            transferred: info.transferred,
                            state: info.state.as_str().to_string(),
                            path: info.path.clone(),
                            source_path: if info.direction == TransferDirection::Send {
                                transfer_source.source_path(&info.transfer_id).await
                            } else {
                                None
                            },
                            ts: info.ts,
                        })
                        .await
                    {
                        log::warn!("failed to persist transfer record: {e}");
                    }
                    if let Some(body) = transfer_notification(&info) {
                        notify_if_unfocused(&transfer_handle, body);
                    }

                    // Android（M9）：传输期间启动前台服务，避免切后台/锁屏被挂起；
                    // 全部结束后再停止，避免常驻通知打扰用户。
                    #[cfg(target_os = "android")]
                    {
                        let active = transfer_watch.list().await.iter().any(|task| {
                            matches!(
                                task.state,
                                TransferState::Transferring
                                    | TransferState::Offered
                                    | TransferState::Pending
                            )
                        });
                        match (active, foreground_running) {
                            (true, false) => match platform::android::start_foreground(
                                "Chatters 正在传输",
                                "保持连接，退出应用前请先完成传输",
                            ) {
                                Ok(()) => foreground_running = true,
                                Err(e) => log::warn!("failed to start foreground service: {e}"),
                            },
                            (false, true) => {
                                if let Err(e) = platform::android::stop_foreground() {
                                    log::warn!("failed to stop foreground service: {e}");
                                }
                                foreground_running = false;
                            }
                            _ => {}
                        }
                    }
                }
            });

            // 会话事件 → 前端事件（message://new、session://update），
            // 传输帧直接交给传输中心处理。
            let session_handle = app.handle().clone();
            let chat_history = history.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(event) = session_events.recv().await {
                    match event {
                        SessionEvent::TransferFrame { device_id, frame } => {
                            if let Err(e) = transfer_router.handle_frame(&device_id, frame).await {
                                log::warn!("transfer frame handling failed: {e}");
                            }
                        }
                        SessionEvent::TextReceived {
                            device_id,
                            msg_id,
                            text,
                            ts,
                        } => {
                            if let Err(e) = chat_history
                                .append_message(StoredMessage {
                                    msg_id: msg_id.clone(),
                                    peer_id: device_id.clone(),
                                    direction: "in".to_string(),
                                    text: text.clone(),
                                    ts,
                                    status: "delivered".to_string(),
                                })
                                .await
                            {
                                log::warn!("failed to persist incoming message: {e}");
                            }
                            let payload = IncomingMessageDto {
                                peer_id: device_id,
                                msg_id,
                                text,
                                ts,
                            };
                            if let Err(e) = session_handle.emit(MESSAGE_NEW_EVENT, payload) {
                                log::warn!("failed to emit new message: {e}");
                            }
                            notify_if_unfocused(&session_handle, "收到一条新消息");
                        }
                        SessionEvent::PeerConnected {
                            device_id, name, ..
                        } => {
                            // 重连成功：自动重试一次因断线失败的发送任务（M11）。
                            let retry_router = transfer_router.clone();
                            let retry_peer = device_id.clone();
                            tauri::async_runtime::spawn(async move {
                                retry_router.handle_peer_connected(&retry_peer).await;
                            });
                            let update = SessionUpdate::Connected {
                                peer_id: device_id,
                                name,
                            };
                            if let Err(e) = session_handle.emit(SESSION_UPDATE_EVENT, update) {
                                log::warn!("failed to emit session update: {e}");
                            }
                        }
                        SessionEvent::PeerDisconnected { device_id } => {
                            // 连接断开时把该对端未完成任务标记为可重试（M3-b）。
                            transfer_router.handle_peer_disconnected(&device_id).await;
                            let update = SessionUpdate::Disconnected { peer_id: device_id };
                            if let Err(e) = session_handle.emit(SESSION_UPDATE_EVENT, update) {
                                log::warn!("failed to emit session update: {e}");
                            }
                        }
                        SessionEvent::PeerReconnecting { device_id, attempt } => {
                            let update = SessionUpdate::Reconnecting {
                                peer_id: device_id,
                                attempt,
                            };
                            if let Err(e) = session_handle.emit(SESSION_UPDATE_EVENT, update) {
                                log::warn!("failed to emit session update: {e}");
                            }
                        }
                        SessionEvent::PeerReconnectFailed { device_id } => {
                            let update = SessionUpdate::ReconnectFailed { peer_id: device_id };
                            if let Err(e) = session_handle.emit(SESSION_UPDATE_EVENT, update) {
                                log::warn!("failed to emit session update: {e}");
                            }
                        }
                    }
                }
            });

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let (mut rx, _task) = match discovery::start_discovery(
                    cfg.discovery_port,
                    cfg.device_id.clone(),
                    cfg.nickname.clone(),
                    cfg.tcp_port,
                    discovery,
                )
                .await
                {
                    Ok(started) => started,
                    Err(e) => {
                        log::error!(
                            "discovery failed to start on port {}: {e}",
                            cfg.discovery_port
                        );
                        return;
                    }
                };

                while let Some(event) = rx.recv().await {
                    let update = DeviceUpdate::from(event);
                    {
                        let mut guard = devices.write().await;
                        apply_device_update(&mut guard, &update);
                    }
                    if let Err(e) = handle.emit(DEVICE_UPDATE_EVENT, update) {
                        log::warn!("failed to emit device update: {e}");
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::refresh_devices,
            commands::get_local_device,
            commands::send_text,
            commands::resend_text,
            commands::send_file,
            commands::accept_file,
            commands::reject_file,
            commands::cancel_transfer,
            commands::retry_transfer,
            commands::list_transfers,
            commands::get_image_preview,
            commands::pick_file,
            commands::get_settings,
            commands::save_settings,
            commands::get_history,
            commands::get_history_page,
            commands::clear_history,
            commands::take_share_payload,
            commands::import_shared_file,
            commands::open_download_dir,
            commands::pick_download_dir,
            commands::report_error,
            commands::get_error_log,
            commands::open_error_log_dir,
            commands::get_autostart,
            commands::set_autostart,
            commands::add_device_by_ip,
            commands::save_clipboard_image,
            commands::send_paths,
            commands::open_transfer_file,
            commands::reveal_transfer_file,
            commands::save_transfer_as
        ])
        .run(tauri::generate_context!())
}

/// Tauri 移动端入口（Android）：`Rust.kt` 通过 JNI 调用 `Rust.onCreate`，由该宏生成桥接代码。
///
/// 桌面端不参与编译（`mobile` cfg 由 `tauri-build` 按目标平台注入），
/// 因此不影响 `main.rs` 的桌面启动流程。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_mobile() {
    // Android 没有 main.rs，日志需在这里初始化；默认 info 级别，便于 `adb logcat -s RustStdoutStderr` 排障。
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();
    if let Err(e) = run() {
        log::error!("chatters exited with error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：首次启动创建配置文件，且再次加载得到同一 device_id。
    #[test]
    fn load_or_init_config_persists_device_id() {
        let dir = std::env::temp_dir().join("chatters_lib_test_cfg");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::remove_file(&path).ok();

        let first = load_or_init_config(&path);
        assert!(path.exists());
        let second = load_or_init_config(&path);
        assert_eq!(first.device_id, second.device_id);
        assert_eq!(first.nickname, second.nickname);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 异常：配置内容损坏时回退默认配置而不是崩溃。
    #[test]
    fn load_or_init_config_falls_back_on_corrupt_file() {
        let dir = std::env::temp_dir().join("chatters_lib_test_cfg_bad");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, "{{{ not json").unwrap();

        let cfg = load_or_init_config(&path);
        assert_eq!(cfg.tcp_port, config::DEFAULT_TCP_PORT);
        assert!(!cfg.device_id.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    fn transfer_info(direction: TransferDirection, state: TransferState) -> TransferInfo {
        TransferInfo {
            transfer_id: "t-1".to_string(),
            peer_id: "peer-1".to_string(),
            direction,
            kind: transfer::TransferKind::File,
            filename: "a.zip".to_string(),
            size: 10,
            transferred: 10,
            state,
            path: None,
            error: None,
            ts: 1,
        }
    }

    /// 正常路径：接收文件待确认与接收完成都提醒，发送侧不提醒（F51）。
    #[test]
    fn transfer_notification_covers_receive_states() {
        assert_eq!(
            transfer_notification(&transfer_info(
                TransferDirection::Receive,
                TransferState::Pending
            )),
            Some("收到文件，请在会话中确认接收")
        );
        assert_eq!(
            transfer_notification(&transfer_info(
                TransferDirection::Receive,
                TransferState::Done
            )),
            Some("文件接收完成")
        );
        assert_eq!(
            transfer_notification(&transfer_info(TransferDirection::Send, TransferState::Done)),
            None
        );
        assert_eq!(
            transfer_notification(&transfer_info(
                TransferDirection::Receive,
                TransferState::Transferring
            )),
            None
        );
    }
}
