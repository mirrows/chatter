//! 前端可调用的 Tauri command 封装。
//!
//! - M2-a：设备发现（`list_devices` / `get_local_device` + `device://update`）；
//! - M2-b：文字消息（`send_text` + `message://new` / `session://update`）。
//!
//! 与 `src/api/types.ts` 的类型一一对应（rules.md R3.5），字段统一序列化为
//! camelCase，避免前端做字段名转换。

use crate::config::AppConfig;
use crate::discovery::{DeviceEntry, DiscoveryEvent};
#[cfg(not(target_os = "android"))]
use crate::platform::shell::{open_command, reveal_command};
use crate::session::{SendStatus, SessionHub};
use crate::storage::{now_millis, History, HistoryStore, StoredMessage};
use crate::transfer::{TransferInfo, TransferManager};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Emitter, Manager};
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_dialog::DialogExt;
use tokio::sync::RwLock;

/// 设备表变更事件名，前端通过 `listen("device://update")` 订阅。
pub const DEVICE_UPDATE_EVENT: &str = "device://update";
/// 收到新消息事件名。
pub const MESSAGE_NEW_EVENT: &str = "message://new";
/// 会话连接状态事件名。
pub const SESSION_UPDATE_EVENT: &str = "session://update";
/// 传输任务状态事件名。
pub const TRANSFER_UPDATE_EVENT: &str = "transfer://update";

/// 在线设备 DTO（对应前端 `DeviceEntry`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDto {
    /// 设备唯一 ID。
    pub device_id: String,
    /// 设备昵称。
    pub name: String,
    /// 设备地址，格式为 `ip:port`（发现包来源地址）。
    pub addr: String,
    /// 对端 TCP 通信端口。
    pub tcp_port: u16,
    /// 设备指纹（v2/M10）：device_id 的 SHA-256 前 8 字节，供人工核对。
    pub fingerprint: String,
}

impl From<&DeviceEntry> for DeviceDto {
    fn from(entry: &DeviceEntry) -> Self {
        Self {
            device_id: entry.device_id.clone(),
            name: entry.name.clone(),
            addr: entry.addr.to_string(),
            tcp_port: entry.tcp_port,
            fingerprint: fingerprint(&entry.device_id),
        }
    }
}

/// 计算设备指纹：`device_id` 的 SHA-256 前 4 字节，按 2 组 4 位大写十六进制展示。
///
/// 例：`A3F2-91C0`（对应 `13-v2-plan.md` §7.2）。仅用于人工核对身份，不替代加密。
pub fn fingerprint(device_id: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(device_id.as_bytes());
    let hex: String = digest[..4].iter().map(|b| format!("{b:02X}")).collect();
    format!("{}-{}", &hex[0..4], &hex[4..8])
}

/// 设备表变更事件，通过 Tauri event `device://update` 推送给前端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DeviceUpdate {
    /// 设备上线或信息刷新。
    Up {
        /// 设备快照。
        device: DeviceDto,
    },
    /// 设备离线（超时或收到 bye）。
    Down {
        /// 离线设备 ID。
        device_id: String,
    },
}

impl From<DiscoveryEvent> for DeviceUpdate {
    fn from(event: DiscoveryEvent) -> Self {
        match event {
            DiscoveryEvent::DeviceUp(entry) => DeviceUpdate::Up {
                device: DeviceDto::from(&entry),
            },
            DiscoveryEvent::DeviceDown(device_id) | DiscoveryEvent::DeviceBye(device_id) => {
                DeviceUpdate::Down { device_id }
            }
        }
    }
}

/// 本机信息 DTO（对应前端 `LocalDevice`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalDeviceDto {
    /// 本机唯一 ID。
    pub device_id: String,
    /// 本机昵称。
    pub name: String,
    /// UDP 发现端口。
    pub discovery_port: u16,
    /// TCP 通信端口。
    pub tcp_port: u16,
    /// 本机指纹（v2/M10），便于让对方核对。
    pub fingerprint: String,
}

/// 收到的新消息（`message://new` 事件载荷，对应前端 `IncomingMessage`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingMessageDto {
    /// 发送方设备 ID。
    pub peer_id: String,
    /// 消息 ID。
    pub msg_id: String,
    /// 消息内容。
    pub text: String,
    /// 发送时间（Unix 毫秒）。
    pub ts: i64,
}

/// 会话连接状态事件（`session://update` 事件载荷）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SessionUpdate {
    /// 握手完成，会话可用。
    Connected {
        /// 对端设备 ID。
        peer_id: String,
        /// 对端昵称。
        name: String,
    },
    /// 会话断开。
    Disconnected {
        /// 对端设备 ID。
        peer_id: String,
    },
    /// 正在自动重连（v2/M10）。
    Reconnecting {
        /// 对端设备 ID。
        peer_id: String,
        /// 第几次尝试（从 1 开始）。
        attempt: u32,
    },
    /// 自动重连失败（v2/M10），需用户手动重连。
    ReconnectFailed {
        /// 对端设备 ID。
        peer_id: String,
    },
}

