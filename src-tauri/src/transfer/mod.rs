//! 传输任务管理（M3-a）：FILE_OFFER/ACCEPT/CHUNK/DONE/RESULT 主流程。
//!
//! 设计见 requirements/02-technical-design.md 第 4 节：
//! - 发送端先流式计算整文件 sha256，写入 FILE_OFFER；
//! - 接收端按 `kind` 决定是否自动接收（图片自动，文件需确认，F34）；
//! - 数据以 1MB 块经同一 TCP 连接传输，写入 `.part` 临时文件；
//! - FILE_DONE 后接收端比对 sha256，一致才改名为正式文件并回 FILE_RESULT。

pub mod chunkmap;
pub mod receiver;
pub mod sender;

use crate::protocol::{
    decode_chunk, FileAccept, FileBlockMap, FileCancel, FileDone, FileHave, FileKind, FileOffer,
    FileReject, FileResult, Frame, FrameType, ProtocolError,
};
use crate::session::{SessionError, SessionHub};
use chunkmap::{BlockBitmap, ChunkMap};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Mutex};

/// 单块数据大小（v1 默认 1MB）。
pub const CHUNK_SIZE: usize = 1024 * 1024;
/// 协商后的单块大小（v2 `chunk2m`：2MB，与协议上限一致）。
pub const CHUNK_SIZE_2M: usize = crate::protocol::MAX_CHUNK_DATA;
/// 等待接收方 ACCEPT/REJECT 的超时。
pub const OFFER_TIMEOUT: Duration = Duration::from_secs(30);
/// 等待接收方 FILE_RESULT 的超时。
pub const RESULT_TIMEOUT: Duration = Duration::from_secs(60);
/// 进度事件最小间隔（设计 §5：≥100ms，避免 UI 卡顿）。
pub const PROGRESS_THROTTLE: Duration = Duration::from_millis(100);
/// 传输空闲超时：超过该时长没有数据块即判定中断（M3-b）。
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
/// 空闲检查周期。
pub const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(1);

/// 传输类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    /// 图片（自动接收）。
    Image,
    /// 普通文件（需确认）。
    File,
}

impl TransferKind {
    /// 按扩展名判断类型（F20：jpg/png/gif/webp/bmp 视为图片）。
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .unwrap_or_default();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" => TransferKind::Image,
            _ => TransferKind::File,
        }
    }

    /// 转为协议层类型。
    pub fn to_protocol(self) -> FileKind {
        match self {
            TransferKind::Image => FileKind::Image,
            TransferKind::File => FileKind::File,
        }
    }

    /// 由协议层类型还原。
    pub fn from_protocol(kind: FileKind) -> Self {
        match kind {
            FileKind::Image => TransferKind::Image,
            FileKind::File => TransferKind::File,
        }
    }

    /// 稳定的字符串标识（用于前端 DTO）。
    pub fn as_str(self) -> &'static str {
        match self {
            TransferKind::Image => "image",
            TransferKind::File => "file",
        }
    }
}

/// 传输方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    /// 本机发送。
    Send,
    /// 本机接收。
    Receive,
}

impl TransferDirection {
    /// 稳定的字符串标识（用于前端 DTO）。
    pub fn as_str(self) -> &'static str {
        match self {
            TransferDirection::Send => "send",
            TransferDirection::Receive => "receive",
        }
    }
}

/// 传输状态机（设计 §5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferState {
    /// 等待用户确认（仅接收文件）。
    Pending,
    /// 已发出 OFFER，等待对端确认。
    Offered,
    /// 正在传输数据块。
    Transferring,
    /// 完成且校验通过。
    Done,
    /// 失败（含校验不一致）。
    Failed,
    /// 任一方取消。
    Cancelled,
    /// 接收方拒绝。
    Rejected,
}

impl TransferState {
    /// 是否为终态。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            TransferState::Done
                | TransferState::Failed
                | TransferState::Cancelled
                | TransferState::Rejected
        )
    }

    /// 稳定的字符串标识（用于前端 DTO）。
    pub fn as_str(self) -> &'static str {
        match self {
            TransferState::Pending => "pending",
            TransferState::Offered => "offered",
            TransferState::Transferring => "transferring",
            TransferState::Done => "done",
            TransferState::Failed => "failed",
            TransferState::Cancelled => "cancelled",
            TransferState::Rejected => "rejected",
        }
    }
}

