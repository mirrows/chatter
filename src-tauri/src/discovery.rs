//! UDP 设备发现模块（M2-a）。
//!
//! 协议见 requirements/02-technical-design.md 第 3 节：
//! - 启动时广播 `query`，并每 5 秒广播 `announce`（心跳）。
//! - 收到 `query` 单播回应 `announce`。
//! - 15 秒未收到某设备任何包则标记离线。
//!
//! 本模块为纯 Rust（无 Tauri 依赖），可独立单元测试。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

/// 发现协议 magic，过滤无关包。
pub const MAGIC: &str = "CHTRS";
/// 协议版本。
pub const PROTOCOL_VERSION: u8 = 1;
/// announce 心跳间隔。
pub const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(5);
/// 超过该时长未收到某设备包则判定离线。
pub const OFFLINE_AFTER: Duration = Duration::from_secs(15);

/// 发现包类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiscoveryType {
    /// 请求发现（广播）。
    Query,
    /// 自我宣告（广播或单播回应）。
    Announce,
    /// 退出通知。
    Bye,
}

/// 发现包结构（JSON over UDP）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryPacket {
    /// 固定 magic "CHTRS"。
    pub magic: String,
    /// 协议版本。
    pub ver: u8,
    /// 包类型。
    #[serde(rename = "type")]
    pub packet_type: DiscoveryType,
    /// 本机唯一 ID。
    pub device_id: String,
    /// 设备昵称。
    pub name: String,
    /// 对端 TCP 端口。
    pub tcp_port: u16,
}

impl DiscoveryPacket {
    /// 构造一个发现包。
    pub fn new(packet_type: DiscoveryType, device_id: &str, name: &str, tcp_port: u16) -> Self {
        Self {
            magic: MAGIC.to_string(),
            ver: PROTOCOL_VERSION,
            packet_type,
            device_id: device_id.to_string(),
            name: name.to_string(),
            tcp_port,
        }
    }

    /// 序列化为 JSON 字节。
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// 从 JSON 字节反序列化；magic 不符或版本不匹配返回 None。
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p: DiscoveryPacket = serde_json::from_slice(bytes).ok()?;
        if p.magic != MAGIC || p.ver != PROTOCOL_VERSION {
            return None;
        }
        Some(p)
    }
}

/// 在线设备信息。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceEntry {
    /// 设备唯一 ID。
    pub device_id: String,
    /// 设备昵称。
    pub name: String,
    /// 最近一次发现包的来源地址。
    pub addr: SocketAddr,
    /// 对端 TCP 端口。
    pub tcp_port: u16,
    /// 最近一次收到包的时间。
    pub last_seen: Instant,
}

/// 在线设备表：负责按 device_id 去重、心跳超时离线判定。
#[derive(Debug, Default)]
pub struct DeviceTable {
    devices: HashMap<String, DeviceEntry>,
}

impl DeviceTable {
    /// 记录一次发现包；相同 device_id 刷新 last_seen。
    pub fn upsert(&mut self, packet: &DiscoveryPacket, addr: SocketAddr) -> DeviceEntry {
        let entry = DeviceEntry {
            device_id: packet.device_id.clone(),
            name: packet.name.clone(),
            addr,
            tcp_port: packet.tcp_port,
            last_seen: Instant::now(),
        };
        self.devices.insert(packet.device_id.clone(), entry.clone());
        entry
    }

    /// 移除某设备（收到 Bye 或手动删除时）。
    pub fn remove(&mut self, device_id: &str) -> Option<DeviceEntry> {
        self.devices.remove(device_id)
    }

    /// 清理超时设备，返回被移除的设备列表。
    pub fn sweep_offline(&mut self) -> Vec<DeviceEntry> {
        let now = Instant::now();
        let offline: Vec<String> = self
            .devices
            .iter()
            .filter(|(_, e)| now.duration_since(e.last_seen) > OFFLINE_AFTER)
            .map(|(id, _)| id.clone())
            .collect();
        offline
            .into_iter()
            .filter_map(|id| self.devices.remove(&id))
            .collect()
    }

    /// 当前在线设备快照。
    pub fn list(&self) -> Vec<DeviceEntry> {
        let mut v: Vec<DeviceEntry> = self.devices.values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// 在线设备数。
    pub fn len(&self) -> usize {
        self.devices.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }
}

/// 发现服务事件，推送给上层（前端/会话模块）。
#[derive(Debug, Clone, PartialEq)]
pub enum DiscoveryEvent {
    /// 设备上线或信息刷新。
    DeviceUp(DeviceEntry),
    /// 设备离线。
    DeviceDown(String),
    /// 收到 Bye。
    DeviceBye(String),
}

/// 受限广播地址（255.255.255.255）上的指定端口。
///
/// 纯函数，便于测试；端口必须与监听端口一致，否则对端收不到广播。
pub fn broadcast_addr(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::BROADCAST, port))
}