/// 文字消息发送状态（与前端 `TextStatus` 对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TextStatus {
    /// 已收到回执。
    Delivered,
    /// ACK 超时。
    Failed,
}

impl From<SendStatus> for TextStatus {
    fn from(status: SendStatus) -> Self {
        match status {
            SendStatus::Delivered => TextStatus::Delivered,
            SendStatus::Failed => TextStatus::Failed,
        }
    }
}

/// `send_text` 返回（对应前端 `TextSendResult`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSendResultDto {
    /// 本次消息 ID。
    pub msg_id: String,
    /// 发送状态。
    pub status: TextStatus,
}

/// 传输任务 DTO（对应前端 `TransferInfo`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferDto {
    /// 任务 ID。
    pub transfer_id: String,
    /// 对端设备 ID。
    pub peer_id: String,
    /// 方向：send | receive。
    pub direction: &'static str,
    /// 类型：image | file。
    pub kind: &'static str,
    /// 文件名。
    pub filename: String,
    /// 总字节数。
    pub size: u64,
    /// 已传输字节数。
    pub transferred: u64,
    /// 状态：pending | offered | transferring | done | failed | cancelled | rejected。
    pub state: &'static str,
    /// 接收完成后的落盘路径。
    pub path: Option<String>,
    /// 失败原因。
    pub error: Option<String>,
}

impl From<&TransferInfo> for TransferDto {
    fn from(info: &TransferInfo) -> Self {
        Self {
            transfer_id: info.transfer_id.clone(),
            peer_id: info.peer_id.clone(),
            direction: info.direction.as_str(),
            kind: info.kind.as_str(),
            filename: info.filename.clone(),
            size: info.size,
            transferred: info.transferred,
            state: info.state.as_str(),
            path: info.path.clone(),
            error: info.error.clone(),
        }
    }
}

/// 标准 base64 编码（用于图片预览 data URL，避免额外依赖）。
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) as usize & 0x3F] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3F] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(triple >> 6) as usize & 0x3F] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[triple as usize & 0x3F] as char
        } else {
            '='
        });
    }
    out
}

/// 按扩展名推断图片 MIME 类型。
fn image_mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "image/jpeg",
    }
}

/// 批量发送结果（对应前端 `PathSendResult`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathSendResult {
    /// 原始路径。
    pub path: String,
    /// 是否已发起传输。
    pub ok: bool,
    /// 失败原因（`is_dir` 表示目录，前端提示压缩为 zip）。
    pub error: Option<String>,
    /// 成功时的任务快照。
    pub transfer: Option<TransferDto>,
}

/// Tauri 管理的应用共享状态。
pub struct AppState {
    /// 本机配置（设置页可修改，读多写少）。
    pub config: Arc<RwLock<AppConfig>>,
    /// 配置文件路径（保存设置时落盘）。
    pub config_path: PathBuf,
    /// 在线设备快照，发现事件驱动更新。
    pub devices: Arc<RwLock<Vec<DeviceDto>>>,
    /// TCP 会话中心（文字消息、图片/文件传输共用）。
    pub session: SessionHub,
    /// 传输中心（图片/文件任务）。
    pub transfers: TransferManager,
    /// 本地历史（消息 + 传输记录）。
    pub history: HistoryStore,
}

impl AppState {
    /// 以配置、会话、传输中心与历史存储创建状态，设备列表初始为空。
    pub fn new(
        config: AppConfig,
        config_path: PathBuf,
        session: SessionHub,
        transfers: TransferManager,
        history: HistoryStore,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
            devices: Arc::new(RwLock::new(Vec::new())),
            session,
            transfers,
            history,
        }
    }
}

/// 把一次设备事件应用到设备列表快照上（按 deviceId 去重、按昵称排序）。
///
/// 纯函数，便于单元测试。
pub fn apply_device_update(devices: &mut Vec<DeviceDto>, update: &DeviceUpdate) {
    match update {
        DeviceUpdate::Up { device } => {
            match devices.iter_mut().find(|d| d.device_id == device.device_id) {
                Some(slot) => *slot = device.clone(),
                None => {
                    devices.push(device.clone());
                    devices.sort_by(|a, b| a.name.cmp(&b.name));
                }
            }
        }
        DeviceUpdate::Down { device_id } => devices.retain(|d| &d.device_id != device_id),
    }
}

/// 获取当前在线设备列表（F1）。
#[tauri::command]
pub async fn list_devices(state: tauri::State<'_, AppState>) -> Result<Vec<DeviceDto>, String> {
    Ok(state.devices.read().await.clone())
}

/// 获取本机设备信息（昵称、ID、端口），供界面显示（F2）。
#[tauri::command]
pub async fn get_local_device(state: tauri::State<'_, AppState>) -> Result<LocalDeviceDto, String> {
    let config = state.config.read().await;
    Ok(LocalDeviceDto {
        device_id: config.device_id.clone(),
        name: config.nickname.clone(),
        discovery_port: config.discovery_port,
        tcp_port: config.tcp_port,
        fingerprint: fingerprint(&config.device_id),
    })
}