/// 传输任务快照（推送给前端）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferInfo {
    /// 任务 ID。
    pub transfer_id: String,
    /// 对端设备 ID。
    pub peer_id: String,
    /// 方向。
    pub direction: TransferDirection,
    /// 类型。
    pub kind: TransferKind,
    /// 文件名。
    pub filename: String,
    /// 总字节数。
    pub size: u64,
    /// 已传输字节数。
    pub transferred: u64,
    /// 当前状态。
    pub state: TransferState,
    /// 接收完成后的落盘路径。
    pub path: Option<String>,
    /// 失败原因（面向用户，可为空）。
    pub error: Option<String>,
    /// 任务创建时间（Unix 毫秒）。
    ///
    /// 前端用它与文字消息按时间混排（v3：图片/文件不再单独置顶展示）。
    pub ts: i64,
}

/// 传输层事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
    /// 任务状态/进度更新。
    Updated(TransferInfo),
}

/// 传输配置。
#[derive(Debug, Clone)]
pub struct TransferConfig {
    /// 接收文件落盘目录（F34/F52）。
    pub download_dir: PathBuf,
    /// 是否自动接收图片（F34：图片自动、文件确认）。
    pub auto_accept_images: bool,
    /// 是否自动接收普通文件（F52 设置项，默认关闭）。
    pub auto_accept_files: bool,
    /// 空闲超时（超时判定中断，测试可缩短）。
    pub idle_timeout: Duration,
}

impl TransferConfig {
    /// 默认配置：图片自动接收、文件需确认、空闲超时 30 秒。
    pub fn new(download_dir: PathBuf, auto_accept_images: bool) -> Self {
        Self {
            download_dir,
            auto_accept_images,
            auto_accept_files: false,
            idle_timeout: IDLE_TIMEOUT,
        }
    }
}

/// 传输错误。
#[derive(Debug)]
pub enum TransferError {
    /// 本地文件读写失败。
    Io(std::io::Error),
    /// 帧编码/解码失败。
    Protocol(ProtocolError),
    /// 会话层失败（连接断开等）。
    Session(SessionError),
    /// 未知任务 ID。
    UnknownTransfer(String),
    /// 状态不允许该操作。
    InvalidState(String),
    /// 文件名非法（含路径穿越，rules.md R5.2）。
    UnsafeFileName(String),
    /// 源不是普通文件。
    SourceNotFile(String),
    /// 等待对端超时。
    Timeout(&'static str),
    /// 本地元数据 JSON 编解码失败。
    Json(serde_json::Error),
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::Io(e) => write!(f, "transfer io error: {e}"),
            TransferError::Protocol(e) => write!(f, "transfer protocol error: {e}"),
            TransferError::Session(e) => write!(f, "transfer session error: {e}"),
            TransferError::UnknownTransfer(id) => write!(f, "unknown transfer: {id}"),
            TransferError::InvalidState(msg) => write!(f, "invalid transfer state: {msg}"),
            TransferError::UnsafeFileName(name) => write!(f, "unsafe file name: {name}"),
            TransferError::SourceNotFile(path) => write!(f, "source is not a file: {path}"),
            TransferError::Timeout(what) => write!(f, "timed out waiting for {what}"),
            TransferError::Json(e) => write!(f, "transfer metadata json error: {e}"),
        }
    }
}

impl std::error::Error for TransferError {}

impl From<std::io::Error> for TransferError {
    fn from(e: std::io::Error) -> Self {
        TransferError::Io(e)
    }
}

impl From<ProtocolError> for TransferError {
    fn from(e: ProtocolError) -> Self {
        TransferError::Protocol(e)
    }
}

impl From<SessionError> for TransferError {
    fn from(e: SessionError) -> Self {
        TransferError::Session(e)
    }
}

impl From<serde_json::Error> for TransferError {
    fn from(e: serde_json::Error) -> Self {
        TransferError::Json(e)
    }
}

