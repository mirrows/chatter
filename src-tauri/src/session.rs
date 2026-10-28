//! TCP 会话模块（M2-b）：监听、HELLO 握手、文字消息与 ACK、心跳。
//!
//! 设计见 requirements/02-technical-design.md 第 4 节。要点：
//! - 每个对端一个 TCP 连接，连接建立后双方先交换 HELLO；
//! - 文字消息发送后等待 TEXT_ACK，超时（默认 3 秒）视为失败；
//! - 心跳帧（0x1F）为单次通知，收到后不回应，避免双向回环。

use crate::protocol::{
    read_frame, write_frame, Frame, FrameType, Hello, ProtocolError, TextAck, TextMessage,
};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::timeout;

/// ACK 超时：超时未收到回执即认为发送失败（设计 4.3）。
pub const DEFAULT_ACK_TIMEOUT: Duration = Duration::from_secs(3);
/// 心跳发送间隔（设计 4.2）。
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// 会话层配置。
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// 本机唯一 ID。
    pub device_id: String,
    /// 本机昵称。
    pub name: String,
    /// TCP 监听端口。
    pub tcp_port: u16,
    /// ACK 等待时长（测试可缩短）。
    pub ack_timeout: Duration,
}

/// 会话事件，由 Tauri 层转成前端事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// 与某设备建立会话（完成 HELLO）。
    PeerConnected {
        /// 对端设备 ID。
        device_id: String,
        /// 对端昵称。
        name: String,
        /// 对端 TCP 地址。
        addr: SocketAddr,
    },
    /// 会话断开。
    PeerDisconnected {
        /// 对端设备 ID。
        device_id: String,
    },
    /// 收到文字消息。
    TextReceived {
        /// 发送方设备 ID。
        device_id: String,
        /// 消息 ID。
        msg_id: String,
        /// 消息内容。
        text: String,
        /// 发送时间（Unix 毫秒）。
        ts: i64,
    },
    /// 收到传输相关帧（FILE_*），交由传输模块处理。
    TransferFrame {
        /// 对端设备 ID。
        device_id: String,
        /// 原始帧。
        frame: Frame,
    },
}

/// 文字消息发送状态（F11）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendStatus {
    /// 收到 TEXT_ACK。
    Delivered,
    /// ACK 超时。
    Failed,
}

/// `send_text` 的返回结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSendResult {
    /// 本次消息 ID。
    pub msg_id: String,
    /// 发送状态。
    pub status: SendStatus,
}

/// 会话层错误。
#[derive(Debug)]
pub enum SessionError {
    /// 网络 IO 失败。
    Io(std::io::Error),
    /// 协议错误（非法帧、JSON 解析失败等）。
    Protocol(ProtocolError),
    /// 握手阶段收到非 HELLO 帧。
    UnexpectedFrame(FrameType),
    /// 对端在握手完成前关闭连接。
    HandshakeClosed,
    /// 目标设备未建立会话。
    NotConnected(String),
    /// 写入任务已结束（连接已断开）。
    ChannelClosed,
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::Io(e) => write!(f, "session io error: {e}"),
            SessionError::Protocol(e) => write!(f, "session protocol error: {e}"),
            SessionError::UnexpectedFrame(t) => {
                write!(f, "expected HELLO, got frame type 0x{:02X}", t.as_u8())
            }
            SessionError::HandshakeClosed => write!(f, "peer closed before handshake finished"),
            SessionError::NotConnected(id) => write!(f, "peer not connected: {id}"),
            SessionError::ChannelClosed => write!(f, "peer connection writer is closed"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<std::io::Error> for SessionError {
    fn from(e: std::io::Error) -> Self {
        SessionError::Io(e)
    }
}

impl From<ProtocolError> for SessionError {
    fn from(e: ProtocolError) -> Self {
        SessionError::Protocol(e)
    }
}

/// 对端连接句柄：写入通道。
struct PeerHandle {
    sender: mpsc::Sender<Frame>,
}

/// 已握手完成的对端信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerInfo {
    /// 对端设备 ID。
    pub device_id: String,
    /// 对端昵称。
    pub name: String,
}

struct HubInner {
    device_id: String,
    name: String,
    ack_timeout: Duration,
    peers: Mutex<HashMap<String, PeerHandle>>,
    pending_acks: Mutex<HashMap<String, oneshot::Sender<()>>>,
    events: mpsc::Sender<SessionEvent>,
}

/// 会话中心：监听入站连接、管理出站连接、收发文字消息。
#[derive(Clone)]
pub struct SessionHub {
    inner: Arc<HubInner>,
}

/// 当前 Unix 毫秒时间戳。
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 启动 TCP 监听并返回会话中心与事件接收端。
///
/// 同时启动两个后台任务：accept 循环与心跳广播（30 秒一次）。
pub async fn start_session(
    config: SessionConfig,
) -> Result<
    (
        SessionHub,
        mpsc::Receiver<SessionEvent>,
        tokio::task::JoinHandle<()>,
    ),
    SessionError,
> {
    let listener = TcpListener::bind(("0.0.0.0", config.tcp_port)).await?;
    let (event_tx, event_rx) = mpsc::channel::<SessionEvent>(64);
    let inner = Arc::new(HubInner {
        device_id: config.device_id,
        name: config.name,
        ack_timeout: config.ack_timeout,
        peers: Mutex::new(HashMap::new()),
        pending_acks: Mutex::new(HashMap::new()),
        events: event_tx,
    });
    let hub = SessionHub {
        inner: inner.clone(),
    };

    let accept_inner = inner.clone();
    let handle = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let inner = accept_inner.clone();
                    tokio::spawn(async move {
                        if let Err(e) = establish(inner, stream, addr).await {
                            // 日志只记录地址与错误类型，不记录消息内容（R5.1）。
                            log::warn!("incoming session handshake failed from {addr}: {e}");
                        }
                    });
                }
                Err(e) => {
                    log::error!("tcp accept failed: {e}");
                    break;
                }
            }
        }
    });

    let heartbeat_inner = inner;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL);
        loop {
            ticker.tick().await;
            heartbeat_inner.broadcast_heartbeat().await;
        }
    });

    Ok((hub, event_rx, handle))
}