/// 设置项（对应前端 `Settings`，F52）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    /// 本机 ID（只读）。
    pub device_id: String,
    /// 设备昵称。
    pub nickname: String,
    /// UDP 发现端口。
    pub discovery_port: u16,
    /// TCP 通信端口。
    pub tcp_port: u16,
    /// 下载目录。
    pub download_dir: String,
    /// 主题：light | dark。
    pub theme: String,
    /// 自动接收图片。
    pub auto_accept_images: bool,
    /// 自动接收普通文件。
    pub auto_accept_files: bool,
}

/// 设置保存结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsResult {
    /// 是否需要重启应用（昵称/端口变更后生效）。
    pub restart_required: bool,
}

/// 读取当前设置（F52）。
#[tauri::command]
pub async fn get_settings(state: tauri::State<'_, AppState>) -> Result<SettingsDto, String> {
    let config = state.config.read().await;
    Ok(SettingsDto {
        device_id: config.device_id.clone(),
        nickname: config.nickname.clone(),
        discovery_port: config.discovery_port,
        tcp_port: config.tcp_port,
        download_dir: config.download_dir.clone(),
        theme: config.theme.clone(),
        auto_accept_images: config.auto_accept_images,
        auto_accept_files: config.auto_accept_files,
    })
}

/// 保存设置（F52）：下载目录与自动接收即时生效，昵称/端口需重启。
#[tauri::command]
pub async fn save_settings(
    state: tauri::State<'_, AppState>,
    settings: SettingsDto,
) -> Result<SaveSettingsResult, String> {
    let nickname = settings.nickname.trim().to_string();
    if nickname.is_empty() {
        return Err("昵称不能为空".to_string());
    }
    if settings.discovery_port == 0 || settings.tcp_port == 0 {
        return Err("端口必须是 1-65535".to_string());
    }
    if settings.download_dir.trim().is_empty() {
        return Err("下载目录不能为空".to_string());
    }
    let theme = if settings.theme == "dark" {
        "dark"
    } else {
        "light"
    };

    let (restart_required, saved) = {
        let mut config = state.config.write().await;
        let restart_required = config.nickname != nickname
            || config.discovery_port != settings.discovery_port
            || config.tcp_port != settings.tcp_port;
        config.nickname = nickname;
        config.discovery_port = settings.discovery_port;
        config.tcp_port = settings.tcp_port;
        config.download_dir = settings.download_dir.trim().to_string();
        config.theme = theme.to_string();
        config.auto_accept_images = settings.auto_accept_images;
        config.auto_accept_files = settings.auto_accept_files;
        (restart_required, config.clone())
    };

    crate::config::save(&state.config_path, &saved).map_err(|e| e.to_string())?;
    state
        .transfers
        .update_settings(
            PathBuf::from(&saved.download_dir),
            saved.auto_accept_images,
            saved.auto_accept_files,
        )
        .await;
    Ok(SaveSettingsResult { restart_required })
}

/// 读取历史（F40）：`peer_id` 为空时返回全部。
#[tauri::command]
pub async fn get_history(
    state: tauri::State<'_, AppState>,
    peer_id: Option<String>,
) -> Result<History, String> {
    let peer = peer_id.as_deref();
    let messages = state.history.messages_for(peer).await;
    let transfers = state.history.transfers_for(peer).await;
    Ok(History {
        messages,
        transfers,
    })
}

/// 清空历史（F40/F41）：`peer_id` 为空时清空全部；`delete_files` 为 true 时删除下载目录内的历史文件。
#[tauri::command]
pub async fn clear_history(
    state: tauri::State<'_, AppState>,
    peer_id: Option<String>,
    delete_files: Option<bool>,
) -> Result<usize, String> {
    let peer = peer_id.as_deref();
    let mut removed = 0usize;
    if delete_files.unwrap_or(false) {
        let base = {
            let config = state.config.read().await;
            PathBuf::from(&config.download_dir)
        };
        let records = state.history.transfers_for(peer).await;
        let paths: Vec<String> = records
            .into_iter()
            .filter(|record| record.direction == "receive")
            .filter_map(|record| record.path)
            .collect();
        removed = delete_files_under(&base, &paths).await;
    }
    state.history.clear(peer).await.map_err(|e| e.to_string())?;
    Ok(removed)
}

/// 是否已开启开机自启（F50）。
///
/// Android 无“开机自启”概念（M9），恒返回未启用，前端开关不生效。
#[tauri::command]
pub fn get_autostart(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(desktop)]
    {
        app.autolaunch().is_enabled().map_err(|e| e.to_string())
    }
    #[cfg(not(desktop))]
    {
        let _ = app;
        Ok(false)
    }
}

/// 设置开机自启（F50）；Android 上为空操作。
#[tauri::command]
pub fn set_autostart(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let manager = app.autolaunch();
        let result = if enabled {
            manager.enable()
        } else {
            manager.disable()
        };
        result.map_err(|e| e.to_string())
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, enabled);
        Ok(())
    }
}