/// 单个传输任务（内部状态）。
pub(crate) struct Task {
    pub(crate) info: TransferInfo,
    pub(crate) expected_sha256: String,
    /// 发送端源文件。
    pub(crate) source: Option<PathBuf>,
    /// 接收端临时文件（`.part`）。
    pub(crate) part: Option<PathBuf>,
    /// 接收端最终路径。
    pub(crate) final_path: Option<PathBuf>,
    /// 接收端写入句柄。
    pub(crate) file: Option<tokio::fs::File>,
    /// 接收端 sha256 累加器。
    pub(crate) hasher: Option<Sha256>,
    pub(crate) accept_tx: Option<oneshot::Sender<u64>>,
    pub(crate) reject_tx: Option<oneshot::Sender<()>>,
    pub(crate) result_tx: Option<oneshot::Sender<FileResult>>,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) notify: Arc<tokio::sync::Notify>,
    pub(crate) last_emit: Instant,
    /// 最近一次有数据进展的时间（空闲超时判定）。
    pub(crate) last_activity: Instant,
    /// 接收端续传起点（已收到的字节数）。
    pub(crate) resume_from: u64,
    /// 接收端部分文件元数据路径（简版续传匹配用）。
    pub(crate) meta: Option<PathBuf>,
    /// 是否启用块级续传（双方声明 `chunkmap`，M11）。
    pub(crate) use_chunkmap: bool,
    /// 协商后的块大小（字节；1MB 或 2MB）。
    pub(crate) block_size: usize,
    /// 块表（发送端在 send_file 计算；接收端收到 FILE_BLOCK_MAP 后填充）。
    pub(crate) chunk_map: Option<ChunkMap>,
    /// 接收端块位图（M11）。
    pub(crate) bitmap: Option<BlockBitmap>,
    /// 接收端随机写句柄（M11；与 `file` 二选一）。
    pub(crate) chunk_file: Option<Arc<std::sync::Mutex<std::fs::File>>>,
    /// 距上次位图落盘的块数。
    pub(crate) blocks_since_flush: usize,
    /// 发送端等待接收方位图（FILE_HAVE）。
    pub(crate) have_tx: Option<oneshot::Sender<BlockBitmap>>,
}

/// 传输中心：任务表 + 帧路由。
#[derive(Clone)]
pub struct TransferManager {
    inner: Arc<Inner>,
}

/// 传输中心内部状态。
pub(crate) struct Inner {
    pub(crate) config: tokio::sync::RwLock<TransferConfig>,
    pub(crate) session: SessionHub,
    pub(crate) tasks: Mutex<HashMap<String, Task>>,
    events: mpsc::Sender<TransferEvent>,
}

impl Inner {
    /// 复制一份当前配置（避免在 await 中持有读锁）。
    pub(crate) async fn settings(&self) -> TransferConfig {
        self.config.read().await.clone()
    }

    /// 推送一条任务快照给上层。
    pub(crate) async fn emit(&self, info: TransferInfo) {
        if self
            .events
            .send(TransferEvent::Updated(info))
            .await
            .is_err()
        {
            log::warn!("transfer event channel closed");
        }
    }

    /// 更新任务状态并推送。
    pub(crate) async fn mark(
        &self,
        transfer_id: &str,
        state: TransferState,
        error: Option<String>,
    ) -> Option<TransferInfo> {
        let info = {
            let mut tasks = self.tasks.lock().await;
            let task = tasks.get_mut(transfer_id)?;
            task.info.state = state;
            if error.is_some() {
                task.info.error = error;
            }
            task.info.clone()
        };
        self.emit(info.clone()).await;
        Some(info)
    }

    /// 累加进度并按节流推送。
    pub(crate) async fn advance(&self, transfer_id: &str, delta: u64, force: bool) {
        let info = {
            let mut tasks = self.tasks.lock().await;
            let Some(task) = tasks.get_mut(transfer_id) else {
                return;
            };
            task.info.transferred = task.info.transferred.saturating_add(delta);
            let now = Instant::now();
            if !force && now.duration_since(task.last_emit) < PROGRESS_THROTTLE {
                return;
            }
            task.last_emit = now;
            task.info.clone()
        };
        self.emit(info).await;
    }

    /// 记录一次数据活动（用于空闲超时判定）。
    pub(crate) async fn touch(&self, transfer_id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(task) = tasks.get_mut(transfer_id) {
            task.last_activity = Instant::now();
        }
    }

    /// 发送一个帧给指定对端。
    pub(crate) async fn send(&self, peer_id: &str, frame: Frame) -> Result<(), TransferError> {
        self.session.send_frame(peer_id, frame).await?;
        Ok(())
    }
}

