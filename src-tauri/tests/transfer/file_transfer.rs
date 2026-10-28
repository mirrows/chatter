//! 集成测试：图片/文件传输主流程（M3-a）。
//!
//! 覆盖 rules.md R2.4：
//! - 正常路径：图片自动接收（F34）+ sha256 校验 + 落盘内容一致；
//! - 正常路径：普通文件需确认后接收（F34）；
//! - 正常路径：拒绝接收后双方状态一致；
//! - 异常：路径穿越的文件名被拒绝（R5.2）；
//! - 异常：哈希不一致时接收端标记失败且不留残留文件；
//! - 边界：待确认阶段取消，双方都进入 Cancelled。

use chatters_lib::protocol::{
    encode_chunk, read_frame, write_frame, FileDone, FileKind, FileOffer, Frame, FrameType, Hello,
};
use chatters_lib::session::{start_session, SessionConfig, SessionEvent, SessionHub};
use chatters_lib::transfer::{
    TransferConfig, TransferError, TransferEvent, TransferInfo, TransferKind, TransferManager,
    TransferState,
};
use sha2::{Digest, Sha256};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// 取一个当前空闲的 TCP 端口。
async fn free_tcp_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral");
    listener.local_addr().expect("local addr").port()
}

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// 建一个测试节点：会话 + 传输中心 + 事件路由（等价 lib.rs 的接线）。
async fn spawn_node(
    device_id: &str,
    name: &str,
    dir: PathBuf,
) -> (TransferManager, SessionHub, u16) {
    spawn_node_with_timeout(device_id, name, dir, Duration::from_secs(30)).await
}

/// 同上，但可指定空闲超时（用于超时用例）。
async fn spawn_node_with_timeout(
    device_id: &str,
    name: &str,
    dir: PathBuf,
    idle_timeout: Duration,
) -> (TransferManager, SessionHub, u16) {
    let port = free_tcp_port().await;
    let (hub, mut events, _task) = start_session(SessionConfig {
        device_id: device_id.to_string(),
        name: name.to_string(),
        tcp_port: port,
        ack_timeout: Duration::from_secs(1),
    })
    .await
    .expect("start session");

    let (tx, mut rx) = mpsc::channel::<TransferEvent>(128);
    let mut config = TransferConfig::new(dir, true);
    config.idle_timeout = idle_timeout;
    let manager = TransferManager::new(config, hub.clone(), tx);
    manager.spawn_maintenance();

    // 事件通道必须有消费者，否则容量满后传输会阻塞。
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    let router = manager.clone();
    tokio::spawn(async move {
        while let Some(event) = events.recv().await {
            match event {
                SessionEvent::TransferFrame { device_id, frame } => {
                    if let Err(e) = router.handle_frame(&device_id, frame).await {
                        log::warn!("transfer routing failed in test: {e}");
                    }
                }
                SessionEvent::PeerDisconnected { device_id } => {
                    router.handle_peer_disconnected(&device_id).await;
                }
                _ => {}
            }
        }
    });

    (manager, hub, port)
}

