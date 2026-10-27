// 与 Rust 端共享的设备相关类型（对应 discovery.rs 的 DeviceEntry）。
// 注意：两侧定义需保持同步（rules.md R3.5）。

/** 在线设备信息（与 Rust DeviceEntry 对应）。 */
export interface DeviceEntry {
  /** 设备唯一 ID。 */
  deviceId: string;
  /** 设备昵称。 */
  name: string;
  /** 最近一次发现包的来源地址 "ip:port"。 */
  addr: string;
  /** 对端 TCP 端口。 */
  tcpPort: number;
}

/** 发现事件（与 Rust DiscoveryEvent 对应，snake_case 已转 camelCase）。 */
export type DiscoveryEvent =
  | { event: "device_up"; payload: DeviceEntry }
  | { event: "device_down"; payload: string }
  | { event: "device_bye"; payload: string };