/// 计算文件 sha256（流式，阻塞 IO 放到 blocking 线程池）。
pub(crate) async fn hash_file(path: &Path) -> Result<String, TransferError> {
    let path = path.to_path_buf();
    let digest = tokio::task::spawn_blocking(move || -> Result<String, std::io::Error> {
        use std::io::Read;
        let mut file = std::fs::File::open(&path)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; CHUNK_SIZE];
        loop {
            let read = file.read(&mut buf)?;
            if read == 0 {
                break;
            }
            hasher.update(&buf[..read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    })
    .await
    .map_err(|e| TransferError::Io(std::io::Error::other(e)))??;
    Ok(digest)
}

/// 一次读取同时计算整文件 sha256 与块表（M11：避免为大文件多读一遍）。
pub(crate) async fn hash_file_with_blocks(
    path: &Path,
    block_size: usize,
) -> Result<(String, ChunkMap), TransferError> {
    let path = path.to_path_buf();
    let result =
        tokio::task::spawn_blocking(move || -> Result<(String, ChunkMap), std::io::Error> {
            use std::io::Read;
            let mut file = std::fs::File::open(&path)?;
            let mut hasher = Sha256::new();
            let mut map = ChunkMap::empty(block_size as u32).map_err(std::io::Error::other)?;
            let mut buf = vec![0u8; block_size];
            loop {
                let read = file.read(&mut buf)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buf[..read]);
                map.push_hash(&buf[..read]);
            }
            Ok((format!("{:x}", hasher.finalize()), map))
        })
        .await
        .map_err(|e| TransferError::Io(std::io::Error::other(e)))??;
    Ok(result)
}

/// 协商块大小：双方声明 `chunk2m` 用 2MB，否则 1MB（v1 兼容）。
pub(crate) async fn negotiated_block_size(inner: &Inner, peer_id: &str) -> usize {
    if inner
        .session
        .peer_features(peer_id)
        .await
        .iter()
        .any(|feature| feature == "chunk2m")
    {
        CHUNK_SIZE_2M
    } else {
        CHUNK_SIZE
    }
}

/// 是否启用块级续传：本机声明 `chunkmap` 且对端也声明（双方都声明才启用）。
pub(crate) async fn chunkmap_enabled(inner: &Inner, peer_id: &str) -> bool {
    crate::session::LOCAL_FEATURES.contains(&"chunkmap")
        && inner
            .session
            .peer_features(peer_id)
            .await
            .iter()
            .any(|feature| feature == "chunkmap")
}

/// 校验并提取安全的落盘文件名，拒绝路径穿越（rules.md R5.2）。
///
/// 平台差异（Windows 额外禁止 `: < > " | ? *`）由 `platform::file_name_allowed` 判定。
pub fn safe_file_name(raw: &str) -> Result<String, TransferError> {
    if !crate::platform::file_name_allowed(raw) {
        return Err(TransferError::UnsafeFileName(raw.to_string()));
    }
    Ok(raw.trim().to_string())
}

/// 目标文件已存在时生成带序号的新文件名，避免覆盖用户已有文件。
pub fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for index in 1..1000 {
        let name = if ext.is_empty() {
            format!("{stem} ({index})")
        } else {
            format!("{stem} ({index}).{ext}")
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-{}.{}", uuid::Uuid::new_v4().simple(), ext))
}

/// 路径转可展示字符串。
pub(crate) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// 按偏移与块大小计算块序号。
pub(crate) fn chunk_seq_with(offset: u64, chunk_size: usize) -> u32 {
    (offset / chunk_size as u64) as u32
}

/// 解析 FILE_CHUNK 帧，返回 (transfer_id, seq, 数据)。
pub(crate) fn decode_transfer_chunk(
    frame: &Frame,
) -> Result<(uuid::Uuid, u32, Vec<u8>), TransferError> {
    let (id, seq, data) = decode_chunk(&frame.payload)?;
    Ok((id, seq, data.to_vec()))
}

impl TransferManager {
    /// 创建传输中心。
    pub fn new(
        config: TransferConfig,
        session: SessionHub,
        events: mpsc::Sender<TransferEvent>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                config: tokio::sync::RwLock::new(config),
                session,
                tasks: Mutex::new(HashMap::new()),
                events,
            }),
        }
    }

    /// 更新接收设置（设置页保存后即时生效，F52）。
    pub async fn update_settings(
        &self,
        download_dir: PathBuf,
        auto_accept_images: bool,
        auto_accept_files: bool,
    ) {
        let mut config = self.inner.config.write().await;
        config.download_dir = download_dir;
        config.auto_accept_images = auto_accept_images;
        config.auto_accept_files = auto_accept_files;
    }

    /// 当前所有任务快照（按文件名排序）。
    pub async fn list(&self) -> Vec<TransferInfo> {
        let tasks = self.inner.tasks.lock().await;
        let mut items: Vec<TransferInfo> = tasks.values().map(|t| t.info.clone()).collect();
        items.sort_by(|a, b| a.filename.cmp(&b.filename));
        items
    }

    /// 取单个任务快照。
    pub async fn get(&self, transfer_id: &str) -> Option<TransferInfo> {
        let tasks = self.inner.tasks.lock().await;
        tasks.get(transfer_id).map(|t| t.info.clone())
    }

    /// 发送任务的本地源文件路径。
    ///
    /// 发送方预览图片用：源文件就在本地，不必等对端接收完成（v2/M9 真机问题）。
    pub async fn source_path(&self, transfer_id: &str) -> Option<String> {
        let tasks = self.inner.tasks.lock().await;
        tasks
            .get(transfer_id)
            .and_then(|t| t.source.as_ref())
            .map(|p| p.to_string_lossy().to_string())
    }

    /// 发送本地文件（图片或普通文件）。
    ///
    /// sha256 在阻塞线程池中流式计算后再发 OFFER；数据块发送在后台任务里执行，
    /// 因此本方法只等待"任务已创建并发出 OFFER"，不会阻塞界面。
    pub async fn send_file(
        &self,
        peer_id: &str,
        path: &Path,
    ) -> Result<TransferInfo, TransferError> {
        let metadata = tokio::fs::metadata(path).await?;
        if !metadata.is_file() {
            return Err(TransferError::SourceNotFile(path_to_string(path)));
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_string())
            .ok_or_else(|| TransferError::UnsafeFileName(path_to_string(path)))?;
        let kind = TransferKind::from_path(path);
        // M11：双方声明 chunkmap 时一次读取同时算整文件哈希与块表；否则沿用原流程。
        let block_size = negotiated_block_size(&self.inner, peer_id).await;
        let use_chunkmap = chunkmap_enabled(&self.inner, peer_id).await;
        let (sha256, chunk_map) = if use_chunkmap {
            let (sha, map) = hash_file_with_blocks(path, block_size).await?;
            (sha, Some(map))
        } else {
            (hash_file(path).await?, None)
        };
        let size = metadata.len();
        let transfer_id = uuid::Uuid::new_v4().to_string();

        let (accept_tx, accept_rx) = oneshot::channel();
        let (reject_tx, reject_rx) = oneshot::channel();
        let info = TransferInfo {
            transfer_id: transfer_id.clone(),
            peer_id: peer_id.to_string(),
            direction: TransferDirection::Send,
            kind,
            filename: filename.clone(),
            size,
            transferred: 0,
            state: TransferState::Offered,
            path: None,
            error: None,
            ts: crate::storage::now_millis(),
        };
        {
            let mut tasks = self.inner.tasks.lock().await;
            tasks.insert(
                transfer_id.clone(),
                Task {
                    info: info.clone(),
                    expected_sha256: sha256.clone(),
                    source: Some(path.to_path_buf()),
                    part: None,
                    final_path: None,
                    file: None,
                    hasher: None,
                    accept_tx: Some(accept_tx),
                    reject_tx: Some(reject_tx),
                    result_tx: None,
                    cancel: Arc::new(AtomicBool::new(false)),
                    notify: Arc::new(tokio::sync::Notify::new()),
                    last_emit: Instant::now(),
                    last_activity: Instant::now(),
                    resume_from: 0,
                    meta: None,
                    use_chunkmap,
                    block_size,
                    chunk_map,
                    bitmap: None,
                    chunk_file: None,
                    blocks_since_flush: 0,
                    have_tx: None,
                },
            );
        }

        let offer = FileOffer {
            transfer_id: transfer_id.clone(),
            kind: kind.to_protocol(),
            filename,
            size,
            sha256,
        };
        let offer_frame = Frame::json(FrameType::FileOffer, &offer)?;
        if let Err(e) = self.inner.send(peer_id, offer_frame).await {
            self.inner
                .mark(&transfer_id, TransferState::Failed, Some(e.to_string()))
                .await;
            return Err(e);
        }
        self.inner.emit(info.clone()).await;

        let inner = self.inner.clone();
        let id = transfer_id.clone();
        tokio::spawn(async move {
            if let Err(e) = sender::run(inner.clone(), id.clone(), accept_rx, reject_rx).await {
                log::warn!("transfer {id} failed: {e}");
                inner
                    .mark(&id, TransferState::Failed, Some(e.to_string()))
                    .await;
            }
        });
        Ok(info)
    }

    /// 接受接收中的文件传输（F34 用户确认后调用）。
    pub async fn accept(&self, transfer_id: &str) -> Result<(), TransferError> {
        receiver::accept(&self.inner, transfer_id).await
    }

    /// 拒绝接收（F34）。
    pub async fn reject(&self, transfer_id: &str) -> Result<(), TransferError> {
        let peer = {
            let tasks = self.inner.tasks.lock().await;
            let task = tasks
                .get(transfer_id)
                .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.to_string()))?;
            if task.info.state != TransferState::Pending {
                return Err(TransferError::InvalidState(format!(
                    "reject requires pending task, current: {}",
                    task.info.state.as_str()
                )));
            }
            task.info.peer_id.clone()
        };
        let frame = Frame::json(
            FrameType::FileReject,
            &FileReject {
                transfer_id: transfer_id.to_string(),
            },
        )?;
        self.inner.send(&peer, frame).await?;
        self.inner
            .mark(transfer_id, TransferState::Rejected, None)
            .await;
        receiver::cleanup_part(&self.inner, transfer_id).await;
        Ok(())
    }

    /// 取消传输（F33）。
    pub async fn cancel(&self, transfer_id: &str) -> Result<(), TransferError> {
        let peer = {
            let mut tasks = self.inner.tasks.lock().await;
            let task = tasks
                .get_mut(transfer_id)
                .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.to_string()))?;
            if task.info.state.is_terminal() {
                return Ok(());
            }
            task.cancel.store(true, Ordering::Relaxed);
            task.notify.notify_waiters();
            task.info.peer_id.clone()
        };
        let frame = Frame::json(
            FrameType::FileCancel,
            &FileCancel {
                transfer_id: transfer_id.to_string(),
            },
        )?;
        // 对端可能已离线：本地状态必须照常更新，失败只记录日志。
        if let Err(e) = self.inner.send(&peer, frame).await {
            log::warn!("failed to notify cancel for {transfer_id}: {e}");
        }
        self.inner
            .mark(transfer_id, TransferState::Cancelled, None)
            .await;
        receiver::cleanup_part(&self.inner, transfer_id).await;
        Ok(())
    }

    /// 重试失败的发送任务（M3-b）：以原源文件重新发起一次 OFFER。
    pub async fn retry(&self, transfer_id: &str) -> Result<TransferInfo, TransferError> {
        let (peer_id, source) = {
            let tasks = self.inner.tasks.lock().await;
            let task = tasks
                .get(transfer_id)
                .ok_or_else(|| TransferError::UnknownTransfer(transfer_id.to_string()))?;
            if task.info.direction != TransferDirection::Send {
                return Err(TransferError::InvalidState(
                    "retry requires a send task".to_string(),
                ));
            }
            let source = task
                .source
                .clone()
                .ok_or_else(|| TransferError::InvalidState("missing source path".to_string()))?;
            (task.info.peer_id.clone(), source)
        };
        self.send_file(&peer_id, &source).await
    }

    /// 启动空闲超时巡检（M3-b）：中断的任务标记失败并保留 `.part` 以便续传。
    pub fn spawn_maintenance(&self) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(MAINTENANCE_INTERVAL);
            loop {
                ticker.tick().await;
                let idle_timeout = inner.config.read().await.idle_timeout;
                let stale: Vec<String> = {
                    let tasks = inner.tasks.lock().await;
                    tasks
                        .values()
                        .filter(|task| {
                            task.info.state == TransferState::Transferring
                                && task.last_activity.elapsed() > idle_timeout
                        })
                        .map(|task| task.info.transfer_id.clone())
                        .collect()
                };
                for transfer_id in stale {
                    log::warn!("transfer {transfer_id} timed out (idle)");
                    receiver::detach_partial(&inner, &transfer_id).await;
                    inner
                        .mark(
                            &transfer_id,
                            TransferState::Failed,
                            Some("传输超时，可重试".to_string()),
                        )
                        .await;
                }
            }
        });
    }

    /// 对端断线时把相关未完成任务标记为失败（保留 `.part` 以便续传）。
    pub async fn handle_peer_disconnected(&self, peer_id: &str) {
        let affected: Vec<String> = {
            let tasks = self.inner.tasks.lock().await;
            tasks
                .values()
                .filter(|task| task.info.peer_id == peer_id && !task.info.state.is_terminal())
                .map(|task| task.info.transfer_id.clone())
                .collect()
        };
        for transfer_id in affected {
            receiver::detach_partial(&self.inner, &transfer_id).await;
            self.inner
                .mark(
                    &transfer_id,
                    TransferState::Failed,
                    Some("连接已断开，可重试".to_string()),
                )
                .await;
        }
    }

    /// 处理来自会话层的传输帧。
    pub async fn handle_frame(&self, peer_id: &str, frame: Frame) -> Result<(), TransferError> {
        match frame.frame_type {
            FrameType::FileOffer => {
                let offer: FileOffer = frame.parse_json()?;
                receiver::on_offer(self.inner.clone(), peer_id, offer).await
            }
            FrameType::FileAccept => {
                let accept: FileAccept = frame.parse_json()?;
                let sender = {
                    let mut tasks = self.inner.tasks.lock().await;
                    tasks
                        .get_mut(&accept.transfer_id)
                        .and_then(|t| t.accept_tx.take())
                };
                if let Some(tx) = sender {
                    let _ = tx.send(accept.resume_offset);
                }
                Ok(())
            }
            FrameType::FileReject => {
                let reject: FileReject = frame.parse_json()?;
                let sender = {
                    let mut tasks = self.inner.tasks.lock().await;
                    tasks
                        .get_mut(&reject.transfer_id)
                        .and_then(|t| t.reject_tx.take())
                };
                if let Some(tx) = sender {
                    let _ = tx.send(());
                }
                self.inner
                    .mark(&reject.transfer_id, TransferState::Rejected, None)
                    .await;
                Ok(())
            }
            FrameType::FileChunk => receiver::on_chunk(self.inner.clone(), &frame).await,
            FrameType::FileDone => {
                let done: FileDone = frame.parse_json()?;
                receiver::on_done(self.inner.clone(), done).await
            }
            FrameType::FileResult => {
                let result: FileResult = frame.parse_json()?;
                let waiter = {
                    let mut tasks = self.inner.tasks.lock().await;
                    tasks
                        .get_mut(&result.transfer_id)
                        .and_then(|t| t.result_tx.take())
                };
                if let Some(tx) = waiter {
                    let _ = tx.send(result);
                }
                Ok(())
            }
            // 块级续传帧（M11）：BLOCK_MAP 由接收端处理并回 HAVE；HAVE 交给等待中的发送任务。
            FrameType::FileBlockMap => {
                let map: FileBlockMap = frame.parse_json()?;
                receiver::on_block_map(self.inner.clone(), map).await
            }
            FrameType::FileHave => {
                let have: FileHave = frame.parse_json()?;
                let waiter = {
                    let mut tasks = self.inner.tasks.lock().await;
                    tasks.get_mut(&have.transfer_id).and_then(|task| {
                        let blocks = task
                            .chunk_map
                            .as_ref()
                            .map(|map| map.block_count())
                            .unwrap_or(0);
                        task.have_tx.take().map(|tx| (tx, blocks))
                    })
                };
                if let Some((tx, blocks)) = waiter {
                    if let Ok(bitmap) = BlockBitmap::from_hex(&have.bitmap, blocks) {
                        let _ = tx.send(bitmap);
                    }
                }
                Ok(())
            }
            // 重连恢复帧暂未使用（保留编号，避免与未来版本冲突）。
            FrameType::FileResumeState => Ok(()),
            FrameType::FileCancel => {
                let cancel: FileCancel = frame.parse_json()?;
                let notify = {
                    let mut tasks = self.inner.tasks.lock().await;
                    match tasks.get_mut(&cancel.transfer_id) {
                        Some(task) if !task.info.state.is_terminal() => {
                            task.cancel.store(true, Ordering::Relaxed);
                            task.notify.notify_waiters();
                            true
                        }
                        _ => false,
                    }
                };
                if notify {
                    self.inner
                        .mark(&cancel.transfer_id, TransferState::Cancelled, None)
                        .await;
                    receiver::cleanup_part(&self.inner, &cancel.transfer_id).await;
                }
                Ok(())
            }
            other => Err(TransferError::InvalidState(format!(
                "unexpected transfer frame: 0x{:02X}",
                other.as_u8()
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：常见图片扩展名识别为图片（F20），且大小写不敏感。
    #[test]
    fn kind_detection_by_extension() {
        for name in ["a.jpg", "b.JPEG", "c.png", "d.gif", "e.webp", "f.bmp"] {
            assert_eq!(
                TransferKind::from_path(Path::new(name)),
                TransferKind::Image,
                "{name} should be image"
            );
        }
        for name in ["doc.pdf", "archive.zip", "noext"] {
            assert_eq!(TransferKind::from_path(Path::new(name)), TransferKind::File);
        }
    }

    /// 正常路径：普通文件名原样通过校验。
    #[test]
    fn safe_file_name_accepts_plain_names() {
        assert_eq!(safe_file_name("photo.png").unwrap(), "photo.png");
        assert_eq!(safe_file_name("  报告 v2.pdf  ").unwrap(), "报告 v2.pdf");
    }

    /// 异常：路径穿越、绝对路径与空文件名必须被拒绝（rules.md R5.2）。
    #[test]
    fn safe_file_name_rejects_traversal() {
        for raw in [
            "../evil.txt",
            "..\\evil.txt",
            "dir/file.txt",
            "dir\\file.txt",
            "C:\\Windows\\evil.dll",
            "..",
            ".",
            "",
            "   ",
        ] {
            assert!(
                matches!(safe_file_name(raw), Err(TransferError::UnsafeFileName(_))),
                "{raw:?} should be rejected"
            );
        }
    }

    /// 边界：同名文件已存在时生成带序号的新名字，不覆盖已有文件。
    #[test]
    fn unique_path_avoids_overwrite() {
        let dir = std::env::temp_dir().join("chatters_unique_path_test");
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("report.pdf");
        std::fs::write(&first, b"old").unwrap();

        let picked = unique_path(&dir, "report.pdf");
        assert_ne!(picked, first);
        assert_eq!(
            picked.file_name().and_then(|n| n.to_str()),
            Some("report (1).pdf")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// 边界：块序号按 1MB / 2MB 两种块大小计算。
    #[test]
    fn chunk_seq_uses_chunk_size() {
        assert_eq!(chunk_seq_with(0, CHUNK_SIZE), 0);
        assert_eq!(chunk_seq_with(CHUNK_SIZE as u64 - 1, CHUNK_SIZE), 0);
        assert_eq!(chunk_seq_with(CHUNK_SIZE as u64, CHUNK_SIZE), 1);
        assert_eq!(chunk_seq_with((CHUNK_SIZE as u64) * 3 + 5, CHUNK_SIZE), 3);
        assert_eq!(chunk_seq_with(CHUNK_SIZE_2M as u64, CHUNK_SIZE_2M), 1);
        assert_eq!(
            chunk_seq_with((CHUNK_SIZE_2M as u64) * 2 + 7, CHUNK_SIZE_2M),
            2
        );
    }

    /// 正常路径：图片/文件与状态枚举的字符串标识稳定（前端依赖）。
    #[test]
    fn stable_string_ids() {
        assert_eq!(TransferKind::Image.as_str(), "image");
        assert_eq!(TransferKind::File.as_str(), "file");
        assert_eq!(TransferDirection::Send.as_str(), "send");
        assert_eq!(TransferDirection::Receive.as_str(), "receive");
        assert_eq!(TransferState::Pending.as_str(), "pending");
        assert_eq!(TransferState::Transferring.as_str(), "transferring");
        assert_eq!(TransferState::Done.as_str(), "done");
        assert!(TransferState::Failed.is_terminal());
        assert!(!TransferState::Transferring.is_terminal());
    }
}
