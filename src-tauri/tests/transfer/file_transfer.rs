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
use chatters_lib::session::{
    start_session, SessionConfig, SessionEvent, SessionHub, LOCAL_FEATURES,
};
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
    spawn_node_full(device_id, name, dir, idle_timeout, local_features()).await
}

/// 本机默认能力声明（与生产一致）。
fn local_features() -> Vec<String> {
    LOCAL_FEATURES.iter().map(|f| f.to_string()).collect()
}

/// 完整版本：可自定义能力声明（用于模拟旧端做兼容回归）。
async fn spawn_node_full(
    device_id: &str,
    name: &str,
    dir: PathBuf,
    idle_timeout: Duration,
    features: Vec<String>,
) -> (TransferManager, SessionHub, u16) {
    let port = free_tcp_port().await;
    let mut session_config = SessionConfig::new(device_id, name, port, Duration::from_secs(1));
    session_config.features = features;
    let (hub, mut events, _task) = start_session(session_config).await.expect("start session");

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
    wait_state_within(manager, transfer_id, wanted, Duration::from_secs(5)).await
}

/// 轮询等待（可指定超时，供大文件基准使用）。
async fn wait_state_within(
    manager: &TransferManager,
    transfer_id: &str,
    wanted: TransferState,
    limit: Duration,
) -> TransferInfo {
    let deadline = std::time::Instant::now() + limit;
    while std::time::Instant::now() < deadline {
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

/// 正常路径：多块文件走 chunkmap（块表 + 位图 + 随机写），接收端校验通过（M11）。
#[tokio::test]
async fn chunkmap_transfers_multi_block_file() {
    let root = temp_root("chunkmap");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    // 2.5MB：默认 1MB 块 → 3 块；若协商 2MB 块 → 2 块（两种都应正确）。
    let payload: Vec<u8> = (0..(2 * 1024 * 1024 + 512 * 1024))
        .map(|index| (index % 251) as u8)
        .collect();
    let source = dir_b.join("multi.bin");
    std::fs::write(&source, &payload).unwrap();

    let info = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send file");
    // 普通文件默认等待确认。
    wait_state(&manager_a, &info.transfer_id, TransferState::Pending).await;
    manager_a
        .accept(&info.transfer_id)
        .await
        .expect("accept file");

    let done = wait_state(&manager_a, &info.transfer_id, TransferState::Done).await;
    let received = PathBuf::from(done.path.expect("received path"));
    assert_eq!(std::fs::read(&received).unwrap(), payload);

    // 成功后不留下 `.part` / `.bitmap` 残留。
    assert!(part_files(&dir_a).is_empty(), "part must be renamed");
    let leftovers: Vec<PathBuf> = std::fs::read_dir(&dir_a)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("bitmap"))
        .collect();
    assert!(leftovers.is_empty(), "bitmap must be cleaned after success");

    std::fs::remove_dir_all(&root).ok();
}

/// 兼容：双方都不声明 chunkmap 时回退到顺序写/偏移续传，传输仍然成功（M11 降级）。
#[tokio::test]
async fn legacy_peer_falls_back_without_chunkmap() {
    let root = temp_root("legacy");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node_full(
        "device-a",
        "PC-a",
        dir_a.clone(),
        Duration::from_secs(30),
        Vec::new(),
    )
    .await;
    let (manager_b, hub_b, _port_b) = spawn_node_full(
        "device-b",
        "PC-b",
        dir_b.clone(),
        Duration::from_secs(30),
        Vec::new(),
    )
    .await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    // 1.5MB（1MB 块 → 2 块），走旧偏移续传路径。
    let payload: Vec<u8> = (0..(1024 * 1024 + 512 * 1024))
        .map(|index| (index % 97) as u8)
        .collect();
    let source = dir_b.join("legacy.bin");
    std::fs::write(&source, &payload).unwrap();

    let info = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send file");
    wait_state(&manager_a, &info.transfer_id, TransferState::Pending).await;
    manager_a
        .accept(&info.transfer_id)
        .await
        .expect("accept file");

    let done = wait_state(&manager_a, &info.transfer_id, TransferState::Done).await;
    assert_eq!(std::fs::read(done.path.expect("path")).unwrap(), payload);

    std::fs::remove_dir_all(&root).ok();
}

/// 正常路径：位图缺失时按 `.part` 重建，重试只补缺失块并完成校验（M11）。
#[tokio::test]
async fn chunkmap_rebuilds_missing_bitmap_and_resumes() {
    let root = temp_root("resume-map");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    // 2.5MB、双方都声明 chunk2m → 2MB 块 → 2 块。
    let payload: Vec<u8> = (0..(2 * 1024 * 1024 + 512 * 1024))
        .map(|index| (index % 199) as u8)
        .collect();
    let source = dir_b.join("resume-map.bin");
    std::fs::write(&source, &payload).unwrap();

    // 预置上次中断残留：`.part` 只含第 0 块（2MB），且 **没有** `.bitmap`。
    let first_block = 2 * 1024 * 1024;
    let key = partial_key("resume-map.bin", payload.len() as u64);
    let partial = dir_a.join(".chatters-partial");
    std::fs::create_dir_all(&partial).unwrap();
    std::fs::write(partial.join(format!("{key}.part")), &payload[..first_block]).unwrap();
    let meta = format!(
        "{{\"filename\":\"resume-map.bin\",\"size\":{},\"sha256\":\"{}\"}}",
        payload.len(),
        sha256_hex(&payload)
    );
    std::fs::write(partial.join(format!("{key}.json")), meta).unwrap();

    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send file");
    wait_state(&manager_a, &sent.transfer_id, TransferState::Pending).await;
    manager_a
        .accept(&sent.transfer_id)
        .await
        .expect("accept file");

    // 位图缺失 → 接收端按 `.part` 重建（第 0 块命中）→ 只补第 1 块 → 校验通过。
    let done = wait_state(&manager_a, &sent.transfer_id, TransferState::Done).await;
    assert_eq!(std::fs::read(done.path.expect("path")).unwrap(), payload);
    assert!(part_files(&dir_a).is_empty());

    std::fs::remove_dir_all(&root).ok();
}

/// 正常路径：待确认阶段取消，双方都进入 Cancelled，且不留临时文件（F33）。
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

/// 简版续传（Q8，v1 降级路径）：双方未声明 `chunkmap` 时命中原有部分文件返回已收偏移，
/// 发送端从该偏移继续，最终校验通过。
#[tokio::test]
async fn resume_from_partial_file() {
    let root = temp_root("resume");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    // 模拟 v1 端（不声明 chunkmap），验证偏移续传仍然可用。
    let (manager_a, _hub_a, port_a) = spawn_node_full(
        "device-a",
        "PC-a",
        dir_a.clone(),
        Duration::from_secs(30),
        Vec::new(),
    )
    .await;
    let (manager_b, hub_b, _port_b) = spawn_node_full(
        "device-b",
        "PC-b",
        dir_b.clone(),
        Duration::from_secs(30),
        Vec::new(),
    )
    .await;
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

/// 大文件吞吐基准（默认忽略，P1 验证用）：
/// `cargo test --offline --test file_transfer -- --ignored --nocapture throughput`
#[tokio::test]
#[ignore = "benchmark: run manually with --ignored --nocapture"]
async fn throughput_256mb() {
    const SIZE: usize = 256 * 1024 * 1024;
    let root = temp_root("bench");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let source = dir_b.join("bench.bin");
    std::fs::write(&source, vec![0xABu8; SIZE]).unwrap();

    let started = std::time::Instant::now();
    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send benchmark file");
    // .bin 属于普通文件，需要接收端确认（F34）。
    wait_state_within(
        &manager_a,
        &sent.transfer_id,
        TransferState::Pending,
        Duration::from_secs(10),
    )
    .await;
    manager_a
        .accept(&sent.transfer_id)
        .await
        .expect("accept benchmark file");
    wait_state_within(
        &manager_a,
        &sent.transfer_id,
        TransferState::Done,
        Duration::from_secs(180),
    )
    .await;
    wait_state_within(
        &manager_b,
        &sent.transfer_id,
        TransferState::Done,
        Duration::from_secs(180),
    )
    .await;
    let elapsed = started.elapsed();

    let mib = SIZE as f64 / (1024.0 * 1024.0);
    println!(
        "throughput: {:.1} MiB/s ({} MiB in {:.2}s, includes sha256 pre-hash)",
        mib / elapsed.as_secs_f64(),
        mib,
        elapsed.as_secs_f64()
    );

    std::fs::remove_dir_all(&root).ok();
}

/// 1GB 吞吐基准（默认忽略，M10 验证用）：
/// `cargo test --release --offline --test file_transfer throughput_1gb -- --ignored --nocapture`
#[tokio::test]
#[ignore = "benchmark: run manually with --ignored --nocapture"]
async fn throughput_1gb() {
    const SIZE: usize = 1024 * 1024 * 1024;
    let root = temp_root("bench1g");
    let dir_a = root.join("a");
    let dir_b = root.join("b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let (manager_a, _hub_a, port_a) = spawn_node("device-a", "PC-a", dir_a.clone()).await;
    let (manager_b, hub_b, _port_b) = spawn_node("device-b", "PC-b", dir_b.clone()).await;
    hub_b.connect(loopback(port_a)).await.expect("connect a");

    let source = dir_b.join("bench-1g.bin");
    std::fs::write(&source, vec![0xCDu8; SIZE]).unwrap();

    let started = std::time::Instant::now();
    let sent = manager_b
        .send_file("device-a", &source)
        .await
        .expect("send 1GB file");
    wait_state_within(
        &manager_a,
        &sent.transfer_id,
        TransferState::Pending,
        Duration::from_secs(30),
    )
    .await;
    manager_a
        .accept(&sent.transfer_id)
        .await
        .expect("accept 1GB file");
    wait_state_within(
        &manager_a,
        &sent.transfer_id,
        TransferState::Done,
        Duration::from_secs(600),
    )
    .await;
    wait_state_within(
        &manager_b,
        &sent.transfer_id,
        TransferState::Done,
        Duration::from_secs(600),
    )
    .await;
    let elapsed = started.elapsed();

    let mib = SIZE as f64 / (1024.0 * 1024.0);
    println!(
        "throughput: {:.1} MiB/s ({} MiB in {:.2}s, includes sha256 pre-hash)",
        mib / elapsed.as_secs_f64(),
        mib,
        elapsed.as_secs_f64()
    );

    std::fs::remove_dir_all(&root).ok();
}
