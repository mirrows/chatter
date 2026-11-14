//! TCP 帧协议（M2-b）。
//!
//! 协议见 requirements/02-technical-design.md 第 4 节：
//! - 帧头：magic(5B) + type(1B) + length(4B, 大端) + payload(length 字节)；
//! - magic 固定为 `CHTRS`，单帧长度上限 4MB（超长帧必须拒绝，rules.md R5.3）。
//!
//! 本模块只做编解码，不涉及连接管理，便于独立单元测试。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

/// 帧头 magic（与发现协议的 magic 一致，便于统一识别本程序流量）。
pub const MAGIC: &[u8; 5] = b"CHTRS";
/// 帧头长度：magic(5) + type(1) + length(4)。
pub const HEADER_LEN: usize = 10;
/// 单帧 payload 上限（4MB）。
pub const MAX_FRAME_LEN: u32 = 4 * 1024 * 1024;

/// 帧类型（对应设计文档 4.2 节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameType {
    /// 建连后首个帧，携带 device_id / name / ver。
    Hello = 0x01,
    /// 文字消息。
    Text = 0x02,
    /// 文字消息送达回执。
    TextAck = 0x03,
    /// 传输请求（图片或文件）。
    FileOffer = 0x10,
    /// 接受传输（携带续传偏移）。
    FileAccept = 0x11,
    /// 拒绝传输。
    FileReject = 0x12,
    /// 取消传输。
    FileCancel = 0x13,
    /// 文件数据块。
    FileChunk = 0x14,
    /// 传输结束（携带整文件 sha256）。
    FileDone = 0x15,
    /// 校验结果。
    FileResult = 0x16,
    /// 块表（M11：每块哈希前 8 字节，v3/后续启用）。
    FileBlockMap = 0x17,
    /// 已收到块位图（M11：接收方回传缺失情况）。
    FileHave = 0x18,
    /// 续传状态（M11：重连后恢复位图/偏移）。
    FileResumeState = 0x19,
    /// 心跳（单次通知，收到后不回应，见设计文档 4.2）。
    Heartbeat = 0x1F,
}

impl FrameType {
    /// 由字节还原帧类型；未知类型返回 None。
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(FrameType::Hello),
            0x02 => Some(FrameType::Text),
            0x03 => Some(FrameType::TextAck),
            0x10 => Some(FrameType::FileOffer),
            0x11 => Some(FrameType::FileAccept),
            0x12 => Some(FrameType::FileReject),
            0x13 => Some(FrameType::FileCancel),
            0x14 => Some(FrameType::FileChunk),
            0x15 => Some(FrameType::FileDone),
            0x16 => Some(FrameType::FileResult),
            0x17 => Some(FrameType::FileBlockMap),
            0x18 => Some(FrameType::FileHave),
            0x19 => Some(FrameType::FileResumeState),
            0x1F => Some(FrameType::Heartbeat),
            _ => None,
        }
    }

    /// 帧类型字节值。
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// 协议错误。
#[derive(Debug)]
pub enum ProtocolError {
    /// 底层读写失败（含连接被对端关闭导致的半包）。
    Io(std::io::Error),
    /// magic 不匹配：不是本协议的连接。
    BadMagic([u8; 5]),
    /// 帧长度超过上限。
    FrameTooLarge(u32),
    /// 未知帧类型。
    UnknownType(u8),
    /// FILE_CHUNK payload 结构非法（长度不足）。
    BadChunkPayload(usize),
    /// payload JSON 编解码失败。
    Json(serde_json::Error),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::Io(e) => write!(f, "protocol io error: {e}"),
            ProtocolError::BadMagic(m) => write!(f, "bad frame magic: {m:?}"),
            ProtocolError::FrameTooLarge(len) => {
                write!(f, "frame too large: {len} > {MAX_FRAME_LEN}")
            }
            ProtocolError::UnknownType(t) => write!(f, "unknown frame type: 0x{t:02X}"),
            ProtocolError::BadChunkPayload(len) => {
                write!(f, "invalid file chunk payload length: {len}")
            }
            ProtocolError::Json(e) => write!(f, "frame payload json error: {e}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<std::io::Error> for ProtocolError {
    fn from(e: std::io::Error) -> Self {
        ProtocolError::Io(e)
    }
}

impl From<serde_json::Error> for ProtocolError {
    fn from(e: serde_json::Error) -> Self {
        ProtocolError::Json(e)
    }
}

/// 一个完整帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 帧类型。
    pub frame_type: FrameType,
    /// payload 原始字节。
    pub payload: Vec<u8>,
}