/// 启动发现服务：绑定 UDP 端口，广播 announce/query，回应 query。
///
/// `local_device_id`/`local_name` 用于本机 announce 包；
/// `tcp_port` 为本机 TCP 监听端口。
/// 返回事件接收端，由调用方消费。
pub async fn start_discovery(
    bind_port: u16,
    local_device_id: String,
    local_name: String,
    tcp_port: u16,
) -> Result<(mpsc::Receiver<DiscoveryEvent>, tokio::task::JoinHandle<()>), std::io::Error> {
    let socket = UdpSocket::bind(("0.0.0.0", bind_port)).await?;
    socket.set_broadcast(true)?;
    let (tx, rx) = mpsc::channel::<DiscoveryEvent>(64);

    let handle = tokio::spawn(async move {
        let mut table = DeviceTable::default();
        let mut announce = tokio::time::interval(ANNOUNCE_INTERVAL);
        let mut sweep = tokio::time::interval(Duration::from_secs(1));
        let mut buf = [0u8; 2048];

        let ann = DiscoveryPacket::new(
            DiscoveryType::Announce,
            &local_device_id,
            &local_name,
            tcp_port,
        );
        let query = DiscoveryPacket::new(
            DiscoveryType::Query,
            &local_device_id,
            &local_name,
            tcp_port,
        );
        // 发送目标：受限广播地址 + 本机监听端口（端口必须与 bind 一致）。
        let broadcast_addr = broadcast_addr(bind_port);

        let _ = socket.send_to(&query.encode(), broadcast_addr).await;
        let _ = socket.send_to(&ann.encode(), broadcast_addr).await;

        loop {
            tokio::select! {
                _ = announce.tick() => {
                    let _ = socket.send_to(&ann.encode(), broadcast_addr).await;
                }
                _ = sweep.tick() => {
                    for d in table.sweep_offline() {
                        let _ = tx.send(DiscoveryEvent::DeviceDown(d.device_id)).await;
                    }
                }
                Ok((len, from)) = socket.recv_from(&mut buf) => {
                    let Some(packet) = DiscoveryPacket::decode(&buf[..len]) else { continue };
                    // 忽略自己
                    if packet.device_id == local_device_id { continue; }
                    match packet.packet_type {
                        DiscoveryType::Query => {
                            // 单播回应 announce
                            let _ = socket.send_to(&ann.encode(), from).await;
                        }
                        DiscoveryType::Announce => {
                            let entry = table.upsert(&packet, from);
                            let _ = tx.send(DiscoveryEvent::DeviceUp(entry)).await;
                        }
                        DiscoveryType::Bye => {
                            table.remove(&packet.device_id);
                            let _ = tx.send(DiscoveryEvent::DeviceBye(packet.device_id)).await;
                        }
                    }
                }
                else => break,
            }
        }
    });

    Ok((rx, handle))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正常路径：announce 包编解码往返一致。
    #[test]
    fn packet_encode_decode_roundtrip() {
        let p = DiscoveryPacket::new(DiscoveryType::Announce, "id-1", "PC-abc", 28101);
        let bytes = p.encode();
        let decoded = DiscoveryPacket::decode(&bytes).unwrap();
        assert_eq!(decoded, p);
    }

    /// 异常：乱码字节应返回 None 而非 panic。
    #[test]
    fn decode_garbage_returns_none() {
        assert!(DiscoveryPacket::decode(b"not json at all").is_none());
        assert!(DiscoveryPacket::decode(&[]).is_none());
    }

    /// 边界：magic 不符（伪造包）被过滤。
    #[test]
    fn decode_wrong_magic_filtered() {
        let mut p = DiscoveryPacket::new(DiscoveryType::Announce, "id-1", "PC-abc", 28101);
        p.magic = "EVIL".to_string();
        assert!(DiscoveryPacket::decode(&p.encode()).is_none());
    }

    /// 正常路径：设备表 upsert 去重并刷新。
    #[test]
    fn device_table_upsert_dedupes() {
        let mut table = DeviceTable::default();
        let p = DiscoveryPacket::new(DiscoveryType::Announce, "id-1", "PC-abc", 28101);
        let addr: SocketAddr = "192.168.1.10:5000".parse().unwrap();
        table.upsert(&p, addr);
        table.upsert(&p, addr);
        assert_eq!(table.len(), 1);
        assert_eq!(table.list()[0].device_id, "id-1");
    }

    /// 边界：设备表移除后为空。
    #[test]
    fn device_table_remove() {
        let mut table = DeviceTable::default();
        let p = DiscoveryPacket::new(DiscoveryType::Announce, "id-1", "PC-abc", 28101);
        let addr: SocketAddr = "192.168.1.10:5000".parse().unwrap();
        table.upsert(&p, addr);
        assert_eq!(table.remove("id-1").unwrap().device_id, "id-1");
        assert!(table.is_empty());
    }

    /// 边界：广播地址端口必须跟随配置端口，而不是硬编码 28100。
    #[test]
    fn broadcast_addr_follows_configured_port() {
        assert_eq!(broadcast_addr(28100).to_string(), "255.255.255.255:28100");
        assert_eq!(broadcast_addr(39000).to_string(), "255.255.255.255:39000");
    }

    /// 异常：协议版本不一致的包被过滤（避免与 v2 客户端互相污染）。
    #[test]
    fn decode_wrong_version_filtered() {
        let mut p = DiscoveryPacket::new(DiscoveryType::Announce, "id-1", "PC-abc", 28101);
        p.ver = PROTOCOL_VERSION + 1;
        assert!(DiscoveryPacket::decode(&p.encode()).is_none());
    }

    /// 边界：超时设备被 sweep 移除，返回离线列表。
    #[test]
    fn device_table_sweep_offline_removes_stale() {
        let mut table = DeviceTable::default();
        let p = DiscoveryPacket::new(DiscoveryType::Announce, "id-1", "PC-abc", 28101);
        let addr: SocketAddr = "192.168.1.10:5000".parse().unwrap();
        table.upsert(&p, addr);
        // 手动把 last_seen 回拨到离线阈值之外。
        if let Some(entry) = table.devices.get_mut("id-1") {
            entry.last_seen = Instant::now() - OFFLINE_AFTER - Duration::from_secs(1);
        }
        let offline = table.sweep_offline();
        assert_eq!(offline.len(), 1);
        assert_eq!(offline[0].device_id, "id-1");
        assert!(table.is_empty());
    }
}
