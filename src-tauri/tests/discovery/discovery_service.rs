//! 集成测试：真实 UDP 套接字上的设备发现流程（M2-a）。
//!
//! 覆盖 rules.md R2.4 要求的正常路径、边界与异常用例：
//! - 对端 announce → 本机产生 DeviceUp；
//! - 对端 query → 本机单播回应 announce；
//! - 对端 bye → 本机产生 DeviceBye；
//! - 自己发出的包 / 非法字节被忽略，不产生事件。

use chatters_lib::discovery::{start_discovery, DiscoveryEvent, DiscoveryPacket, DiscoveryType};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time::timeout;

/// 取一个当前空闲的 UDP 端口（绑定 0 后立即释放）。
async fn free_udp_port() -> u16 {
    let socket = UdpSocket::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    socket.local_addr().expect("local addr").port()
}

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], port))
}

/// 正常路径：收到对端 announce 后产生 DeviceUp 事件，字段与包一致。
#[tokio::test]
async fn announce_from_peer_emits_device_up() {
    let port = free_udp_port().await;
    let (mut rx, task) = start_discovery(
        port,
        "local-device".to_string(),
        "PC-local".to_string(),
        28101,
    )
    .await
    .expect("start discovery");

    let peer = UdpSocket::bind("127.0.0.1:0").await.expect("peer bind");
    let announce = DiscoveryPacket::new(DiscoveryType::Announce, "peer-1", "PC-peer", 28102);
    peer.send_to(&announce.encode(), loopback(port))
        .await
        .expect("send announce");

    let event = timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("timed out waiting for device up")
        .expect("discovery channel closed");

    match event {
        DiscoveryEvent::DeviceUp(entry) => {
            assert_eq!(entry.device_id, "peer-1");
            assert_eq!(entry.name, "PC-peer");
            assert_eq!(entry.tcp_port, 28102);
            assert_eq!(entry.addr.ip().to_string(), "127.0.0.1");
        }
        other => panic!("expected DeviceUp, got {other:?}"),
    }

    task.abort();
}

/// 正常路径：收到 query 时单播回应 announce（携带本机 device_id）。
#[tokio::test]
async fn query_is_answered_with_announce() {
    let port = free_udp_port().await;
    let (_rx, task) = start_discovery(
        port,
        "local-device-2".to_string(),
        "PC-local-2".to_string(),
        28101,
    )
    .await
    .expect("start discovery");

    let peer = UdpSocket::bind("127.0.0.1:0").await.expect("peer bind");
    let query = DiscoveryPacket::new(DiscoveryType::Query, "peer-2", "PC-peer", 28102);
    peer.send_to(&query.encode(), loopback(port))
        .await
        .expect("send query");

    let mut buf = [0u8; 2048];
    let (len, _from) = timeout(Duration::from_secs(3), peer.recv_from(&mut buf))
        .await
        .expect("timed out waiting for announce reply")
        .expect("receive reply");
    let packet = DiscoveryPacket::decode(&buf[..len]).expect("decode announce");
    assert_eq!(packet.packet_type, DiscoveryType::Announce);
    assert_eq!(packet.device_id, "local-device-2");
    assert_eq!(packet.tcp_port, 28101);

    task.abort();
}

/// 边界：收到 bye 后产生 DeviceBye 事件。
#[tokio::test]
async fn bye_from_peer_emits_device_bye() {
    let port = free_udp_port().await;
    let (mut rx, task) = start_discovery(
        port,
        "local-device-3".to_string(),
        "PC-local-3".to_string(),
        28101,
    )
    .await
    .expect("start discovery");

    let peer = UdpSocket::bind("127.0.0.1:0").await.expect("peer bind");
    let bye = DiscoveryPacket::new(DiscoveryType::Bye, "peer-3", "PC-peer", 28102);
    peer.send_to(&bye.encode(), loopback(port))
        .await
        .expect("send bye");

    let event = timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("timed out waiting for device bye")
        .expect("discovery channel closed");
    assert_eq!(event, DiscoveryEvent::DeviceBye("peer-3".to_string()));

    task.abort();
}

/// 异常：非法字节与本机自己的包都被忽略，不产生任何事件。
#[tokio::test]
async fn malformed_and_self_packets_are_ignored() {
    let port = free_udp_port().await;
    let (mut rx, task) = start_discovery(
        port,
        "local-device-4".to_string(),
        "PC-local-4".to_string(),
        28101,
    )
    .await
    .expect("start discovery");

    let peer = UdpSocket::bind("127.0.0.1:0").await.expect("peer bind");
    peer.send_to(b"not a discovery packet", loopback(port))
        .await
        .expect("send garbage");
    let own = DiscoveryPacket::new(
        DiscoveryType::Announce,
        "local-device-4",
        "PC-local-4",
        28101,
    );
    peer.send_to(&own.encode(), loopback(port))
        .await
        .expect("send self announce");

    assert!(
        timeout(Duration::from_millis(500), rx.recv())
            .await
            .is_err(),
        "self or malformed packets must not produce events"
    );

    task.abort();
}