impl Frame {
    /// 以原始字节构造帧。
    pub fn new(frame_type: FrameType, payload: Vec<u8>) -> Self {
        Self {
            frame_type,
            payload,
        }
    }

    /// 空 payload 帧（心跳）。
    pub fn empty(frame_type: FrameType) -> Self {
        Self {
            frame_type,
            payload: Vec::new(),
        }
    }

    /// 把 `value` 序列化为 JSON 作为 payload。
    pub fn json<T: Serialize>(frame_type: FrameType, value: &T) -> Result<Self, ProtocolError> {
        Ok(Self {
            frame_type,
            payload: serde_json::to_vec(value)?,
        })
    }

    /// 把 payload 反序列化为 JSON 结构。
    pub fn parse_json<T: DeserializeOwned>(&self) -> Result<T, ProtocolError> {
        Ok(serde_json::from_slice(&self.payload)?)
    }

    /// 编码为完整字节（帧头 + payload）。
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let len = u32::try_from(self.payload.len())
            .map_err(|_| ProtocolError::FrameTooLarge(u32::MAX))?;
        if len > MAX_FRAME_LEN {
            return Err(ProtocolError::FrameTooLarge(len));
        }
        let mut out = Vec::with_capacity(HEADER_LEN + self.payload.len());
        out.extend_from_slice(MAGIC);
        out.push(self.frame_type.as_u8());
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }
}

/// 从字节流读取一帧；对端在帧边界正常关闭时返回 `Ok(None)`。
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Frame>, ProtocolError> {
    let mut header = [0u8; HEADER_LEN];
    // 单独读第一个字节：读到 0 字节才是"干净关闭"，读一半属于半包错误。
    let read = reader.read(&mut header[..1]).await?;
    if read == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut header[1..]).await?;

    let mut magic = [0u8; 5];
    magic.copy_from_slice(&header[..5]);
    if &magic != MAGIC {
        return Err(ProtocolError::BadMagic(magic));
    }

    let frame_type = FrameType::from_u8(header[5]).ok_or(ProtocolError::UnknownType(header[5]))?;
    let len = u32::from_be_bytes([header[6], header[7], header[8], header[9]]);
    if len > MAX_FRAME_LEN {
        // 先校验长度再分配内存，避免超长帧导致 OOM（rules.md R5.3）。
        return Err(ProtocolError::FrameTooLarge(len));
    }

    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload).await?;
    Ok(Some(Frame::new(frame_type, payload)))
}

/// 写出一个帧并 flush。
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    frame: &Frame,
) -> Result<(), ProtocolError> {
    let bytes = frame.encode()?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// 握手帧 JSON（HELLO）。
///
/// `features` 为 v2 引入的可选能力协商字段（v1 端无此字段，按空数组处理）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    /// 发送方唯一 ID。
    pub device_id: String,
    /// 发送方昵称。
    pub name: String,
    /// 协议版本。
    pub ver: u8,
    /// 能力声明（v2，缺省为空，保证 v1 兼容）。
    #[serde(default)]
    pub features: Vec<String>,
    /// 发送方 TCP 监听端口（v2，用于断线自动重连；v1 端为空）。
    #[serde(default)]
    pub tcp_port: Option<u16>,
}

impl Hello {
    /// 以本机身份构造 HELLO（无能力声明，v1 兼容格式）。
    pub fn new(device_id: &str, name: &str) -> Self {
        Self {
            device_id: device_id.to_string(),
            name: name.to_string(),
            ver: 1,
            features: Vec::new(),
            tcp_port: None,
        }
    }

    /// 以本机身份 + 能力声明 + TCP 端口构造 HELLO（v2）。
    pub fn with_features(
        device_id: &str,
        name: &str,
        features: Vec<String>,
        tcp_port: u16,
    ) -> Self {
        Self {
            device_id: device_id.to_string(),
            name: name.to_string(),
            ver: 1,
            features,
            tcp_port: Some(tcp_port),
        }
    }