/// 由发现结果推导对端 TCP 地址。
///
/// 注意：`DeviceDto.addr` 是发现包的来源地址（UDP 端口），
/// 建连必须用 `tcp_port` 重新组合，否则会连到错误的端口。
pub fn peer_tcp_addr(device: &DeviceDto) -> Option<SocketAddr> {
    let (ip, _) = device.addr.rsplit_once(':')?;
    let ip: IpAddr = ip
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .ok()?;
    Some(SocketAddr::new(ip, device.tcp_port))
}

/// 确保与目标设备存在会话（不存在时按发现结果建连），返回实际会话 ID。
async fn ensure_session(state: &AppState, peer_id: &str) -> Result<String, String> {
    if state.session.is_connected(peer_id).await {
        return Ok(peer_id.to_string());
    }
    let target = {
        let devices = state.devices.read().await;
        devices
            .iter()
            .find(|d| d.device_id == peer_id)
            .and_then(peer_tcp_addr)
    };
    let target = target.ok_or_else(|| format!("peer not found: {peer_id}"))?;
    let peer = state
        .session
        .connect(target)
        .await
        .map_err(|e| e.to_string())?;
    Ok(peer.device_id)
}

/// 发送文字消息（F10）。
///
/// 未建立会话时按发现到的地址自动连接（F1 的结果即可直接聊天），
/// 返回消息 ID 与送达状态（F11）。
#[tauri::command]
pub async fn send_text(
    state: tauri::State<'_, AppState>,
    peer_id: String,
    text: String,
) -> Result<TextSendResultDto, String> {
    let text = text.trim_end_matches(['\r', '\n']).to_string();
    if text.trim().is_empty() {
        return Err("empty message".to_string());
    }

    let session_id = ensure_session(&state, &peer_id).await?;
    let result = state
        .session
        .send_text(&session_id, &text)
        .await
        .map_err(|e| e.to_string())?;
    let status = match result.status {
        SendStatus::Delivered => "delivered",
        SendStatus::Failed => "failed",
    };
    if let Err(e) = state
        .history
        .append_message(StoredMessage {
            msg_id: result.msg_id.clone(),
            peer_id: session_id,
            direction: "out".to_string(),
            text,
            ts: now_millis(),
            status: status.to_string(),
        })
        .await
    {
        log::warn!("failed to persist outgoing message: {e}");
    }
    Ok(TextSendResultDto {
        msg_id: result.msg_id,
        status: TextStatus::from(result.status),
    })
}

/// 发送图片或文件（F20/F30）：后端按扩展名区分类型，图片在接收端自动接收。
#[tauri::command]
pub async fn send_file(
    state: tauri::State<'_, AppState>,
    peer_id: String,
    path: String,
) -> Result<TransferDto, String> {
    let session_id = ensure_session(&state, &peer_id).await?;
    let info = state
        .transfers
        .send_file(&session_id, Path::new(&path))
        .await
        .map_err(|e| e.to_string())?;
    Ok(TransferDto::from(&info))
}

/// 接受接收中的文件（F34）。
#[tauri::command]
pub async fn accept_file(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    state
        .transfers
        .accept(&transfer_id)
        .await
        .map_err(|e| e.to_string())
}

/// 拒绝接收文件（F34）。
#[tauri::command]
pub async fn reject_file(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    state
        .transfers
        .reject(&transfer_id)
        .await
        .map_err(|e| e.to_string())
}

/// 取消传输（F33）。
#[tauri::command]
pub async fn cancel_transfer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    state
        .transfers
        .cancel(&transfer_id)
        .await
        .map_err(|e| e.to_string())
}

