//! 前端可调用的 Tauri command 封装。
//!
//! - M2-a：设备发现（`list_devices` / `get_local_device` + `device://update`）；
//! - M2-b：文字消息（`send_text` + `message://new` / `session://update`）。
//!
//! 与 `src/api/types.ts` 的类型一一对应（rules.md R3.5），字段统一序列化为
//! camelCase，避免前端做字段名转换。

use crate::config::AppConfig;
use crate::discovery::{DeviceEntry, DiscoveryEvent, DiscoveryHandle};
use crate::errorlog::ErrorLog;
use crate::platform::shell::{open_path, reveal_command};
use crate::session::{SendStatus, SessionHub};
use crate::storage::{now_millis, History, HistoryStore, StoredMessage, StoredTransfer};
use crate::transfer::{
    TransferDirection, TransferInfo, TransferKind, TransferManager, TransferState,
};
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tauri::{Emitter, Manager};
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt as AutostartManagerExt;
use tauri_plugin_dialog::{DialogExt, FilePath};
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
    /// 运行平台：`android` | `windows` | `macos` | `linux`。
    ///
    /// 前端据此隐藏平台不支持的操作（如 Android 无「打开所在文件夹」）。
    pub platform: &'static str,
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
    /// 任务创建时间（Unix 毫秒），前端用于与文本消息按时间混排。
    pub ts: i64,
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
            ts: info.ts,
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
    /// 发现服务句柄：发送失败/切回前台时可主动触发重新发现。
    pub discovery: DiscoveryHandle,
    /// 错误日志（v3）：按天归档，设置页据此展示入口。
    pub error_log: Arc<ErrorLog>,
    /// 实际接收目录（Android 固定为 `<app_data>/download/chatter`，v3 清空历史时用它兜底）。
    pub receive_dir: PathBuf,
}

impl AppState {
    /// 以配置、会话、传输中心与历史存储创建状态，设备列表初始为空。
    // 仅在 lib.rs 启动处调用一次，参数即各子系统句柄，拆结构体反而降低可读性。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: AppConfig,
        config_path: PathBuf,
        session: SessionHub,
        transfers: TransferManager,
        history: HistoryStore,
        discovery: DiscoveryHandle,
        error_log: Arc<ErrorLog>,
        receive_dir: PathBuf,
    ) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
            devices: Arc::new(RwLock::new(Vec::new())),
            session,
            transfers,
            history,
            discovery,
            error_log,
            receive_dir,
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

/// 主动触发一次设备重新发现。
///
/// 场景（v2/M9）：Android 的 Wi-Fi 省电策略会过滤入站广播，设备可能被判定离线；
/// 切回前台或发送失败时调用本命令，立即补发 query 而不必等下一个周期。
#[tauri::command]
pub fn refresh_devices(state: tauri::State<'_, AppState>) {
    state.discovery.request_refresh();
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
        platform: crate::platform::name(),
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
    /// 是否开启定时清理（v3，移动端）。
    #[serde(default)]
    pub auto_clean_enabled: bool,
    /// 定时清理保留天数（v3，必须大于 0；缺省按 7 天，兼容旧客户端）。
    #[serde(default = "default_clean_days")]
    pub auto_clean_days: u32,
}

/// serde 默认值：定时清理保留天数。
fn default_clean_days() -> u32 {
    crate::config::DEFAULT_CLEAN_DAYS
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
    // 移动端下载目录固定为应用私有目录：这里直接回报实际接收目录，避免界面显示相对路径（v3）。
    #[cfg(target_os = "android")]
    let download_dir = state.receive_dir.to_string_lossy().to_string();
    #[cfg(not(target_os = "android"))]
    let download_dir = config.download_dir.clone();
    Ok(SettingsDto {
        device_id: config.device_id.clone(),
        nickname: config.nickname.clone(),
        discovery_port: config.discovery_port,
        tcp_port: config.tcp_port,
        download_dir,
        theme: config.theme.clone(),
        auto_accept_images: config.auto_accept_images,
        auto_accept_files: config.auto_accept_files,
        auto_clean_enabled: config.auto_clean_enabled,
        auto_clean_days: config.auto_clean_days,
    })
}

/// 校验定时清理天数（v3）：必须是大于 0 的整数（前端已限制为纯数字）。
fn validate_clean_days(days: u32) -> Result<(), String> {
    if days == 0 {
        return Err("清理天数必须大于 0".to_string());
    }
    Ok(())
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
    validate_clean_days(settings.auto_clean_days)?;
    let theme = if settings.theme == "dark" {
        "dark"
    } else {
        "light"
    };

    let (restart_required, saved) = {
        let mut config = state.config.write().await;
        // 昵称不需要重启：本机立即生效，其他设备在下次连接/应用重启后自然看到新名称（v3）。
        // 只有端口改动必须重启才能重新绑定。
        let restart_required = config.discovery_port != settings.discovery_port
            || config.tcp_port != settings.tcp_port;
        config.nickname = nickname;
        config.discovery_port = settings.discovery_port;
        config.tcp_port = settings.tcp_port;
        config.download_dir = settings.download_dir.trim().to_string();
        config.theme = theme.to_string();
        config.auto_accept_images = settings.auto_accept_images;
        config.auto_accept_files = settings.auto_accept_files;
        config.auto_clean_enabled = settings.auto_clean_enabled;
        config.auto_clean_days = settings.auto_clean_days;
        (restart_required, config.clone())
    };

    crate::config::save(&state.config_path, &saved).map_err(|e| e.to_string())?;
    // 桌面端跟随用户设置；Android 固定为实际接收目录（v3：避免切到相对路径后清空历史失效）。
    #[cfg(target_os = "android")]
    let receive_dir = state.receive_dir.clone();
    #[cfg(not(target_os = "android"))]
    let receive_dir = PathBuf::from(&saved.download_dir);
    state
        .transfers
        .update_settings(
            receive_dir,
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

/// 历史分页默认与上限（v3：向上无限滚动，默认 100 条/页）。
pub const DEFAULT_HISTORY_PAGE: u32 = 100;
/// 单页硬上限，避免前端一次拉取过多记录。
pub const MAX_HISTORY_PAGE: u32 = 500;

/// 历史时间线条目（消息或传输记录），供分页命令返回。
///
/// 标签用 `itemType`（而非 `kind`）：传输记录自身已有 `kind`（image/file）字段，
/// 复用会与前端联合类型判别冲突。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "itemType",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum HistoryItem {
    /// 文字消息。
    Message(StoredMessage),
    /// 传输记录。
    Transfer(StoredTransfer),
}

/// 历史分页结果（对应前端 `HistoryPage`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPageDto {
    /// 按时间升序的条目，最多 `limit` 条。
    pub items: Vec<HistoryItem>,
    /// 是否还有更早的记录。
    pub has_more: bool,
}