    /// 是否声明了某项能力。
    pub fn supports(&self, feature: &str) -> bool {
        self.features.iter().any(|item| item == feature)
    }
}

/// 文字消息 payload（TEXT）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextMessage {
    /// 消息 ID（uuid）。
    pub msg_id: String,
    /// 文本内容（保留换行）。
    pub text: String,
    /// 发送时间（Unix 毫秒）。
    pub ts: i64,
}

/// 文字消息回执 payload（TEXT_ACK）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextAck {
    /// 被确认的消息 ID。
    pub msg_id: String,
}

/// 传输类型：图片自动接收，普通文件需确认（F34）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    /// 图片（jpg/png/gif/webp/bmp）。
    Image,
    /// 普通文件。
    File,
}

/// FILE_OFFER payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileOffer {
    /// 传输任务 ID。
    pub transfer_id: String,
    /// 传输类型。
    pub kind: FileKind,
    /// 原始文件名（不含路径）。
    pub filename: String,
    /// 文件总字节数。
    pub size: u64,
    /// 整文件 sha256（十六进制小写）。
    pub sha256: String,
}

/// FILE_ACCEPT payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileAccept {
    /// 传输任务 ID。
    pub transfer_id: String,
    /// 已确认接收的字节数（简版续传从该偏移继续）。
    pub resume_offset: u64,
}

/// FILE_REJECT payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileReject {
    /// 传输任务 ID。
    pub transfer_id: String,
}

/// FILE_CANCEL payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCancel {
    /// 传输任务 ID。
    pub transfer_id: String,
}

/// FILE_DONE payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDone {
    /// 传输任务 ID。
    pub transfer_id: String,
    /// 发送端计算的整文件 sha256。
    pub sha256: String,
}

/// FILE_RESULT payload。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResult {
    /// 传输任务 ID。
    pub transfer_id: String,
    /// 校验是否通过。
    pub ok: bool,
    /// 接收端落盘路径（仅成功时有值，供发送端展示）。
    pub path: Option<String>,
    /// 失败原因分类（M11：`missing_blocks` / `block_hash_mismatch` / `file_hash_mismatch`）。
    #[serde(default)]
    pub error: Option<String>,
}

/// 每块哈希的十六进制长度（SHA-256 前 8 字节 → 16 个字符）。
pub const BLOCK_HASH_HEX_LEN: usize = 16;

/// FILE_BLOCK_MAP payload（M11）：块表，`hashes` 为连续十六进制串。
///
/// 结构校验（块大小、哈希串长度、十六进制字符）由 `transfer::chunkmap` 负责，
/// 协议层只做 JSON 编解码，保持与其它帧一致。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileBlockMap {
    /// 传输任务 ID。
    pub transfer_id: String,
    /// 协商后的块大小（字节）。
    pub block_size: u32,
    /// 连续十六进制块哈希，每 `BLOCK_HASH_HEX_LEN` 个字符对应一块。
    pub hashes: String,
}

impl FileBlockMap {
    /// 块数量（按哈希串长度推导；不含非法长度校验）。
    pub fn block_count(&self) -> usize {
        self.hashes.len() / BLOCK_HASH_HEX_LEN
    }

    /// 第 `index` 块的哈希（8 字节）；越界或非法十六进制返回 None。
    pub fn hash_at(&self, index: usize) -> Option<[u8; 8]> {
        let start = index.checked_mul(BLOCK_HASH_HEX_LEN)?;
        let end = start.checked_add(BLOCK_HASH_HEX_LEN)?;
        let slice = self.hashes.get(start..end)?;
        let mut out = [0u8; 8];
        for (i, chunk) in slice.as_bytes().chunks(2).enumerate() {
            let text = std::str::from_utf8(chunk).ok()?;
            out[i] = u8::from_str_radix(text, 16).ok()?;
        }
        Some(out)
    }
}

/// FILE_HAVE payload（M11）：接收方已收到的块位图（十六进制，位序 = 块索引）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHave {
    /// 传输任务 ID。
    pub transfer_id: String,
    /// 十六进制位图。
    pub bitmap: String,
}