/// 重试失败的发送任务（M3-b）：以原文件重新发起传输。
#[tauri::command]
pub async fn retry_transfer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<TransferDto, String> {
    let info = state
        .transfers
        .retry(&transfer_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(TransferDto::from(&info))
}

/// 列出全部传输任务（界面重建时使用）。
#[tauri::command]
pub async fn list_transfers(state: tauri::State<'_, AppState>) -> Result<Vec<TransferDto>, String> {
    let items = state.transfers.list().await;
    Ok(items.iter().map(TransferDto::from).collect())
}

/// 读取已接收图片为 data URL，供聊天窗内直接预览（F23）。
///
/// M3-a 采用 base64 内联以避开额外的资源协议与权限配置；超过 8MB 的图片
/// 不内联预览（后续里程碑可改用 asset 协议流式预览）。
#[tauri::command]
pub async fn get_image_preview(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<String, String> {
    const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;
    let info = state
        .transfers
        .get(&transfer_id)
        .await
        .ok_or_else(|| format!("unknown transfer: {transfer_id}"))?;
    let path = info
        .path
        .ok_or_else(|| "image not received yet".to_string())?;
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| e.to_string())?;
    if metadata.len() > MAX_PREVIEW_BYTES {
        return Err("image too large for preview".to_string());
    }
    let bytes = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    Ok(format!(
        "data:{};base64,{}",
        image_mime(Path::new(&path)),
        base64_encode(&bytes)
    ))
}

/// 打开系统文件选择框，返回选中文件的绝对路径（F20/F30）。
#[tauri::command]
pub async fn pick_file(app: tauri::AppHandle, images_only: bool) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut builder = app.dialog().file();
    if images_only {
        builder = builder.add_filter("图片", &["jpg", "jpeg", "png", "gif", "webp", "bmp"]);
    }
    builder.pick_file(move |picked| {
        let _ = tx.send(picked);
    });
    let picked = rx
        .await
        .map_err(|_| "file dialog closed unexpectedly".to_string())?;
    match picked {
        Some(file) => {
            let path: PathBuf = file.into_path().map_err(|e| e.to_string())?;
            Ok(Some(path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// 发送一批本地路径（拖拽入口，F22）：目录返回 `is_dir` 由前端提示压缩为 zip（F31）。
#[tauri::command]
pub async fn send_paths(
    state: tauri::State<'_, AppState>,
    peer_id: String,
    paths: Vec<String>,
) -> Result<Vec<PathSendResult>, String> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let session_id = ensure_session(&state, &peer_id).await?;
    let mut results = Vec::with_capacity(paths.len());
    for raw in paths {
        let path = Path::new(&raw);
        let metadata = match tokio::fs::metadata(path).await {
            Ok(metadata) => metadata,
            Err(e) => {
                results.push(PathSendResult {
                    path: raw,
                    ok: false,
                    error: Some(format!("not_found: {e}")),
                    transfer: None,
                });
                continue;
            }
        };
        if metadata.is_dir() {
            results.push(PathSendResult {
                path: raw,
                ok: false,
                error: Some("is_dir".to_string()),
                transfer: None,
            });
            continue;
        }
        match state.transfers.send_file(&session_id, path).await {
            Ok(info) => results.push(PathSendResult {
                path: raw,
                ok: true,
                error: None,
                transfer: Some(TransferDto::from(&info)),
            }),
            Err(e) => results.push(PathSendResult {
                path: raw,
                ok: false,
                error: Some(e.to_string()),
                transfer: None,
            }),
        }
    }
    Ok(results)
}

/// 解析手动输入的设备地址（F3）：支持 `1.2.3.4`、`1.2.3.4:28101`、`[::1]:28101`。
pub fn parse_manual_addr(raw: &str, default_port: u16) -> Result<SocketAddr, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("请输入设备 IP 地址".to_string());
    }
    if let Ok(addr) = trimmed.parse::<SocketAddr>() {
        return Ok(addr);
    }
    let ip_text = trimmed.trim_start_matches('[').trim_end_matches(']');
    let ip: IpAddr = ip_text
        .parse()
        .map_err(|_| format!("无效的 IP 地址：{trimmed}"))?;
    Ok(SocketAddr::new(ip, default_port))
}

/// 手动添加设备（F3）：直接握手建连，成功后写入设备列表并通知前端。
#[tauri::command]
pub async fn add_device_by_ip(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    addr: String,
) -> Result<DeviceDto, String> {
    let default_port = state.config.read().await.tcp_port;
    let target = parse_manual_addr(&addr, default_port)?;
    let peer = state
        .session
        .connect(target)
        .await
        .map_err(|e| format!("连接失败：{e}"))?;

    let dto = DeviceDto {
        fingerprint: fingerprint(&peer.device_id),
        device_id: peer.device_id,
        name: peer.name,
        addr: target.to_string(),
        tcp_port: target.port(),
    };
    {
        let mut devices = state.devices.write().await;
        apply_device_update(
            &mut devices,
            &DeviceUpdate::Up {
                device: dto.clone(),
            },
        );
    }
    if let Err(e) = app.emit(
        DEVICE_UPDATE_EVENT,
        DeviceUpdate::Up {
            device: dto.clone(),
        },
    ) {
        log::warn!("failed to emit manual device update: {e}");
    }
    Ok(dto)
}

/// 读取剪贴板图片并落盘（F21），返回临时文件路径供 `send_file` 使用。
#[tauri::command]
pub async fn save_clipboard_image(
    app: tauri::AppHandle,
    data_base64: String,
    extension: String,
) -> Result<String, String> {
    const MAX_BYTES: usize = 50 * 1024 * 1024;
    let ext = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if !matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
    ) {
        return Err("不支持的图片格式".to_string());
    }
    let bytes = base64_decode(&data_base64)?;
    if bytes.is_empty() {
        return Err("图片数据为空".to_string());
    }
    if bytes.len() > MAX_BYTES {
        return Err("图片超过 50MB".to_string());
    }
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("clipboard");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.{ext}", uuid::Uuid::new_v4().simple()));
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// 标准 base64 解码（用于粘贴图片，非法字符直接拒绝）。
pub fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut table = [255u8; 256];
    for (index, byte) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[*byte as usize] = index as u8;
    }
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for byte in input.bytes() {
        if matches!(byte, b'=' | b'\n' | b'\r' | b' ') {
            continue;
        }
        let value = table[byte as usize];
        if value == 255 {
            return Err("图片数据编码无效".to_string());
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1u32 << bits) - 1;
        }
    }
    Ok(out)
}