impl HubInner {
    /// 向所有已连接对端发送心跳（无对端时为空操作）。
    async fn broadcast_heartbeat(&self) {
        let senders: Vec<mpsc::Sender<Frame>> = {
            let peers = self.peers.lock().await;
            peers.values().map(|p| p.sender.clone()).collect()
        };
        for sender in senders {
            let _ = sender.send(Frame::empty(FrameType::Heartbeat)).await;
        }
    }

    /// 发送一个帧到指定对端。
    async fn send_frame(&self, peer_id: &str, frame: Frame) -> Result<(), SessionError> {
        let sender = {
            let peers = self.peers.lock().await;
            peers
                .get(peer_id)
                .map(|p| p.sender.clone())
                .ok_or_else(|| SessionError::NotConnected(peer_id.to_string()))?
        };
        sender
            .send(frame)
            .await
            .map_err(|_| SessionError::ChannelClosed)
    }

    /// 处理一个来自对端的帧。
    async fn dispatch(&self, peer_id: &str, frame: Frame) -> Result<(), SessionError> {
        match frame.frame_type {
            FrameType::Text => {
                let message: TextMessage = frame.parse_json()?;
                let _ = self
                    .events
                    .send(SessionEvent::TextReceived {
                        device_id: peer_id.to_string(),
                        msg_id: message.msg_id.clone(),
                        text: message.text,
                        ts: message.ts,
                    })
                    .await;
                // 回执失败不终止读循环：对端可能刚好断开。
                let ack = Frame::json(
                    FrameType::TextAck,
                    &TextAck {
                        msg_id: message.msg_id,
                    },
                )?;
                if let Err(e) = self.send_frame(peer_id, ack).await {
                    log::warn!("failed to send text ack to {peer_id}: {e}");
                }
            }
            FrameType::TextAck => {
                let ack: TextAck = frame.parse_json()?;
                if let Some(sender) = self.pending_acks.lock().await.remove(&ack.msg_id) {
                    let _ = sender.send(());
                }
            }
            // 心跳仅用于对端活跃度，收到后不回应（避免双向回环）。
            FrameType::Heartbeat => {}
            // 传输帧交给上层传输模块（FILE_OFFER/CHUNK/DONE 等）。
            FrameType::FileOffer
            | FrameType::FileAccept
            | FrameType::FileReject
            | FrameType::FileCancel
            | FrameType::FileChunk
            | FrameType::FileDone
            | FrameType::FileResult => {
                let _ = self
                    .events
                    .send(SessionEvent::TransferFrame {
                        device_id: peer_id.to_string(),
                        frame,
                    })
                    .await;
            }
            // 握手完成后不应再出现 HELLO，忽略以保持连接可用。
            FrameType::Hello => {}
        }
        Ok(())
    }
}