/// 合并消息与传输记录，取「比游标更早」的最新一页（纯函数，便于单元测试）。
///
/// `before_ts` 为 None 时取最新的 `limit` 条；返回是否还有更早的记录。
pub fn page_history(
    messages: Vec<StoredMessage>,
    transfers: Vec<StoredTransfer>,
    before_ts: Option<i64>,
    limit: usize,
) -> (Vec<HistoryItem>, bool) {
    let mut sorted: Vec<(i64, String, HistoryItem)> = Vec::new();
    for message in messages {
        if before_ts.is_none_or(|cursor| message.ts < cursor) {
            sorted.push((
                message.ts,
                format!("m-{}", message.msg_id),
                HistoryItem::Message(message),
            ));
        }
    }
    for transfer in transfers {
        if before_ts.is_none_or(|cursor| transfer.ts < cursor) {
            sorted.push((
                transfer.ts,
                format!("t-{}", transfer.transfer_id),
                HistoryItem::Transfer(transfer),
            ));
        }
    }
    // 时间升序 + key 兜底，保证同一毫秒内顺序稳定
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let total = sorted.len();
    let start = total.saturating_sub(limit);
    let items = sorted[start..]
        .iter()
        .map(|(_, _, item)| item.clone())
        .collect();
    (items, start > 0)
}

/// 分页读取历史（v3）：前端向上滚动时用 `before_ts` 逐页加载。
#[tauri::command]
pub async fn get_history_page(
    state: tauri::State<'_, AppState>,
    peer_id: Option<String>,
    before_ts: Option<i64>,
    limit: Option<u32>,
) -> Result<HistoryPageDto, String> {
    let peer = peer_id.as_deref();
    let messages = state.history.messages_for(peer).await;
    let transfers = state.history.transfers_for(peer).await;
    let limit = limit
        .unwrap_or(DEFAULT_HISTORY_PAGE)
        .clamp(1, MAX_HISTORY_PAGE) as usize;
    let (items, has_more) = page_history(messages, transfers, before_ts, limit);
    Ok(HistoryPageDto { items, has_more })
}

/// 删除目录内所有「单层普通文件」：不递归、不删子目录、不跟随符号链接。
///
/// 用于「清空全部历史 + 删除文件」：历史记录里没有指向的文件也应一并清理，
/// 避免磁盘残留文件与已清空的记录对不上（v3 需求 7）。目录不存在返回 0。
pub async fn purge_download_dir(dir: &Path) -> usize {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(_) => return 0,
    };
    let mut removed = 0usize;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let is_file = entry
            .file_type()
            .await
            .map(|kind| kind.is_file())
            .unwrap_or(false);
        if !is_file {
            continue;
        }
        if tokio::fs::remove_file(entry.path()).await.is_ok() {
            removed += 1;
        }
    }
    removed
}

/// 清空历史（F40/F41）：`peer_id` 为空时清空全部；`delete_files` 为 true 时删除文件。
///
/// - 指定 `peer_id`：只删除该会话记录里的接收文件；
/// - 清空全部（`peer_id = None`）且 `delete_files`：额外**清空下载目录内所有单层文件**（v3）。
#[tauri::command]
pub async fn clear_history(
    state: tauri::State<'_, AppState>,
    peer_id: Option<String>,
    delete_files: Option<bool>,
) -> Result<usize, String> {
    let peer = peer_id.as_deref();
    let mut removed = 0usize;
    if delete_files.unwrap_or(false) {
        // 用「实际接收目录」而不是配置值：Android 的配置里是相对路径，canonicalize 会失败（v3 真机问题）。
        let base = state.receive_dir.clone();
        let records = state.history.transfers_for(peer).await;
        let paths: Vec<String> = records
            .into_iter()
            .filter(|record| record.direction == "receive")
            .filter_map(|record| record.path)
            .collect();
        removed = delete_files_under(&base, &paths).await;
        // 清空全部历史时，把下载目录里的其它文件也清掉（子目录保留）。
        if peer.is_none() {
            removed += purge_download_dir(&base).await;
        }
    }
    state.history.clear(peer).await.map_err(|e| e.to_string())?;
    Ok(removed)
}