/// 解析已完成传输的落盘路径（F23）。
async fn received_file_path(state: &AppState, transfer_id: &str) -> Result<PathBuf, String> {
    let info = state
        .transfers
        .get(transfer_id)
        .await
        .ok_or_else(|| format!("unknown transfer: {transfer_id}"))?;
    let raw = info.path.ok_or_else(|| "文件尚未接收完成".to_string())?;
    let path = PathBuf::from(raw);
    if !path.is_file() {
        return Err("文件不存在或已被移动".to_string());
    }
    Ok(path)
}

/// 用默认程序打开已接收的文件（F23）。
#[tauri::command]
pub async fn open_transfer_file(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let path = received_file_path(&state, &transfer_id).await?;

    // Android（M9）：没有 xdg-open，改用 FileProvider + ACTION_VIEW 拉起系统查看器。
    #[cfg(target_os = "android")]
    {
        let path_text = path.to_string_lossy().to_string();
        return tauri::async_runtime::spawn_blocking(move || {
            crate::platform::android::open_file(&path_text)
        })
        .await
        .map_err(|e| format!("无法打开文件：{e}"))?
        .map_err(|e| format!("无法打开文件：{e}"));
    }

    #[cfg(not(target_os = "android"))]
    {
        open_command(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开文件：{e}"))
    }
}

/// 在资源管理器中定位已接收的文件（F23）。
#[tauri::command]
pub async fn reveal_transfer_file(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let path = received_file_path(&state, &transfer_id).await?;

    // Android 没有“在文件管理器中定位”的概念，引导用户用另存为导出。
    #[cfg(target_os = "android")]
    {
        let _ = path;
        return Err("Android 不支持定位文件，请使用「另存为」保存到手机存储".to_string());
    }

    #[cfg(not(target_os = "android"))]
    {
        reveal_command(&path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开文件夹：{e}"))
    }
}

/// 另存为：选择目标路径后复制文件（F23），取消时返回 null。
#[tauri::command]
pub async fn save_transfer_as(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<Option<String>, String> {
    let source = received_file_path(&state, &transfer_id).await?;
    let default_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("chatters-file")
        .to_string();

    // Android（M9）：走 SAF（ACTION_CREATE_DOCUMENT）导出到用户选择的目录，
    // 私有目录里的原文件保留，便于再次导出或预览。
    #[cfg(target_os = "android")]
    {
        let _ = &app;
        let path_text = source.to_string_lossy().to_string();
        let file_name = default_name.clone();
        return tauri::async_runtime::spawn_blocking(move || {
            crate::platform::android::save_file(&path_text, &file_name)
        })
        .await
        .map_err(|e| format!("保存失败：{e}"))?
        .map_err(|e| format!("保存失败：{e}"));
    }

    #[cfg(not(target_os = "android"))]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog()
            .file()
            .set_file_name(default_name)
            .save_file(move |picked| {
                let _ = tx.send(picked);
            });
        let picked = rx.await.map_err(|_| "保存对话框已关闭".to_string())?;
        match picked {
            Some(file) => {
                let target = file.into_path().map_err(|e| e.to_string())?;
                tokio::fs::copy(&source, &target)
                    .await
                    .map_err(|e| format!("保存失败：{e}"))?;
                Ok(Some(target.to_string_lossy().to_string()))
            }
            None => Ok(None),
        }
    }
}

/// 删除下载目录内的历史文件（F41）：只删除确实存在且位于下载目录内的文件。
pub async fn delete_files_under(base: &Path, paths: &[String]) -> usize {
    let Ok(base) = tokio::fs::canonicalize(base).await else {
        return 0;
    };
    let mut removed = 0usize;
    for raw in paths {
        let Ok(target) = tokio::fs::canonicalize(raw).await else {
            continue;
        };
        if !target.starts_with(&base) {
            continue;
        }
        if tokio::fs::remove_file(&target).await.is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::{TransferDirection, TransferKind, TransferState};
    use std::net::SocketAddr;

    fn dto(id: &str, name: &str) -> DeviceDto {
        DeviceDto {
            device_id: id.to_string(),
            name: name.to_string(),
            addr: "192.168.1.10:28101".to_string(),
            tcp_port: 28101,
            fingerprint: fingerprint(id),
        }
    }

    /// 正常路径：指纹稳定、按 2 组 4 位十六进制展示，不同 ID 结果不同（v2/M10）。
    #[test]
    fn fingerprint_is_stable_and_grouped() {
        let code = fingerprint("device-1234");
        assert_eq!(code, fingerprint("device-1234"));
        assert_ne!(code, fingerprint("device-1235"));
        let parts: Vec<&str> = code.split('-').collect();
        assert_eq!(parts.len(), 2, "code = {code}");
        assert!(
            parts
                .iter()
                .all(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_hexdigit())),
            "code = {code}"
        );
    }

    /// 正常路径：设备 DTO 序列化包含 fingerprint（前端用于人工核对）。
    #[test]
    fn device_dto_includes_fingerprint() {
        let json = serde_json::to_value(dto("id-1", "PC-a")).expect("serialize");
        assert_eq!(json["fingerprint"], serde_json::json!(fingerprint("id-1")));
    }

    /// 正常路径：重连事件序列化为前端约定的 kind（v2/M10）。
    #[test]
    fn session_update_reconnecting_serializes() {
        let update = SessionUpdate::Reconnecting {
            peer_id: "peer-1".to_string(),
            attempt: 2,
        };
        let json = serde_json::to_value(update).expect("serialize");
        assert_eq!(json["kind"], "reconnecting");
        assert_eq!(json["peerId"], "peer-1");
        assert_eq!(json["attempt"], 2);

        let failed = SessionUpdate::ReconnectFailed {
            peer_id: "peer-1".to_string(),
        };
        let json = serde_json::to_value(failed).expect("serialize");
        assert_eq!(json["kind"], "reconnectFailed");
        assert_eq!(json["peerId"], "peer-1");
    }

    /// 正常路径：DiscoveryEvent 转成前端事件结构（camelCase + tag）。
    #[test]
    fn discovery_event_maps_to_update() {
        let entry = DeviceEntry {
            device_id: "id-1".to_string(),
            name: "PC-8f3a".to_string(),
            addr: "192.168.1.10:28101".parse::<SocketAddr>().unwrap(),
            tcp_port: 28101,
            last_seen: std::time::Instant::now(),
        };
        let update = DeviceUpdate::from(DiscoveryEvent::DeviceUp(entry));
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"kind\":\"up\""));
        assert!(json.contains("\"deviceId\":\"id-1\""));
        assert!(json.contains("\"tcpPort\":28101"));
        assert!(json.contains("\"addr\":\"192.168.1.10:28101\""));
    }

    /// 正常路径：设备下线事件字段为 camelCase（前端读 `update.deviceId`）。
    #[test]
    fn device_down_serializes_camel_case() {
        let update = DeviceUpdate::Down {
            device_id: "peer-1".to_string(),
        };
        let json = serde_json::to_string(&update).unwrap();
        assert_eq!(json, "{\"kind\":\"down\",\"deviceId\":\"peer-1\"}");
    }

    /// 正常路径：新设备插入并按昵称排序。
    #[test]
    fn apply_update_inserts_sorted() {
        let mut devices = Vec::new();
        apply_device_update(
            &mut devices,
            &DeviceUpdate::Up {
                device: dto("b", "PC-b"),
            },
        );
        apply_device_update(
            &mut devices,
            &DeviceUpdate::Up {
                device: dto("a", "PC-a"),
            },
        );
        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["PC-a", "PC-b"]);
    }

    /// 边界：同一设备重复上线只保留一条并刷新内容。
    #[test]
    fn apply_update_dedupes_by_device_id() {
        let mut devices = Vec::new();
        apply_device_update(
            &mut devices,
            &DeviceUpdate::Up {
                device: dto("a", "PC-old"),
            },
        );
        apply_device_update(
            &mut devices,
            &DeviceUpdate::Up {
                device: dto("a", "PC-new"),
            },
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "PC-new");
    }

    /// 异常/边界：下线事件移除对应设备，未知 ID 不 panic。
    #[test]
    fn apply_update_down_removes_device() {
        let mut devices = vec![dto("a", "PC-a"), dto("b", "PC-b")];
        apply_device_update(
            &mut devices,
            &DeviceUpdate::Down {
                device_id: "a".to_string(),
            },
        );
        apply_device_update(
            &mut devices,
            &DeviceUpdate::Down {
                device_id: "not-exist".to_string(),
            },
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, "b");
    }

    /// 正常路径：用发现到的 IP + TCP 端口组合出建连地址（不是 UDP 来源端口）。
    #[test]
    fn peer_tcp_addr_uses_tcp_port() {
        let device = DeviceDto {
            device_id: "id-1".to_string(),
            name: "PC-a".to_string(),
            addr: "192.168.1.10:50000".to_string(),
            tcp_port: 28101,
            fingerprint: fingerprint("id-1"),
        };
        assert_eq!(
            peer_tcp_addr(&device).map(|a| a.to_string()),
            Some("192.168.1.10:28101".to_string())
        );
    }

    /// 异常：地址字段不可解析时返回 None，不 panic。
    #[test]
    fn peer_tcp_addr_invalid_addr_returns_none() {
        let device = DeviceDto {
            device_id: "id-1".to_string(),
            name: "PC-a".to_string(),
            addr: "not-an-address".to_string(),
            tcp_port: 28101,
            fingerprint: fingerprint("id-1"),
        };
        assert!(peer_tcp_addr(&device).is_none());
    }

    /// 正常路径：发送状态序列化为前端约定的小写字符串。
    #[test]
    fn text_send_result_serializes() {
        let result = TextSendResultDto {
            msg_id: "m-1".to_string(),
            status: TextStatus::Delivered,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"msgId\":\"m-1\""));
        assert!(json.contains("\"status\":\"delivered\""));
        assert_eq!(
            serde_json::to_string(&TextStatus::Failed).unwrap(),
            "\"failed\""
        );
    }

    /// 正常路径：会话状态事件带 kind 标签，便于前端区分。
    #[test]
    fn session_update_serializes_with_kind() {
        let update = SessionUpdate::Disconnected {
            peer_id: "peer-1".to_string(),
        };
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains("\"kind\":\"disconnected\""));
        assert!(json.contains("\"peerId\":\"peer-1\""));
    }

    /// 正常路径：base64 编码与 RFC 4648 标准一致（含补位与多字节字符）。
    #[test]
    fn base64_encoding_matches_rfc4648() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_encode("中文".as_bytes()), "5Lit5paH");
    }

    /// 正常路径：base64 解码与编码互逆。
    #[test]
    fn base64_decode_roundtrip() {
        for sample in [&b""[..], b"f", b"fo", b"foo", b"foobar"] {
            let encoded = base64_encode(sample);
            assert_eq!(base64_decode(&encoded).unwrap(), sample);
        }
        assert_eq!(base64_decode("5Lit5paH").unwrap(), "中文".as_bytes());
    }

    /// 异常：非法字符或非法长度返回错误而不是 panic。
    #[test]
    fn base64_decode_rejects_invalid_input() {
        assert!(base64_decode("!!!!").is_err());
        assert!(base64_decode("AB$CD").is_err());
    }

    /// 正常路径：手动地址解析支持纯 IP、带端口与 IPv6 方括号。
    #[test]
    fn parse_manual_addr_supports_forms() {
        assert_eq!(
            parse_manual_addr("192.168.1.10", 28101)
                .unwrap()
                .to_string(),
            "192.168.1.10:28101"
        );
        assert_eq!(
            parse_manual_addr(" 192.168.1.10:30000 ", 28101)
                .unwrap()
                .to_string(),
            "192.168.1.10:30000"
        );
        assert_eq!(
            parse_manual_addr("[::1]:28101", 28101).unwrap().to_string(),
            "[::1]:28101"
        );
    }

    /// 异常：空输入与非法地址返回可读错误。
    #[test]
    fn parse_manual_addr_rejects_invalid() {
        assert!(parse_manual_addr("   ", 28101).is_err());
        assert!(parse_manual_addr("not-an-ip", 28101).is_err());
        assert!(parse_manual_addr("999.1.1.1", 28101).is_err());
    }

    /// 正常路径：只删除下载目录内的文件（F41 安全约束）。
    #[tokio::test]
    async fn delete_files_under_respects_base_dir() {
        let root = std::env::temp_dir().join("chatters_delete_under_test");
        let inside = root.join("inside.txt");
        let outside = std::env::temp_dir().join("chatters_outside_delete.txt");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(&inside, b"x").unwrap();
        std::fs::write(&outside, b"x").unwrap();

        let removed = delete_files_under(
            &root,
            &[
                inside.to_string_lossy().to_string(),
                outside.to_string_lossy().to_string(),
            ],
        )
        .await;

        assert_eq!(removed, 1, "只应删除下载目录内的文件");
        assert!(!inside.exists());
        assert!(outside.exists(), "下载目录外的文件必须保留");

        std::fs::remove_dir_all(&root).ok();
        std::fs::remove_file(&outside).ok();
    }

    /// 异常：目录不存在或路径失效时不 panic，返回 0。
    #[tokio::test]
    async fn delete_files_under_handles_missing_paths() {
        let missing_base = std::env::temp_dir().join("chatters_no_such_base_dir");
        let removed =
            delete_files_under(&missing_base, &["C:/definitely/not/here.txt".to_string()]).await;
        assert_eq!(removed, 0);
    }

    /// 正常路径：传输 DTO 字段为 camelCase、枚举为小写字符串（前端依赖）。
    #[test]
    fn transfer_dto_serializes() {
        let info = TransferInfo {
            transfer_id: "t-1".to_string(),
            peer_id: "peer-1".to_string(),
            direction: TransferDirection::Receive,
            kind: TransferKind::Image,
            filename: "photo.png".to_string(),
            size: 10,
            transferred: 4,
            state: TransferState::Transferring,
            path: None,
            error: None,
        };
        let json = serde_json::to_string(&TransferDto::from(&info)).unwrap();
        assert!(json.contains("\"transferId\":\"t-1\""));
        assert!(json.contains("\"direction\":\"receive\""));
        assert!(json.contains("\"kind\":\"image\""));
        assert!(json.contains("\"state\":\"transferring\""));
        assert!(json.contains("\"transferred\":4"));
    }
}