/// 轮询等待任务进入指定状态。
async fn wait_state(
    manager: &TransferManager,
    transfer_id: &str,
    wanted: TransferState,
) -> TransferInfo {
    for _ in 0..200 {
        if let Some(info) = manager.get(transfer_id).await {
            if info.state == wanted {
                return info;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("transfer {transfer_id} did not reach {wanted:?}");
}

/// 列出目录中的 `.part` 临时文件数量。
fn part_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("part"))
        .collect()
}

/// 创建本次测试专用的临时目录。
fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "chatters_transfer_{tag}_{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// 正常路径：图片自动接收，校验通过、内容一致、临时文件已清理（F34/F36）。
#[tokio::test]
async fn image_is_auto_received_and_verified() {
    let root = temp_root("image");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let data = vec![7u8; 3 * 1024 * 1024 + 123];
    let source = dir_b.join("photo.png");
    std::fs::write(&source, &data).unwrap();

    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send image");
    assert_eq!(sent.kind, TransferKind::Image);

    let sender = wait_state(&manager_b, &sent.transfer_id, TransferState::Done).await;
    assert_eq!(sender.transferred, data.len() as u64);

    let received = wait_state(&manager_a, &sent.transfer_id, TransferState::Done).await;
    assert_eq!(received.direction.as_str(), "receive");
    assert_eq!(received.filename, "photo.png");
    let path = received.path.expect("received path");
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert!(part_files(&dir_a).is_empty(), "no .part left behind");

    std::fs::remove_dir_all(&root).ok();
}

/// 正常路径：普通文件停在 Pending，用户确认后才接收完成（F34）。
#[tokio::test]
async fn file_requires_confirmation_before_receiving() {
    let root = temp_root("file");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let data = b"hello chatters file".to_vec();
    let source = dir_b.join("notes.txt");
    std::fs::write(&source, &data).unwrap();

    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send file");
    assert_eq!(sent.kind, TransferKind::File);

    let pending = wait_state(&manager_a, &sent.transfer_id, TransferState::Pending).await;
    assert_eq!(pending.direction.as_str(), "receive");
    assert!(pending.path.is_none());

    manager_a
        .accept(&sent.transfer_id)
        .await
        .expect("accept file");
    let received = wait_state(&manager_a, &sent.transfer_id, TransferState::Done).await;
    let path = received.path.expect("received path");
    assert_eq!(std::fs::read(&path).unwrap(), data);
    wait_state(&manager_b, &sent.transfer_id, TransferState::Done).await;

    std::fs::remove_dir_all(&root).ok();
}

/// 正常路径：接收方拒绝后，双方状态都变为 Rejected。
#[tokio::test]
async fn reject_notifies_sender() {
    let root = temp_root("reject");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let source = dir_b.join("payload.bin");
    std::fs::write(&source, b"binary").unwrap();
    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send file");

    wait_state(&manager_a, &sent.transfer_id, TransferState::Pending).await;
    manager_a
        .reject(&sent.transfer_id)
        .await
        .expect("reject file");
    wait_state(&manager_a, &sent.transfer_id, TransferState::Rejected).await;
    wait_state(&manager_b, &sent.transfer_id, TransferState::Rejected).await;

    std::fs::remove_dir_all(&root).ok();
}

/// 异常：文件名含路径穿越时直接拒绝，不落盘（rules.md R5.2）。
#[tokio::test]
async fn path_traversal_offer_is_rejected() {
    let root = temp_root("traversal");
    let dir_a = root.join("a");
    std::fs::create_dir_all(&dir_a).unwrap();
    let (manager_a, _hub_a, _port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;

    let offer = FileOffer {
        transfer_id: uuid::Uuid::new_v4().to_string(),
        kind: FileKind::Image,
        filename: "../evil.txt".to_string(),
        size: 4,
        sha256: sha256_hex(b"evil"),
    };
    let frame = Frame::json(FrameType::FileOffer, &offer).unwrap();

    let error = manager_a
        .handle_frame("device-b", frame)
        .await
        .expect_err("traversal must be rejected");
    assert!(matches!(error, TransferError::UnsafeFileName(_)));
    assert!(
        !root.join("evil.txt").exists(),
        "must not escape target dir"
    );
    assert!(part_files(&dir_a).is_empty());

    std::fs::remove_dir_all(&root).ok();
}

/// 异常：收到的数据与声明的 sha256 不一致时标记失败且不留残留文件（F36）。
#[tokio::test]
async fn hash_mismatch_marks_failed() {
    let root = temp_root("hash");
    let dir_a = root.join("a");
    std::fs::create_dir_all(&dir_a).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (_manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", root.join("b")).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let transfer_id = uuid::Uuid::new_v4().to_string();
    let declared = sha256_hex(b"AAAA");
    let offer = FileOffer {
        transfer_id: transfer_id.clone(),
        kind: FileKind::Image,
        filename: "broken.png".to_string(),
        size: 4,
        sha256: declared.clone(),
    };
    hub_b
        .send_frame(
            "device-a",
            Frame::json(FrameType::FileOffer, &offer).unwrap(),
        )
        .await
        .expect("send offer");
    wait_state(&manager_a, &transfer_id, TransferState::Transferring).await;

    // 实际发送的内容与 OFFER 声明的哈希不一致。
    let payload = encode_chunk(&uuid::Uuid::parse_str(&transfer_id).unwrap(), 0, b"BBBB").unwrap();
    hub_b
        .send_frame("device-a", Frame::new(FrameType::FileChunk, payload))
        .await
        .expect("send chunk");
    hub_b
        .send_frame(
            "device-a",
            Frame::json(
                FrameType::FileDone,
                &FileDone {
                    transfer_id: transfer_id.clone(),
                    sha256: declared,
                },
            )
            .unwrap(),
        )
        .await
        .expect("send done");

    let failed = wait_state(&manager_a, &transfer_id, TransferState::Failed).await;
    assert_eq!(failed.error.as_deref(), Some("哈希校验失败"));
    assert!(failed.path.is_none());
    assert!(!dir_a.join("broken.png").exists());
    assert!(part_files(&dir_a).is_empty());

    std::fs::remove_dir_all(&root).ok();
}

/// 边界：待确认阶段取消，双方都进入 Cancelled，且不留临时文件（F33）。
#[tokio::test]
async fn cancel_before_accept_cancels_both_sides() {
    let root = temp_root("cancel");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let source = dir_b.join("big.zip");
    std::fs::write(&source, vec![1u8; 4096]).unwrap();
    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send file");
    wait_state(&manager_a, &sent.transfer_id, TransferState::Pending).await;

    manager_a
        .cancel(&sent.transfer_id)
        .await
        .expect("cancel transfer");
    wait_state(&manager_a, &sent.transfer_id, TransferState::Cancelled).await;
    wait_state(&manager_b, &sent.transfer_id, TransferState::Cancelled).await;
    assert!(part_files(&dir_a).is_empty());

    std::fs::remove_dir_all(&root).ok();
}

/// 续传匹配键（与接收端实现保持一致，测试用于预置部分文件）。
fn partial_key(filename: &str, size: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(filename.as_bytes());
    hasher.update(b"|");
    hasher.update(size.to_be_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[..32].to_string()
}

/// 简版续传（Q8）：命中原有部分文件时返回已收偏移，发送端从该偏移继续，最终校验通过。
#[tokio::test]
async fn resume_from_partial_file() {
    let root = temp_root("resume");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let data = vec![9u8; 2 * 1024 * 1024 + 777];
    let source = dir_b.join("resume.bin");
    std::fs::write(&source, &data).unwrap();

    // 预置上次中断留下的 1MB 部分文件与元数据。
    let partial = dir_a.join(".chatters-partial");
    std::fs::create_dir_all(&partial).unwrap();
    let key = partial_key("resume.bin", data.len() as u64);
    std::fs::write(partial.join(format!("{key}.part")), &data[..1024 * 1024]).unwrap();
    let meta = format!(
        "{{\"filename\":\"resume.bin\",\"size\":{},\"sha256\":\"{}\"}}",
        data.len(),
        sha256_hex(&data)
    );
    std::fs::write(partial.join(format!("{key}.json")), meta).unwrap();

    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send file");
    let pending = wait_state(&manager_a, &sent.transfer_id, TransferState::Pending).await;
    assert_eq!(pending.transferred, 1024 * 1024, "接收端应命中续传偏移");

    manager_a
        .accept(&sent.transfer_id)
        .await
        .expect("accept resumed file");
    let received = wait_state(&manager_a, &sent.transfer_id, TransferState::Done).await;
    let path = received.path.expect("received path");
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert!(!partial.join(format!("{key}.part")).exists());

    std::fs::remove_dir_all(&root).ok();
}

/// 边界：长时间没有数据块时判定超时并保留部分文件以便续传（M3-b）。
#[tokio::test]
async fn idle_transfer_times_out() {
    let root = temp_root("idle");
    let dir_a = root.join("a");
    std::fs::create_dir_all(&dir_a).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node_with_timeout(
        "device-a",
        "PC-a",
        dir_a.clone(),
        Duration::from_millis(200),
    )
    .await;
    let (_manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", root.join("b")).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let transfer_id = uuid::Uuid::new_v4().to_string();
    let data = b"partial-data".to_vec();
    let offer = FileOffer {
        transfer_id: transfer_id.clone(),
        kind: FileKind::Image,
        filename: "idle.png".to_string(),
        size: 64,
        sha256: sha256_hex(&data),
    };
    hub_b
        .send_frame(
            "device-a",
            Frame::json(FrameType::FileOffer, &offer).unwrap(),
        )
        .await
        .expect("send offer");
    wait_state(&manager_a, &transfer_id, TransferState::Transferring).await;

    let chunk = encode_chunk(&uuid::Uuid::parse_str(&transfer_id).unwrap(), 0, &data).unwrap();
    hub_b
        .send_frame("device-a", Frame::new(FrameType::FileChunk, chunk))
        .await
        .expect("send chunk");

    let failed = wait_state(&manager_a, &transfer_id, TransferState::Failed).await;
    assert_eq!(failed.error.as_deref(), Some("传输超时，可重试"));

    std::fs::remove_dir_all(&root).ok();
}

/// 异常：对端断开连接后未完成任务立即标记失败并可重试（M3-b）。
#[tokio::test]
async fn peer_disconnect_marks_task_failed() {
    let root = temp_root("disconnect");
    let dir_a = root.join("a");
    std::fs::create_dir_all(&dir_a).unwrap();
    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;

    let mut stream = TcpStream::connect(loopback(port_a))
        .await
        .expect("connect raw peer");
    write_frame(
        &mut stream,
        &Frame::json(FrameType::Hello, &Hello::new("raw-peer", "PC-raw")).unwrap(),
    )
    .await
    .expect("send hello");
    let hello = read_frame(&mut stream)
        .await
        .expect("read hello")
        .expect("hello frame");
    assert_eq!(hello.frame_type, FrameType::Hello);

    let transfer_id = uuid::Uuid::new_v4().to_string();
    let offer = FileOffer {
        transfer_id: transfer_id.clone(),
        kind: FileKind::Image,
        filename: "raw.png".to_string(),
        size: 64,
        sha256: sha256_hex(b"raw"),
    };
    write_frame(
        &mut stream,
        &Frame::json(FrameType::FileOffer, &offer).unwrap(),
    )
    .await
    .expect("send offer");
    wait_state(&manager_a, &transfer_id, TransferState::Transferring).await;

    drop(stream);
    let failed = wait_state(&manager_a, &transfer_id, TransferState::Failed).await;
    assert_eq!(failed.error.as_deref(), Some("连接已断开，可重试"));

    std::fs::remove_dir_all(&root).ok();
}

/// 异常：源路径是目录时拒绝发送（F31 由前端提示压缩为 zip）。
#[tokio::test]
async fn sending_directory_is_rejected() {
    let root = temp_root("dir");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    let folder = root.join("folder");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    std::fs::create_dir_all(&folder).unwrap();

    let (_manager_a, _hub_a, _port_a) = spawn_node("device-a", "PC-a", dir_a).await;
    let (manager_b, _hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b).await;

    let error = manager_b
        .send_file("device-a", &folder)
        .await
        .expect_err("directory must be rejected");
    assert!(matches!(error, TransferError::SourceNotFile(_)));

    std::fs::remove_dir_all(&root).ok();
}