/// 选择下载目录（v3）：返回用户选择的绝对路径，取消时返回 `None`。
///
/// Android 的文件选择器不提供目录选择，直接返回错误（前端隐藏入口）。
#[tauri::command]
pub async fn pick_download_dir(app: tauri::AppHandle) -> Result<Option<String>, String> {
    #[cfg(target_os = "android")]
    {
        let _ = app;
        Err("Android 不支持选择目录".to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog().file().pick_folder(move |picked| {
            let _ = tx.send(picked);
        });
        let picked = rx
            .await
            .map_err(|_| "file dialog closed unexpectedly".to_string())?;
        match picked {
            Some(FilePath::Path(path)) => Ok(Some(path.to_string_lossy().to_string())),
            _ => Ok(None),
        }
    }
}

/// 取走系统分享内容（v3，Android `ACTION_SEND`）；无分享或桌面端返回 `None`。
#[tauri::command]
pub async fn take_share_payload() -> Result<Option<crate::share::SharePayload>, String> {
    Ok(crate::share::take_pending())
}

/// 把系统分享的 `content://` 导入缓存并返回本地路径（v3，Android）。
#[tauri::command]
pub async fn import_shared_file(uri: String) -> Result<String, String> {
    #[cfg(target_os = "android")]
    {
        tauri::async_runtime::spawn_blocking(move || crate::platform::android::import_file(&uri))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = uri;
        Err("桌面端不需要导入分享文件".to_string())
    }
}

/// 打开下载目录（v3 需求 6）：桌面端用系统文件管理器打开，目录不存在时先创建。
///
/// Android 的私有目录无法用文件管理器打开，返回明确错误（前端按平台隐藏入口）。
#[tauri::command]
pub async fn open_download_dir(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let dir = {
        let config = state.config.read().await;
        config.download_dir.trim().to_string()
    };
    if dir.is_empty() {
        return Err("下载目录未设置".to_string());
    }
    #[cfg(target_os = "android")]
    {
        let _ = dir;
        Err("Android 不支持打开下载文件夹".to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let path = PathBuf::from(&dir);
        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| e.to_string())?;
        // 打开外壳属于阻塞操作，放到阻塞线程池，避免占用异步运行时（v3）。
        tauri::async_runtime::spawn_blocking(move || open_path(&path))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/// 错误日志位置信息（v3）。
#[derive(Serialize)]
pub struct ErrorLogInfo {
    /// 日志根目录（按天建子目录）。
    pub dir: String,
    /// 今天的日志文件路径。
    pub file: String,
    /// 目录里是否已有日志（决定设置页是否展示入口）。
    pub has_logs: bool,
}

/// 组装日志信息。
fn error_log_info(state: &AppState) -> ErrorLogInfo {
    ErrorLogInfo {
        dir: state.error_log.root().to_string_lossy().to_string(),
        file: state.error_log.today_file().to_string_lossy().to_string(),
        has_logs: state.error_log.has_logs(),
    }
}

/// 上报一条错误并写入本地日志（前端统一兜底提示后调用）。
#[tauri::command]
pub async fn report_error(
    state: tauri::State<'_, AppState>,
    scope: String,
    message: String,
) -> Result<ErrorLogInfo, String> {
    state.error_log.record(&scope, &message)?;
    Ok(error_log_info(&state))
}

/// 读取错误日志位置与「是否已有日志」（设置页展示用）。
#[tauri::command]
pub async fn get_error_log(state: tauri::State<'_, AppState>) -> Result<ErrorLogInfo, String> {
    Ok(error_log_info(&state))
}

/// 打开错误日志目录（桌面端）；Android 没有文件管理器入口，返回明确错误。
#[tauri::command]
pub async fn open_error_log_dir(state: tauri::State<'_, AppState>) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let _ = &state;
        Err("Android 无法打开文件夹，请查看日志目录路径".to_string())
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = state.error_log.root().to_path_buf();
        tauri::async_runtime::spawn_blocking(move || open_path(&dir))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("无法打开错误日志目录：{e}"))
    }
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
    let ip = ip.trim_start_matches('[').trim_end_matches(']');
    // IPv6 链路本地地址可能带 zone id（`fe80::1%3`），`IpAddr` 不接受该后缀。
    let ip_text = ip.split('%').next().unwrap_or(ip);
    let ip: IpAddr = ip_text.parse().ok()?;
    Some(SocketAddr::new(ip, device.tcp_port))
}

/// 在设备表里按 device_id 解析 TCP 地址（纯函数，便于单元测试）。
pub fn resolve_device_addr(devices: &[DeviceDto], peer_id: &str) -> Option<SocketAddr> {
    devices
        .iter()
        .find(|d| d.device_id == peer_id)
        .and_then(peer_tcp_addr)
}

/// 触发重新发现后，等待对端单播回应的时长。
const REDISCOVER_WAIT: Duration = Duration::from_millis(600);

/// 解析发送目标：发现表优先，其次会话层记录的「上次成功握手地址」。
async fn lookup_peer_addr(state: &AppState, peer_id: &str) -> Option<SocketAddr> {
    let from_devices = {
        let devices = state.devices.read().await;
        resolve_device_addr(&devices, peer_id)
    };
    match from_devices {
        Some(addr) => Some(addr),
        None => state.session.known_addr(peer_id).await,
    }
}

/// 确保与目标设备存在会话（不存在时按发现结果建连），返回实际会话 ID。
///
/// 解析顺序（v2/M9 真机问题修复）：
/// 1. 已有会话；
/// 2. 发现表里的设备（正常路径）；
/// 3. 会话层记住的上次握手地址——移动端入站广播被系统过滤、设备被超时移除后，
///    仍可重连到同一台对端；
/// 4. 主动触发一次重新发现，等待 `REDISCOVER_WAIT` 后再试 2、3。
async fn ensure_session(state: &AppState, peer_id: &str) -> Result<String, String> {
    if state.session.is_connected(peer_id).await {
        return Ok(peer_id.to_string());
    }

    let mut target = lookup_peer_addr(state, peer_id).await;
    if target.is_none() {
        state.discovery.request_refresh();
        tokio::time::sleep(REDISCOVER_WAIT).await;
        target = lookup_peer_addr(state, peer_id).await;
    }

    let Some(target) = target else {
        let ids: Vec<String> = {
            let devices = state.devices.read().await;
            devices.iter().map(|d| d.device_id.clone()).collect()
        };
        // 只记录 ID，不输出消息内容或文件路径（rules.md R5.1）。
        log::warn!(
            "peer not found: peer_id={peer_id} discovered={} ids={ids:?}",
            ids.len()
        );
        return Err(format!("peer not found: {peer_id}"));
    };

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

/// 重发一条失败的文字消息（v3）：复用原 msg_id，历史只更新同一条记录。
///
/// 与 `send_text` 的区别：不生成新消息 ID，因此界面与 `history.json` 都保持一条，
/// ACK 丢失后的重发也不会让对端出现重复消息。
#[tauri::command]
pub async fn resend_text(
    state: tauri::State<'_, AppState>,
    msg_id: String,
) -> Result<TextSendResultDto, String> {
    let stored = state
        .history
        .find_message(&msg_id)
        .await
        .ok_or_else(|| "消息不存在，无法重发".to_string())?;
    let (peer_id, text) = resend_target(&stored)?;

    let session_id = ensure_session(&state, &peer_id).await?;
    let result = state
        .session
        .send_text_with_id(&session_id, &msg_id, &text)
        .await
        .map_err(|e| e.to_string())?;
    let status = match result.status {
        SendStatus::Delivered => "delivered",
        SendStatus::Failed => "failed",
    };
    // 同 msg_id 覆盖：状态 failed → delivered，时间戳沿用原记录（重启前后顺序一致）。
    if let Err(e) = state
        .history
        .append_message(StoredMessage {
            msg_id: result.msg_id.clone(),
            peer_id: session_id,
            direction: "out".to_string(),
            text,
            ts: stored.ts,
            status: status.to_string(),
        })
        .await
    {
        log::warn!("failed to persist resent message: {e}");
    }
    Ok(TextSendResultDto {
        msg_id: result.msg_id,
        status: TextStatus::from(result.status),
    })
}

/// 校验并取出重发所需的（对端, 文本）：只允许本机发出的非空消息（v3）。
fn resend_target(stored: &StoredMessage) -> Result<(String, String), String> {
    if stored.direction != "out" {
        return Err("只能重发本机发出的消息".to_string());
    }
    if stored.text.trim().is_empty() {
        return Err("消息内容为空".to_string());
    }
    Ok((stored.peer_id.clone(), stored.text.clone()))
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

/// 图片预览来源（纯逻辑，便于单元测试）。
///
/// 优先使用当前运行内的任务快照；重启后内存任务表为空，回退到历史记录
/// （v2/M9 真机问题：Windows 上重启后历史图片预览一直不可用）。
/// 错误字符串会原样返回前端，用于区分「未接收完成 / 记录缺失 / 文件丢失 / 过大」。
pub fn preview_source(
    running: Option<&TransferInfo>,
    source: Option<&str>,
    stored: Option<&StoredTransfer>,
) -> Result<(String, TransferKind), String> {
    if let Some(info) = running {
        if let Some(path) = info.path.clone() {
            return Ok((path, info.kind));
        }
        // 发送方向：源文件就在本地，随时可预览，不必等对端接收完成（v2/M9）。
        if info.direction == TransferDirection::Send {
            if let Some(path) = source {
                return Ok((path.to_string(), info.kind));
            }
        }
        if info.state != TransferState::Done {
            return Err("image not received yet".to_string());
        }
    }

    let Some(record) = stored else {
        return Err("unknown transfer".to_string());
    };
    if record.state != "done" {
        return Err("image not received yet".to_string());
    }
    // 接收方向用落盘路径；发送方向回退到历史里记录的源文件（v3：重进会话后可预览）。
    let path = record.path.clone().or_else(|| record.source_path.clone());
    let Some(path) = path else {
        return Err("image not received yet".to_string());
    };
    Ok((path, TransferKind::from_path(Path::new(&record.filename))))
}

/// 读取已接收图片为 data URL，供聊天窗内直接预览（F23）。
///
/// 采用 base64 内联以避开额外的资源协议与权限配置；超过 20MB 不内联预览
/// （大图请用「打开文件」查看，后续可用 asset 协议流式预览）。
#[tauri::command]
pub async fn get_image_preview(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<String, String> {
    const MAX_PREVIEW_BYTES: u64 = 20 * 1024 * 1024;

    let running = state.transfers.get(&transfer_id).await;
    let source = state.transfers.source_path(&transfer_id).await;
    let stored = state.history.find_transfer(&transfer_id).await;
    let (path, kind) = preview_source(running.as_ref(), source.as_deref(), stored.as_ref())
        .map_err(|reason| {
            log::warn!("image preview unavailable: transfer_id={transfer_id} reason={reason}");
            reason
        })?;

    if kind != TransferKind::Image {
        log::warn!("image preview rejected: transfer_id={transfer_id} kind=file");
        return Err("not an image".to_string());
    }

    let metadata = tokio::fs::metadata(&path).await.map_err(|e| {
        log::warn!("image preview file missing: transfer_id={transfer_id} error={e}");
        "preview file unavailable".to_string()
    })?;
    if metadata.len() > MAX_PREVIEW_BYTES {
        log::warn!(
            "image preview too large: transfer_id={transfer_id} bytes={}",
            metadata.len()
        );
        return Err("image too large for preview".to_string());
    }

    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        log::warn!("image preview read failed: transfer_id={transfer_id} error={e}");
        "preview file unavailable".to_string()
    })?;
    Ok(format!(
        "data:{};base64,{}",
        image_mime(Path::new(&path)),
        base64_encode(&bytes)
    ))
}

/// 返回图片的本地绝对路径（v3：桌面端用 asset 协议流式预览，避免 base64 IPC 与 20MB 限制）。
///
/// Android 端不使用该命令（WebView 对 asset 协议支持不一致），仍走 `get_image_preview`。
#[tauri::command]
pub async fn get_image_path(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<String, String> {
    let running = state.transfers.get(&transfer_id).await;
    let source = state.transfers.source_path(&transfer_id).await;
    let stored = state.history.find_transfer(&transfer_id).await;
    let (path, kind) = preview_source(running.as_ref(), source.as_deref(), stored.as_ref())?;
    if kind != TransferKind::Image {
        return Err("not an image".to_string());
    }
    Ok(path)
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
    let Some(file) = picked else {
        return Ok(None);
    };
    match file {
        FilePath::Path(path) => Ok(Some(path.to_string_lossy().to_string())),
        // Android 的文件选择器只返回 `content://` Uri，Rust 读不了：
        // 交给 Kotlin 复制到应用缓存目录后再按普通路径发送（v2/M9 真机问题）。
        FilePath::Url(url) => {
            #[cfg(target_os = "android")]
            {
                let uri = url.to_string();
                let path = tauri::async_runtime::spawn_blocking(move || {
                    crate::platform::android::import_file(&uri)
                })
                .await
                .map_err(|e| e.to_string())??;
                Ok(Some(path))
            }
            #[cfg(not(target_os = "android"))]
            {
                let _ = url;
                Err("unsupported file path".to_string())
            }
        }
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

/// 从内存任务与历史记录中挑选可打开的本地路径（v3；纯函数便于单测）。
///
/// - 接收方向用落盘路径；
/// - 发送方向优先内存里的源路径（本次运行内发送的），再退回历史的 `source_path`；
/// - 内存里没有该任务（应用重启后从历史加载）时按历史记录的方向取值。
///
/// 返回 `None` 表示没有任何可用路径（例如尚未接收完成）。
fn pick_local_path(
    direction: Option<TransferDirection>,
    live_path: Option<String>,
    memory_source: Option<String>,
    stored: Option<&StoredTransfer>,
) -> Option<String> {
    match direction {
        Some(TransferDirection::Receive) => live_path,
        Some(TransferDirection::Send) => {
            memory_source.or_else(|| stored.and_then(|s| s.source_path.clone()))
        }
        None => match stored {
            Some(s) if s.direction == "send" => s.source_path.clone(),
            Some(s) => s.path.clone(),
            None => None,
        },
    }
}

/// 解析「打开 / 定位 / 另存为」要用的本地文件路径（v3）。
///
/// 内存任务优先（本次运行），其次历史记录（重启后仍能找回自己发出的文件）；
/// 源文件被移动/删除时返回 `file unavailable`，由前端按方向给出不同提示。
async fn transfer_local_path(state: &AppState, transfer_id: &str) -> Result<PathBuf, String> {
    let info = state.transfers.get(transfer_id).await;
    let direction = info.as_ref().map(|item| item.direction);
    let live_path = info.and_then(|item| item.path);
    let memory_source = if direction == Some(TransferDirection::Send) {
        state.transfers.source_path(transfer_id).await
    } else {
        None
    };
    let stored = state.history.find_transfer(transfer_id).await;
    let known = direction.is_some() || stored.is_some();

    let raw =
        pick_local_path(direction, live_path, memory_source, stored.as_ref()).ok_or_else(|| {
            if known {
                "file not available".to_string()
            } else {
                format!("unknown transfer: {transfer_id}")
            }
        })?;

    let path = PathBuf::from(raw);
    if !path.is_file() {
        return Err("file unavailable".to_string());
    }
    Ok(path)
}

/// 用默认程序打开已完成的传输文件（F23 / v3）。
///
/// 发送方打开本地源文件（随时可找回自己发出的文件），接收方打开落盘文件；
/// 源文件被移动或删除时返回 `file unavailable`，由前端按方向提示。
#[tauri::command]
pub async fn open_transfer_file(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let path = transfer_local_path(&state, &transfer_id).await?;

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
        // 打开外壳属于阻塞操作，放到阻塞线程池，避免占用异步运行时（v3）。
        tauri::async_runtime::spawn_blocking(move || open_path(&path))
            .await
            .map_err(|e| format!("无法打开文件：{e}"))?
            .map_err(|e| format!("无法打开文件：{e}"))
    }
}

/// 在资源管理器中定位已接收的文件（F23）。
#[tauri::command]
pub async fn reveal_transfer_file(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let path = transfer_local_path(&state, &transfer_id).await?;

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
    let source = transfer_local_path(&state, &transfer_id).await?;
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

    /// 构造一条历史传输记录（v3 路径解析用）。
    fn stored(direction: &str, path: Option<&str>, source_path: Option<&str>) -> StoredTransfer {
        StoredTransfer {
            transfer_id: "t-1".to_string(),
            peer_id: "peer-1".to_string(),
            direction: direction.to_string(),
            kind: "file".to_string(),
            filename: "report.pdf".to_string(),
            size: 10,
            transferred: 10,
            state: "done".to_string(),
            path: path.map(|value| value.to_string()),
            source_path: source_path.map(|value| value.to_string()),
            ts: 1,
        }
    }

    /// 正常路径：接收方向用落盘路径，发送方向优先内存源路径。
    #[test]
    fn pick_local_path_prefers_live_task() {
        assert_eq!(
            pick_local_path(
                Some(TransferDirection::Receive),
                Some("C:/dl/report.pdf".to_string()),
                None,
                None
            ),
            Some("C:/dl/report.pdf".to_string())
        );
        assert_eq!(
            pick_local_path(
                Some(TransferDirection::Send),
                None,
                Some("C:/src/report.pdf".to_string()),
                None
            ),
            Some("C:/src/report.pdf".to_string())
        );
    }

    /// 边界：内存任务没有源路径时退回历史 `source_path`（重启后仍可打开发出的文件）。
    #[test]
    fn pick_local_path_falls_back_to_history_source() {
        let record = stored("send", None, Some("C:/src/old.pdf"));
        assert_eq!(
            pick_local_path(Some(TransferDirection::Send), None, None, Some(&record)),
            Some("C:/src/old.pdf".to_string())
        );
    }

    /// 边界：任务不在内存（重启后从历史加载）时按历史方向取值。
    #[test]
    fn pick_local_path_uses_history_when_task_absent() {
        let send = stored("send", None, Some("C:/src/sent.pdf"));
        let receive = stored("receive", Some("C:/dl/got.pdf"), None);
        assert_eq!(
            pick_local_path(None, None, None, Some(&send)),
            Some("C:/src/sent.pdf".to_string())
        );
        assert_eq!(
            pick_local_path(None, None, None, Some(&receive)),
            Some("C:/dl/got.pdf".to_string())
        );
    }

    /// 异常：既无内存任务也无历史记录（或记录里没有路径）时返回 None。
    #[test]
    fn pick_local_path_returns_none_without_paths() {
        assert_eq!(pick_local_path(None, None, None, None), None);
        let pending = stored("receive", None, None);
        assert_eq!(pick_local_path(None, None, None, Some(&pending)), None);
    }

    /// 正常路径：定时清理天数必须大于 0（v3）。
    #[test]
    fn validate_clean_days_accepts_positive() {
        assert!(validate_clean_days(1).is_ok());
        assert!(validate_clean_days(7).is_ok());
        assert!(validate_clean_days(365).is_ok());
    }

    /// 异常：天数为 0 时拒绝保存并给出中文提示。
    #[test]
    fn validate_clean_days_rejects_zero() {
        assert_eq!(
            validate_clean_days(0),
            Err("清理天数必须大于 0".to_string())
        );
    }

    /// 构造一条历史消息（v3 重发校验用）。
    fn resend_message(direction: &str, text: &str) -> StoredMessage {
        StoredMessage {
            msg_id: "m-1".to_string(),
            peer_id: "peer-1".to_string(),
            direction: direction.to_string(),
            text: text.to_string(),
            ts: 1,
            status: "failed".to_string(),
        }
    }

    /// 正常路径：本机发出的非空消息可以重发，且携带原 msg_id 所需的对端与文本。
    #[test]
    fn resend_target_accepts_outgoing() {
        let target = resend_target(&resend_message("out", "hello")).expect("accept");
        assert_eq!(target, ("peer-1".to_string(), "hello".to_string()));
    }

    /// 异常：收到的消息不能重发；空白文本也拒绝。
    #[test]
    fn resend_target_rejects_incoming_and_empty() {
        assert_eq!(
            resend_target(&resend_message("in", "hello")),
            Err("只能重发本机发出的消息".to_string())
        );
        assert_eq!(
            resend_target(&resend_message("out", "   ")),
            Err("消息内容为空".to_string())
        );
    }

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
            ts: 1_700_000_000_000,
        };
        let json = serde_json::to_string(&TransferDto::from(&info)).unwrap();
        assert!(json.contains("\"transferId\":\"t-1\""));
        assert!(json.contains("\"direction\":\"receive\""));
        assert!(json.contains("\"kind\":\"image\""));
        assert!(json.contains("\"state\":\"transferring\""));
        assert!(json.contains("\"transferred\":4"));
        assert!(json.contains("\"ts\":1700000000000"));
    }

    /// 边界：IPv6 方括号与链路本地 zone id 都能解析（否则会退化成 peer not found）。
    #[test]
    fn peer_tcp_addr_handles_ipv6() {
        assert_eq!(
            peer_tcp_addr(&device("id-2", "[fd00::1]:5000", 28101))
                .unwrap()
                .to_string(),
            "[fd00::1]:28101"
        );
        assert_eq!(
            peer_tcp_addr(&device("id-3", "[fe80::1%3]:5000", 28101))
                .unwrap()
                .ip()
                .to_string(),
            "fe80::1"
        );
    }

    /// 异常：非法地址或缺少端口时返回 None（调用方据此给出 peer not found）。
    #[test]
    fn peer_tcp_addr_rejects_malformed() {
        assert!(peer_tcp_addr(&device("id-4", "not-an-address", 28101)).is_none());
        assert!(peer_tcp_addr(&device("id-5", "192.168.1.10", 28101)).is_none());
    }

    /// 正常路径/异常：按 device_id 在设备表里解析地址。
    #[test]
    fn resolve_device_addr_matches_device() {
        let devices = vec![
            device("id-1", "192.168.1.10:5000", 28101),
            device("id-2", "192.168.1.11:5000", 28102),
        ];
        assert_eq!(
            resolve_device_addr(&devices, "id-2").unwrap().to_string(),
            "192.168.1.11:28102"
        );
        assert!(resolve_device_addr(&devices, "missing").is_none());
    }

    /// 正常路径：内存任务已完成时直接取落盘路径。
    #[test]
    fn preview_source_prefers_running_task() {
        let running = preview_task(
            TransferDirection::Receive,
            TransferState::Done,
            Some("C:/dl/photo.png"),
            TransferKind::Image,
        );
        let (path, kind) = preview_source(Some(&running), None, None).unwrap();
        assert_eq!(path, "C:/dl/photo.png");
        assert_eq!(kind, TransferKind::Image);
    }

    /// 正常路径：发送方向用本地源文件，传输未完成也能预览（v2/M9 真机问题）。
    #[test]
    fn preview_source_uses_source_for_send() {
        let sending = preview_task(
            TransferDirection::Send,
            TransferState::Transferring,
            None,
            TransferKind::Image,
        );
        let (path, kind) = preview_source(Some(&sending), Some("C:/pics/a.png"), None).unwrap();
        assert_eq!(path, "C:/pics/a.png");
        assert_eq!(kind, TransferKind::Image);

        // 发送任务缺少源路径（例如重启后）时仍按「未接收完成」处理。
        assert_eq!(
            preview_source(Some(&sending), None, None).unwrap_err(),
            "image not received yet"
        );
    }

    /// 正常路径：发送方向的历史记录用 `source_path` 预览（v3：重进会话后图片不再加载失败）。
    #[test]
    fn preview_source_uses_stored_source_path_for_send() {
        let mut record = stored_preview("done", None, "sent.png");
        record.direction = "send".to_string();
        record.source_path = Some("C:/pics/sent.png".to_string());

        let (path, kind) = preview_source(None, None, Some(&record)).unwrap();
        assert_eq!(path, "C:/pics/sent.png");
        assert_eq!(kind, TransferKind::Image);
    }

    /// 异常：接收完成但既无落盘路径也无源路径时仍报「未接收完成」。
    #[test]
    fn preview_source_requires_some_path() {
        let record = stored_preview("done", None, "x.png");
        assert_eq!(
            preview_source(None, None, Some(&record)).unwrap_err(),
            "image not received yet"
        );
    }

    /// 正常路径：重启后内存任务表为空，回退历史记录（F23 真机问题修复）。
    #[test]
    fn preview_source_falls_back_to_history() {
        let stored = stored_preview("done", Some("C:/dl/old.jpg"), "old.jpg");
        let (path, kind) = preview_source(None, None, Some(&stored)).unwrap();
        assert_eq!(path, "C:/dl/old.jpg");
        assert_eq!(kind, TransferKind::Image);
    }

    /// 异常：未接收完成 / 记录缺失 / 历史状态非 done 都给出可区分的原因。
    #[test]
    fn preview_source_reports_reasons() {
        let transferring = preview_task(
            TransferDirection::Receive,
            TransferState::Transferring,
            None,
            TransferKind::Image,
        );
        assert_eq!(
            preview_source(Some(&transferring), None, None).unwrap_err(),
            "image not received yet"
        );

        assert_eq!(
            preview_source(None, None, None).unwrap_err(),
            "unknown transfer"
        );

        let failed = stored_preview("failed", Some("C:/dl/x.png"), "x.png");
        assert_eq!(
            preview_source(None, None, Some(&failed)).unwrap_err(),
            "image not received yet"
        );

        let no_path = stored_preview("done", None, "x.png");
        assert_eq!(
            preview_source(None, None, Some(&no_path)).unwrap_err(),
            "image not received yet"
        );
    }

    /// 构造一条历史消息（测试辅助）。
    fn stored_message(id: &str, ts: i64) -> StoredMessage {
        StoredMessage {
            msg_id: id.to_string(),
            peer_id: "peer-1".to_string(),
            direction: "in".to_string(),
            text: format!("msg {id}"),
            ts,
            status: "delivered".to_string(),
        }
    }

    /// 构造一条历史传输记录（测试辅助）。
    fn stored_transfer(id: &str, ts: i64) -> StoredTransfer {
        StoredTransfer {
            transfer_id: id.to_string(),
            peer_id: "peer-1".to_string(),
            direction: "receive".to_string(),
            kind: "image".to_string(),
            filename: format!("{id}.png"),
            size: 1,
            transferred: 1,
            state: "done".to_string(),
            path: Some(format!("C:/dl/{id}.png")),
            source_path: None,
            ts,
        }
    }

    /// 正常路径：不带游标返回最新一页，按时间升序，并标记还有更早记录。
    #[test]
    fn page_history_returns_latest_page() {
        let messages: Vec<StoredMessage> = (1..=5_i64)
            .map(|i| stored_message(&format!("m{i}"), i))
            .collect();
        let (items, has_more) = page_history(messages, Vec::new(), None, 3);

        assert_eq!(items.len(), 3);
        assert!(has_more);
        let ids: Vec<String> = items
            .iter()
            .map(|item| match item {
                HistoryItem::Message(message) => message.msg_id.clone(),
                HistoryItem::Transfer(transfer) => transfer.transfer_id.clone(),
            })
            .collect();
        assert_eq!(ids, vec!["m3", "m4", "m5"]);
    }

    /// 正常路径：消息与传输按时间合并排序（同一时间戳用 key 兜底）。
    #[test]
    fn page_history_merges_messages_and_transfers() {
        let messages = vec![stored_message("m1", 10), stored_message("m2", 30)];
        let transfers = vec![stored_transfer("t1", 20)];
        let (items, has_more) = page_history(messages, transfers, None, 10);

        assert!(!has_more);
        let order: Vec<String> = items
            .iter()
            .map(|item| match item {
                HistoryItem::Message(message) => message.msg_id.clone(),
                HistoryItem::Transfer(transfer) => transfer.transfer_id.clone(),
            })
            .collect();
        assert_eq!(order, vec!["m1", "t1", "m2"]);
    }

    /// 正常路径：用最老时间戳作游标翻页，不重复、不遗漏。
    #[test]
    fn page_history_uses_cursor_without_overlap() {
        let messages: Vec<StoredMessage> = (1..=6_i64)
            .map(|i| stored_message(&format!("m{i}"), i))
            .collect();

        let (newest, has_more) = page_history(messages.clone(), Vec::new(), None, 2);
        assert!(has_more);
        let oldest_ts = match &newest[0] {
            HistoryItem::Message(message) => message.ts,
            HistoryItem::Transfer(transfer) => transfer.ts,
        };

        let (older, has_more_older) = page_history(messages, Vec::new(), Some(oldest_ts), 2);
        assert!(has_more_older);
        match (&older[0], &older[1]) {
            (HistoryItem::Message(a), HistoryItem::Message(b)) => {
                assert_eq!(a.msg_id, "m3");
                assert_eq!(b.msg_id, "m4");
            }
            other => panic!("unexpected items: {other:?}"),
        }
    }

    /// 边界：记录数不超过一页时 has_more 为 false；空历史返回空页。
    #[test]
    fn page_history_reports_end_of_history() {
        let (items, has_more) = page_history(
            vec![stored_message("m1", 1)],
            Vec::new(),
            None,
            DEFAULT_HISTORY_PAGE as usize,
        );
        assert_eq!(items.len(), 1);
        assert!(!has_more);

        let (empty, no_more) = page_history(Vec::new(), Vec::new(), None, 100);
        assert!(empty.is_empty());
        assert!(!no_more);
    }

    /// 异常：游标早于所有记录时返回空页且无更早记录。
    #[test]
    fn page_history_before_earliest_returns_empty() {
        let (items, has_more) =
            page_history(vec![stored_message("m1", 100)], Vec::new(), Some(1), 10);
        assert!(items.is_empty());
        assert!(!has_more);
    }

    /// 正常路径：清空下载目录只删单层普通文件，子目录内容保留（v3 需求 7）。
    #[tokio::test]
    async fn purge_download_dir_removes_files_only() {
        let root = std::env::temp_dir().join("chatters_purge_test");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("a.txt"), b"x").unwrap();
        std::fs::write(root.join("b.png"), b"y").unwrap();
        std::fs::write(root.join("sub/keep.txt"), b"z").unwrap();

        let removed = purge_download_dir(&root).await;

        assert_eq!(removed, 2);
        assert!(!root.join("a.txt").exists());
        assert!(!root.join("b.png").exists());
        assert!(root.join("sub/keep.txt").exists(), "子目录内容必须保留");

        std::fs::remove_dir_all(&root).ok();
    }

    /// 异常：下载目录不存在时返回 0，不 panic。
    #[tokio::test]
    async fn purge_download_dir_missing_returns_zero() {
        let missing = std::env::temp_dir().join("chatters_no_such_download_dir");
        assert_eq!(purge_download_dir(&missing).await, 0);
    }

    /// 构造一条设备 DTO（测试辅助）。
    fn device(id: &str, addr: &str, tcp_port: u16) -> DeviceDto {
        DeviceDto {
            device_id: id.to_string(),
            name: format!("PC-{id}"),
            addr: addr.to_string(),
            tcp_port,
            fingerprint: fingerprint(id),
        }
    }

    /// 构造一条内存中的传输快照（测试辅助）。
    fn preview_task(
        direction: TransferDirection,
        state: TransferState,
        path: Option<&str>,
        kind: TransferKind,
    ) -> TransferInfo {
        TransferInfo {
            transfer_id: "t-1".to_string(),
            peer_id: "peer-1".to_string(),
            direction,
            kind,
            filename: "photo.png".to_string(),
            size: 10,
            transferred: 10,
            state,
            path: path.map(|p| p.to_string()),
            error: None,
            ts: 1,
        }
    }

    /// 构造一条历史传输记录（测试辅助）。
    fn stored_preview(state: &str, path: Option<&str>, filename: &str) -> StoredTransfer {
        StoredTransfer {
            transfer_id: "t-1".to_string(),
            peer_id: "peer-1".to_string(),
            direction: "receive".to_string(),
            kind: "image".to_string(),
            filename: filename.to_string(),
            size: 10,
            transferred: 10,
            state: state.to_string(),
            path: path.map(|p| p.to_string()),
            source_path: None,
            ts: 1,
        }
    }
}
