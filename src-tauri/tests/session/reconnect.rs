//! 集成测试：断线自动重连（v2/M10）。
//!
//! 场景：A 连上 B → B 进程消失（任务被中止）→ A 收到断开事件并自动重连 →
//! B 在同一端口重启 → A 重新建立会话（无需用户操作）。

use chatters_lib::session::{start_session, SessionConfig, SessionEvent, SessionHub};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::timeout;

/// 取一个空闲 TCP 端口。
async fn free_tcp_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    port
}

/// 在指定端口启动会话中心（端口占用时短暂重试）。
async fn start_hub(
    device_id: &str,
    port: u16,
    delays: Vec<u64>,
) -> (
    SessionHub,
    tokio::sync::mpsc::Receiver<SessionEvent>,
    tokio::task::JoinHandle<()>,
) {
    for _ in 0..50 {
        let mut config = SessionConfig::new(device_id, device_id, port, Duration::from_secs(1));
        config.reconnect_delays = delays.clone();
        if let Ok(started) = start_session(config).await {
            return started;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("failed to bind port {port}");
}

/// 等待指定对端重新连上（跳过重连提示事件）。
async fn wait_connected(
    events: &mut tokio::sync::mpsc::Receiver<SessionEvent>,
    peer_id: &str,
) -> bool {
    loop {
        match timeout(Duration::from_secs(10), events.recv()).await {
            Ok(Some(SessionEvent::PeerConnected { device_id, .. })) if device_id == peer_id => {
                return true;
            }
            Ok(Some(_)) => continue,
            _ => return false,
        }
    }
}

/// 正常路径：对端重启后自动重连成功，且期间产生 PeerReconnecting 事件。
#[tokio::test]
async fn reconnects_after_peer_restart() {
    let port_b = free_tcp_port().await;
    let port_a = free_tcp_port().await;

    let (hub_a, mut events_a, task_a) = start_hub("device-a", port_a, vec![1, 1, 1, 1, 1]).await;

    // 伪造 v2 对端：完成 HELLO（声明 tcp_port）后立即断开，模拟对端进程退出。
    let listener = TcpListener::bind(("127.0.0.1", port_b))
        .await
        .expect("bind fake peer");
    let fake = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let (mut stream, _) = listener.accept().await.expect("accept");
        let body = format!(
            r#"{{"device_id":"device-b","name":"device-b","ver":1,"features":["chunk2m"],"tcp_port":{port_b}}}"#
        );
        let mut frame = Vec::new();
        frame.extend_from_slice(b"CHTRS");
        frame.push(0x01);
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.extend_from_slice(body.as_bytes());
        let _ = stream.write_all(&frame).await;
        let _ = stream.shutdown().await;
        // listener 在这里被释放，端口可被真实对端复用。
    });

    hub_a
        .connect(([127, 0, 0, 1], port_b).into())
        .await
        .expect("connect fake peer");
    assert!(wait_connected(&mut events_a, "device-b").await);

    match timeout(Duration::from_secs(5), events_a.recv()).await {
        Ok(Some(SessionEvent::PeerDisconnected { device_id })) => {
            assert_eq!(device_id, "device-b");
        }
        other => panic!("expected PeerDisconnected, got {other:?}"),
    }
    fake.await.expect("fake peer task");

    // 对端在同一端口重启，A 应自动重连成功。
    let (_hub_b, mut events_b, task_b) = start_hub("device-b", port_b, vec![1, 1, 1, 1, 1]).await;
    let mut saw_reconnecting = false;
    loop {
        match timeout(Duration::from_secs(10), events_a.recv()).await {
            Ok(Some(SessionEvent::PeerReconnecting { device_id, .. })) => {
                assert_eq!(device_id, "device-b");
                saw_reconnecting = true;
            }
            Ok(Some(SessionEvent::PeerConnected { device_id, .. })) => {
                assert_eq!(device_id, "device-b");
                break;
            }
            other => panic!("reconnect not observed: {other:?}"),
        }
    }
    assert!(saw_reconnecting, "重连前应发出 PeerReconnecting 事件");
    assert!(wait_connected(&mut events_b, "device-a").await);
    assert!(hub_a.is_connected("device-b").await);

    task_a.abort();
    task_b.abort();
}

/// 边界：对端不广播监听端口（v1）时不尝试重连，只上报一次断开。
#[tokio::test]
async fn v1_peer_is_not_reconnected() {
    let port = free_tcp_port().await;
    let (hub, mut events, task) = start_hub("device-a", port, vec![1, 1, 1]).await;

    // 伪造 v1 对端：完成 HELLO 后立即关闭连接（HELLO 不含 tcp_port）。
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind fake peer");
    let fake_addr = listener.local_addr().expect("addr");
    let fake = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.expect("accept");
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut buf = [0u8; 512];
        let _ = stream.read(&mut buf).await;
        let body = br#"{"device_id":"device-old","name":"PC-old","ver":1}"#;
        let mut frame = Vec::new();
        frame.extend_from_slice(b"CHTRS");
        frame.push(0x01);
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.extend_from_slice(body);
        let _ = stream.write_all(&frame).await;
        // 关闭连接触发断开。
    });

    hub.connect(fake_addr).await.expect("connect fake");
    assert!(wait_connected(&mut events, "device-old").await);
    match timeout(Duration::from_secs(5), events.recv()).await {
        Ok(Some(SessionEvent::PeerDisconnected { device_id })) => {
            assert_eq!(device_id, "device-old");
        }
        other => panic!("expected PeerDisconnected, got {other:?}"),
    }
    // 1.5 秒内不应出现重连事件（v1 对端没有可用端口）。
    let no_reconnect = timeout(Duration::from_millis(1500), events.recv()).await;
    assert!(
        no_reconnect.is_err(),
        "v1 对端不应触发重连，实际收到 {no_reconnect:?}"
    );

    fake.abort();
    task.abort();
}