/// FILE_RESUME_STATE payload（M11）：重连后恢复位图与已收字节。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileResumeState {
    /// 传输任务 ID。
    pub transfer_id: String,
    /// 十六进制位图。
    pub bitmap: String,
    /// 已确认收到的字节数（用于降级到偏移续传）。
    pub received: u64,
}

/// FILE_CHUNK 头部长度：transfer_id(16B) + seq(4B)。
pub const CHUNK_HEADER_LEN: usize = 20;
/// 单块最大数据长度（2MB；帧上限 4MB，含 20B 块头仍安全）。
///
/// v1 客户端使用 1MB；v2 经 `HELLO.features` 的 `chunk2m` 协商后使用 2MB。
pub const MAX_CHUNK_DATA: usize = 2 * 1024 * 1024;

/// 编码 FILE_CHUNK payload：transfer_id(16B) + seq(4B, 大端) + data。
pub fn encode_chunk(transfer_id: &Uuid, seq: u32, data: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    let len = u32::try_from(data.len()).map_err(|_| ProtocolError::FrameTooLarge(u32::MAX))?;
    if data.len() > MAX_CHUNK_DATA {
        return Err(ProtocolError::FrameTooLarge(len));
    }
    let mut out = Vec::with_capacity(CHUNK_HEADER_LEN + data.len());
    out.extend_from_slice(transfer_id.as_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(data);
    Ok(out)
}

/// 解码 FILE_CHUNK payload，返回 (transfer_id, seq, 数据切片)。
pub fn decode_chunk(payload: &[u8]) -> Result<(Uuid, u32, &[u8]), ProtocolError> {
    if payload.len() < CHUNK_HEADER_LEN {
        return Err(ProtocolError::BadChunkPayload(payload.len()));
    }
    let id = Uuid::from_slice(&payload[..16])
        .map_err(|_| ProtocolError::BadChunkPayload(payload.len()))?;
    let seq = u32::from_be_bytes([payload[16], payload[17], payload[18], payload[19]]);
    Ok((id, seq, &payload[CHUNK_HEADER_LEN..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：v2 HELLO 携带能力声明与监听端口，可原样往返。
    #[test]
    fn hello_with_features_roundtrip() {
        let hello = Hello::with_features("id-1", "PC-a", vec!["chunk2m".to_string()], 28101);
        let bytes = serde_json::to_vec(&hello).expect("serialize");
        let back: Hello = serde_json::from_slice(&bytes).expect("deserialize");
        assert_eq!(back, hello);
        assert!(back.supports("chunk2m"));
        assert!(!back.supports("chunkmap"));
        assert_eq!(back.tcp_port, Some(28101));
    }

    /// 边界：v1 HELLO 缺 features/tcp_port 时按默认值解析（向后兼容）。
    #[test]
    fn hello_without_v2_fields_parses() {
        let hello: Hello =
            serde_json::from_str(r#"{"device_id":"id-1","name":"PC-a","ver":1}"#).expect("parse");
        assert!(hello.features.is_empty());
        assert_eq!(hello.tcp_port, None);
        assert!(!hello.supports("chunk2m"));
    }

    fn text_frame() -> Frame {
        Frame::json(
            FrameType::Text,
            &TextMessage {
                msg_id: "m-1".to_string(),
                text: "你好\n第二行".to_string(),
                ts: 1_700_000_000_000,
            },
        )
        .unwrap()
    }

    /// 正常路径：编码后能从字节流解出同一帧与同一 JSON 内容。
    #[tokio::test]
    async fn frame_encode_decode_roundtrip() {
        let frame = text_frame();
        let bytes = frame.encode().unwrap();
        assert_eq!(&bytes[..5], MAGIC);
        assert_eq!(bytes[5], FrameType::Text.as_u8());

        let mut reader: &[u8] = &bytes;
        let decoded = read_frame(&mut reader)
            .await
            .unwrap()
            .expect("frame present");
        assert_eq!(decoded, frame);

        let msg: TextMessage = decoded.parse_json().unwrap();
        assert_eq!(msg.msg_id, "m-1");
        assert_eq!(msg.text, "你好\n第二行");
    }

    /// 正常路径：write_frame 与 read_frame 在真实流上配对（双向管道）。
    #[tokio::test]
    async fn write_then_read_over_stream() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let frame = Frame::json(FrameType::Hello, &Hello::new("device-1", "PC-8f3a")).unwrap();
        write_frame(&mut client, &frame).await.unwrap();
        let received = read_frame(&mut server).await.unwrap().unwrap();
        assert_eq!(received.frame_type, FrameType::Hello);
        let hello: Hello = received.parse_json().unwrap();
        assert_eq!(hello.device_id, "device-1");
        assert_eq!(hello.name, "PC-8f3a");
        assert_eq!(hello.ver, 1);
    }

    /// 边界：心跳帧 payload 为空，可正常往返。
    #[tokio::test]
    async fn empty_heartbeat_roundtrip() {
        let frame = Frame::empty(FrameType::Heartbeat);
        let bytes = frame.encode().unwrap();
        assert_eq!(bytes.len(), HEADER_LEN);
        let mut reader: &[u8] = &bytes;
        let decoded = read_frame(&mut reader).await.unwrap().unwrap();
        assert_eq!(decoded, frame);
        assert!(decoded.payload.is_empty());
    }

    /// 异常：magic 不符的帧被拒绝（非本协议连接）。
    #[tokio::test]
    async fn bad_magic_is_rejected() {
        let bytes = b"EVIL!\x02\0\0\0\x02{}";
        let mut reader: &[u8] = bytes;
        match read_frame(&mut reader).await {
            Err(ProtocolError::BadMagic(m)) => assert_eq!(&m, b"EVIL!"),
            other => panic!("expected BadMagic, got {other:?}"),
        }
    }

    /// 异常：超过 4MB 的帧在读取 payload 前就被拒绝（R5.3）。
    #[tokio::test]
    async fn oversized_frame_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(FrameType::Text.as_u8());
        bytes.extend_from_slice(&(MAX_FRAME_LEN + 1).to_be_bytes());
        let mut reader: &[u8] = &bytes;
        match read_frame(&mut reader).await {
            Err(ProtocolError::FrameTooLarge(len)) => assert_eq!(len, MAX_FRAME_LEN + 1),
            other => panic!("expected FrameTooLarge, got {other:?}"),
        }
    }

    /// 异常：未知帧类型被拒绝。
    #[tokio::test]
    async fn unknown_frame_type_is_rejected() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(0x7A);
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let mut reader: &[u8] = &bytes;
        match read_frame(&mut reader).await {
            Err(ProtocolError::UnknownType(t)) => assert_eq!(t, 0x7A),
            other => panic!("expected UnknownType, got {other:?}"),
        }
    }

    /// 异常：payload 不足（半包）报 IO 错误而不是 panic。
    #[tokio::test]
    async fn truncated_payload_is_io_error() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(FrameType::Text.as_u8());
        bytes.extend_from_slice(&10u32.to_be_bytes());
        bytes.extend_from_slice(b"{}\n");
        let mut reader: &[u8] = &bytes;
        assert!(matches!(
            read_frame(&mut reader).await,
            Err(ProtocolError::Io(_))
        ));
    }

    /// 边界：对端在帧边界关闭连接，返回 None 表示正常结束。
    #[tokio::test]
    async fn clean_close_returns_none() {
        let empty: &[u8] = &[];
        let mut reader = empty;
        assert!(read_frame(&mut reader).await.unwrap().is_none());
    }

    /// 异常：编码超大 payload 时拒绝，避免写出非法帧。
    #[tokio::test]
    async fn encode_oversized_payload_is_rejected() {
        let frame = Frame::new(FrameType::Text, vec![0u8; MAX_FRAME_LEN as usize + 1]);
        assert!(matches!(
            frame.encode(),
            Err(ProtocolError::FrameTooLarge(_))
        ));
    }

    /// 正常路径：FILE_OFFER 的 JSON 载荷往返一致（kind 为小写字符串）。
    #[test]
    fn file_offer_json_roundtrip() {
        let offer = FileOffer {
            transfer_id: "t-1".to_string(),
            kind: FileKind::Image,
            filename: "photo.png".to_string(),
            size: 1024,
            sha256: "abc".to_string(),
        };
        let frame = Frame::json(FrameType::FileOffer, &offer).unwrap();
        let json = String::from_utf8(frame.payload.clone()).unwrap();
        assert!(json.contains("\"kind\":\"image\""));

        let parsed: FileOffer = frame.parse_json().unwrap();
        assert_eq!(parsed, offer);
    }

    /// 正常路径：FILE_CHUNK 编码后可还原 transfer_id / seq / 数据。
    #[test]
    fn chunk_encode_decode_roundtrip() {
        let id = Uuid::new_v4();
        let payload = encode_chunk(&id, 7, b"hello-chunk").unwrap();
        assert_eq!(payload.len(), CHUNK_HEADER_LEN + 11);

        let (decoded_id, seq, data) = decode_chunk(&payload).unwrap();
        assert_eq!(decoded_id, id);
        assert_eq!(seq, 7);
        assert_eq!(data, b"hello-chunk");
    }

    /// 边界：空数据块合法（文件尾正好整除块大小时不会出现，但协议层允许）。
    #[test]
    fn chunk_with_empty_data_is_allowed() {
        let id = Uuid::new_v4();
        let payload = encode_chunk(&id, 0, b"").unwrap();
        let (_, seq, data) = decode_chunk(&payload).unwrap();
        assert_eq!(seq, 0);
        assert!(data.is_empty());
    }

    /// 异常：块 payload 长度不足时拒绝，返回 BadChunkPayload 而不是 panic。
    #[test]
    fn chunk_payload_too_short_is_rejected() {
        let short = [0u8; CHUNK_HEADER_LEN - 1];
        assert!(matches!(
            decode_chunk(&short),
            Err(ProtocolError::BadChunkPayload(_))
        ));
    }

    /// 异常：单块数据超过 1MB 时拒绝编码（避免巨块拖垮接收端）。
    #[test]
    fn oversized_chunk_is_rejected() {
        let id = Uuid::new_v4();
        let data = vec![0u8; MAX_CHUNK_DATA + 1];
        assert!(matches!(
            encode_chunk(&id, 0, &data),
            Err(ProtocolError::FrameTooLarge(_))
        ));
    }

    /// 正常路径：FILE_RESULT 可带路径、也可为空路径（失败）。
    #[test]
    fn file_result_optional_path() {
        let ok = FileResult {
            transfer_id: "t-1".to_string(),
            ok: true,
            path: Some("C:/Downloads/photo.png".to_string()),
            error: None,
        };
        let failed = FileResult {
            transfer_id: "t-1".to_string(),
            ok: false,
            path: None,
            error: Some("missing_blocks".to_string()),
        };
        let ok_json = serde_json::to_string(&ok).unwrap();
        assert!(ok_json.contains("\"ok\":true"));
        assert!(ok_json.contains("\"path\":\"C:/Downloads/photo.png\""));
        let failed_json = serde_json::to_string(&failed).unwrap();
        assert!(failed_json.contains("\"ok\":false"));
        assert!(failed_json.contains("\"path\":null"));
        assert!(failed_json.contains("\"error\":\"missing_blocks\""));
    }

    /// 正常路径：块级续传帧类型（0x17–0x19）可往返转换，且不与已有帧冲突。
    #[test]
    fn block_map_frame_types_roundtrip() {
        for (value, expected) in [
            (0x17u8, FrameType::FileBlockMap),
            (0x18, FrameType::FileHave),
            (0x19, FrameType::FileResumeState),
        ] {
            assert_eq!(FrameType::from_u8(value), Some(expected));
            assert_eq!(expected.as_u8(), value);
        }
    }

    /// 正常路径：块表帧的 JSON 与哈希切分（每 16 字符 8 字节）。
    #[test]
    fn block_map_payload_hash_at() {
        let map = FileBlockMap {
            transfer_id: "t-1".to_string(),
            block_size: 1024,
            hashes: format!("{}{}", "0011223344556677", "8899aabbccddeeff"),
        };
        assert_eq!(map.block_count(), 2);
        assert_eq!(
            map.hash_at(0),
            Some([0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77])
        );
        assert_eq!(
            map.hash_at(1),
            Some([0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff])
        );
        assert_eq!(map.hash_at(2), None);

        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains("\"transfer_id\":\"t-1\""));
        assert!(json.contains("\"block_size\":1024"));
    }
}
