//! 集成测试：两个真实 TCP 会话端点之间的文字消息收发（M2-b）。
//!
//! 覆盖 rules.md R2.4：
//! - 正常路径：HELLO 握手 + TEXT/TEXT_ACK，双方都收到事件；
//! - 异常：对端不发 ACK 时发送方在超时后标记失败；
//! - 异常：对端握手发送非法帧时连接被拒绝；
//! - 边界：未建连时发送返回 NotConnected。

use chatters_lib::protocol::{read_frame, write_frame, Frame, FrameType, Hello};
use chatters_lib::session::{start_session, SendStatus, SessionConfig, SessionError, SessionEvent};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::time::timeout;

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

/// 在事件流中等待 TextReceived，忽略 PeerConnected 等前置事件。
async fn wait_for_text(rx: &mut mpsc::Receiver<SessionEvent>) -> SessionEvent {
    for _ in 0..10 {
        let event = timeout(Duration::from_secs(3), rx.recv())
            .await
            .expect("timed out waiting for text event")
            .expect("session channel closed");
        if matches!(event, SessionEvent::TextReceived { .. }) {
            return event;
        }
    }
    panic!("text event not observed");
}

/// 正常路径：A 发消息，B 收到并入队事件，A 收到 ACK 标记为已送达。
#[tokio::test]
async fn text_delivery_between_two_hubs() {
    let port_b = free_tcp_port().await;
    let port_a = free_tcp_port().await;

    let (_hub_b, mut events_b, task_b) = start_session(SessionConfig::new(
        "device-b",
        "PC-b",
        port_b,
        Duration::from_secs(1),
    ))
    .await
    .expect("start session b");

    let (hub_a, _events_a, task_a) = start_session(SessionConfig::new(
        "device-a",
        "PC-a",
        port_a,
        Duration::from_secs(1),
    ))
    .await
    .expect("start session a");

    let peer = hub_a.connect(loopback(port_b)).await.expect("connect to b");
    assert_eq!(peer.device_id, "device-b");
    assert_eq!(peer.name, "PC-b");
    assert!(hub_a.is_connected("device-b").await);

    let result = hub_a
        .send_text("device-b", "你好\n第二行")
        .await
        .expect("send text");
    assert_eq!(result.status, SendStatus::Delivered);
    assert!(!result.msg_id.is_empty());

    match wait_for_text(&mut events_b).await {
        SessionEvent::TextReceived {
            device_id,
            msg_id,
            text,
            ts,
        } => {
            assert_eq!(device_id, "device-a");
            assert_eq!(msg_id, result.msg_id);
            assert_eq!(text, "你好\n第二行");
            assert!(ts > 0);
        }
        other => panic!("expected TextReceived, got {other:?}"),
    }

    task_a.abort();
    task_b.abort();
}

/// 异常：对端完成握手但不回 ACK，发送方在超时后返回 Failed。
#[tokio::test]
async fn missing_ack_marks_send_failed() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind peer");
    let peer_addr = listener.local_addr().expect("peer addr");

    // 静默对端：完成握手后只读不回复 ACK。
    let peer_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let (mut reader, mut writer) = stream.into_split();
        let first = read_frame(&mut reader)
            .await
            .expect("read hello")
            .expect("hello frame");
        assert_eq!(first.frame_type, FrameType::Hello);
        let hello = Frame::json(FrameType::Hello, &Hello::new("silent-peer", "PC-silent"))
            .expect("hello frame");
        write_frame(&mut writer, &hello).await.expect("write hello");
        // 读走 TEXT，但不回 ACK。
        let text = read_frame(&mut reader)
            .await
            .expect("read text")
            .expect("text frame");
        assert_eq!(text.frame_type, FrameType::Text);
        // 保持连接，直到测试结束。
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let port = free_tcp_port().await;
    let (hub, _events, task) = start_session(SessionConfig::new(
        "device-a",
        "PC-a",
        port,
        Duration::from_millis(200),
    ))
    .await
    .expect("start session");

    hub.connect(peer_addr).await.expect("connect silent peer");
    let result = hub.send_text("silent-peer", "hello").await.expect("send");
    assert_eq!(result.status, SendStatus::Failed);

    peer_task.abort();
    task.abort();
}

/// 异常：对端握手阶段发送非法帧，连接被拒绝且不登记对端。
#[tokio::test]
async fn invalid_handshake_is_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind peer");
    let peer_addr = listener.local_addr().expect("peer addr");

    let peer_task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        // 读掉对端 HELLO，再回一段非协议字节。
        let mut buf = [0u8; 128];
        let _ = stream.read(&mut buf).await;
        let _ = stream.write_all(b"NOT-A-FRAME").await;
    });

    let port = free_tcp_port().await;
    let (hub, _events, task) = start_session(SessionConfig::new(
        "device-a",
        "PC-a",
        port,
        Duration::from_millis(200),
    ))
    .await
    .expect("start session");

    assert!(hub.connect(peer_addr).await.is_err());
    assert!(!hub.is_connected("silent-peer").await);

    peer_task.abort();
    task.abort();
}

/// 边界：未建立会话就发消息返回 NotConnected。
#[tokio::test]
async fn send_without_connection_returns_error() {
    let port = free_tcp_port().await;
    let (hub, _events, task) = start_session(SessionConfig::new(
        "device-a",
        "PC-a",
        port,
        Duration::from_millis(200),
    ))
    .await
    .expect("start session");

    match hub.send_text("nobody", "hi").await {
        Err(SessionError::NotConnected(id)) => assert_eq!(id, "nobody"),
        other => panic!("expected NotConnected, got {other:?}"),
    }

    task.abort();
}

/// 正常路径：连接建立后双方都收到 PeerConnected（携带对端身份）。
#[tokio::test]
async fn session_events_carry_peer_identity() {
    let port_b = free_tcp_port().await;
    let port_a = free_tcp_port().await;

    let (_hub_b, mut events_b, task_b) = start_session(SessionConfig::new(
        "device-b",
        "PC-b",
        port_b,
        Duration::from_secs(1),
    ))
    .await
    .expect("start session b");

    let (hub_a, mut events_a, task_a) = start_session(SessionConfig::new(
        "device-a",
        "PC-a",
        port_a,
        Duration::from_secs(1),
    ))
    .await
    .expect("start session a");

    hub_a.connect(loopback(port_b)).await.expect("connect");

    let connected_b = timeout(Duration::from_secs(3), events_b.recv())
        .await
        .expect("timed out")
        .expect("closed");
    match connected_b {
        SessionEvent::PeerConnected {
            device_id, name, ..
        } => {
            assert_eq!(device_id, "device-a");
            assert_eq!(name, "PC-a");
        }
        other => panic!("expected PeerConnected, got {other:?}"),
    }

    let connected_a = timeout(Duration::from_secs(3), events_a.recv())
        .await
        .expect("timed out")
        .expect("closed");
    match connected_a {
        SessionEvent::PeerConnected { device_id, .. } => assert_eq!(device_id, "device-b"),
        other => panic!("expected PeerConnected, got {other:?}"),
    }

    task_a.abort();
    task_b.abort();
}