impl SessionHub {
    /// 向对端发起连接并完成 HELLO 握手。
    pub async fn connect(&self, addr: SocketAddr) -> Result<PeerInfo, SessionError> {
        let stream = TcpStream::connect(addr).await?;
        establish(self.inner.clone(), stream, addr).await
    }

    /// 是否已与指定设备建立会话。
    pub async fn is_connected(&self, peer_id: &str) -> bool {
        self.inner.peers.lock().await.contains_key(peer_id)
    }

    /// 向指定对端发送任意帧（传输模块复用同一连接）。
    pub async fn send_frame(&self, peer_id: &str, frame: Frame) -> Result<(), SessionError> {
        self.inner.send_frame(peer_id, frame).await
    }

    /// 发送文字消息并等待 ACK（F10/F11）。
    ///
    /// 返回 `Failed` 表示 ACK 超时，调用方可提示重发；连接不存在时返回 `NotConnected`。
    pub async fn send_text(
        &self,
        peer_id: &str,
        text: &str,
    ) -> Result<TextSendResult, SessionError> {
        let msg_id = uuid::Uuid::new_v4().to_string();
        let message = TextMessage {
            msg_id: msg_id.clone(),
            text: text.to_string(),
            ts: now_millis(),
        };
        let frame = Frame::json(FrameType::Text, &message)?;

        let (ack_tx, ack_rx) = oneshot::channel();
        self.inner
            .pending_acks
            .lock()
            .await
            .insert(msg_id.clone(), ack_tx);

        if let Err(e) = self.inner.send_frame(peer_id, frame).await {
            self.inner.pending_acks.lock().await.remove(&msg_id);
            return Err(e);
        }

        let status = match timeout(self.inner.ack_timeout, ack_rx).await {
            Ok(Ok(())) => SendStatus::Delivered,
            // 超时或回执通道被丢弃：视为发送失败。
            _ => {
                self.inner.pending_acks.lock().await.remove(&msg_id);
                SendStatus::Failed
            }
        };
        Ok(TextSendResult { msg_id, status })
    }
}

/// 与一个 TCP 流完成握手并登记对端，随后启动读/写任务。
async fn establish(
    inner: Arc<HubInner>,
    stream: TcpStream,
    addr: SocketAddr,
) -> Result<PeerInfo, SessionError> {
    let (mut reader, mut writer) = stream.into_split();

    // 双向 HELLO：先发本机身份，再等对端身份。
    let hello = Frame::json(FrameType::Hello, &Hello::new(&inner.device_id, &inner.name))?;
    write_frame(&mut writer, &hello).await?;

    let first = read_frame(&mut reader)
        .await?
        .ok_or(SessionError::HandshakeClosed)?;
    if first.frame_type != FrameType::Hello {
        return Err(SessionError::UnexpectedFrame(first.frame_type));
    }
    let peer: Hello = first.parse_json()?;

    // 写入任务：通道关闭即退出（PeerHandle 被移除时发送端随之释放）。
    let (tx, mut rx) = mpsc::channel::<Frame>(64);
    inner
        .peers
        .lock()
        .await
        .insert(peer.device_id.clone(), PeerHandle { sender: tx });

    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if let Err(e) = write_frame(&mut writer, &frame).await {
                log::warn!("session write failed: {e}");
                break;
            }
        }
    });

    // 读任务：循环处理帧，结束后清理对端并通知上层。
    let reader_inner = inner.clone();
    let peer_id = peer.device_id.clone();
    tokio::spawn(async move {
        loop {
            match read_frame(&mut reader).await {
                Ok(Some(frame)) => {
                    if let Err(e) = reader_inner.dispatch(&peer_id, frame).await {
                        log::warn!("session dispatch failed: {e}");
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    log::warn!("session read failed: {e}");
                    break;
                }
            }
        }
        reader_inner.peers.lock().await.remove(&peer_id);
        let _ = reader_inner
            .events
            .send(SessionEvent::PeerDisconnected { device_id: peer_id })
            .await;
    });

    let _ = inner
        .events
        .send(SessionEvent::PeerConnected {
            device_id: peer.device_id.clone(),
            name: peer.name.clone(),
            addr,
        })
        .await;

    Ok(PeerInfo {
        device_id: peer.device_id,
        name: peer.name,
    })
}
