// 与 Rust 端共享的设备相关类型（对应 discovery.rs / commands.rs 的 DTO）。
// 注意：两侧定义需保持同步（rules.md R3.5）。

/** 在线设备信息（与 Rust DeviceDto 对应）。 */
export interface DeviceEntry {
  /** 设备唯一 ID。 */
  deviceId: string;
  /** 设备昵称。 */
  name: string;
  /** 最近一次发现包的来源地址 "ip:port"。 */
  addr: string;
  /** 对端 TCP 端口。 */
  tcpPort: number;
  /** 设备指纹（v2/M10）：SHA-256 前 8 字节，供人工核对。 */
  fingerprint: string;
}

/** 本机信息（与 Rust LocalDeviceDto 对应）。 */
export interface LocalDevice {
  /** 本机唯一 ID。 */
  deviceId: string;
  /** 本机昵称。 */
  name: string;
  /** UDP 发现端口。 */
  discoveryPort: number;
  /** TCP 通信端口。 */
  tcpPort: number;
  /** 本机指纹（v2/M10）。 */
  fingerprint: string;
  /** 运行平台（v2/M9）：用于隐藏平台不支持的操作。 */
  platform: "android" | "windows" | "macos" | "linux";
}

/** 设备表变更事件（与 Rust DeviceUpdate 对应，`device://update` 事件载荷）。 */
export type DeviceUpdate = { kind: "up"; device: DeviceEntry } | { kind: "down"; deviceId: string };

/** 发现事件（与 Rust DiscoveryEvent 对应，后端内部使用）。 */
export type DiscoveryEvent =
  | { event: "device_up"; payload: DeviceEntry }
  | { event: "device_down"; payload: string }
  | { event: "device_bye"; payload: string };

/** 文字消息发送状态（与 Rust TextStatus 对应）。 */
export type TextStatus = "delivered" | "failed";

/** `send_text` 返回（与 Rust TextSendResultDto 对应）。 */
export interface TextSendResult {
  /** 后端生成的消息 ID。 */
  msgId: string;
  /** 送达状态。 */
  status: TextStatus;
}

/** 收到的新消息（与 Rust IncomingMessageDto 对应，`message://new` 载荷）。 */
export interface IncomingMessage {
  /** 发送方设备 ID。 */
  peerId: string;
  /** 消息 ID。 */
  msgId: string;
  /** 消息内容。 */
  text: string;
  /** 发送时间（Unix 毫秒）。 */
  ts: number;
}

/** 会话连接状态事件（与 Rust SessionUpdate 对应，`session://update` 载荷）。 */
export type SessionUpdate =
  | { kind: "connected"; peerId: string; name: string }
  | { kind: "disconnected"; peerId: string }
  | { kind: "reconnecting"; peerId: string; attempt: number }
  | { kind: "reconnectFailed"; peerId: string };

/** 传输方向。 */
export type TransferDirection = "send" | "receive";

/** 传输类型（图片自动接收，文件需确认）。 */
export type TransferKind = "image" | "file";

/** 传输状态（与 Rust TransferState 对应）。 */
export type TransferState =
  "pending" | "offered" | "transferring" | "done" | "failed" | "cancelled" | "rejected";

/** 传输任务（与 Rust TransferDto 对应，`transfer://update` 载荷）。 */
export interface TransferInfo {
  /** 任务 ID。 */
  transferId: string;
  /** 对端设备 ID。 */
  peerId: string;
  /** 方向。 */
  direction: TransferDirection;
  /** 类型。 */
  kind: TransferKind;
  /** 文件名。 */
  filename: string;
  /** 总字节数。 */
  size: number;
  /** 已传输字节数。 */
  transferred: number;
  /** 当前状态。 */
  state: TransferState;
  /** 接收完成后的落盘路径。 */
  path: string | null;
  /** 失败原因。 */
  error: string | null;
  /** 任务创建时间（Unix 毫秒），用于与文本消息按时间混排（v3）。 */
  ts: number;
}

/** 设置项（与 Rust SettingsDto 对应，F52）。 */
export interface Settings {
  /** 本机 ID（只读）。 */
  deviceId: string;
  /** 设备昵称。 */
  nickname: string;
  /** UDP 发现端口。 */
  discoveryPort: number;
  /** TCP 通信端口。 */
  tcpPort: number;
  /** 下载目录。 */
  downloadDir: string;
  /** 主题：light | dark。 */
  theme: "light" | "dark";
  /** 自动接收图片。 */
  autoAcceptImages: boolean;
  /** 自动接收普通文件。 */
  autoAcceptFiles: boolean;
  /** 是否开启定时清理（v3，移动端；默认关闭）。 */
  autoCleanEnabled: boolean;
  /** 定时清理保留天数（v3，默认 7，必须大于 0）。 */
  autoCleanDays: number;
}

/** 保存设置结果（与 Rust SaveSettingsResult 对应）。 */
export interface SaveSettingsResult {
  /** 昵称/端口变更后需重启生效。 */
  restartRequired: boolean;
}

/** 错误日志位置（与 Rust `ErrorLogInfo` 对应，v3）。 */
export interface ErrorLogInfo {
  /** 日志根目录（按天建子目录）。 */
  dir: string;
  /** 今天的日志文件路径。 */
  file: string;
  /** 目录里是否已有日志（决定设置页是否展示入口）。 */
  hasLogs: boolean;
}

/** 系统分享内容（v3，Android `ACTION_SEND`）。 */
export type SharePayload =
  { kind: "text"; text: string } | { kind: "file"; uri: string; name?: string | null };

/** 历史中的一条消息（与 Rust StoredMessage 对应）。 */
export interface StoredMessage {
  msgId: string;
  peerId: string;
  direction: "out" | "in";
  text: string;
  ts: number;
  status: "sending" | "delivered" | "failed";
}

/** 历史中的一条传输记录（与 Rust StoredTransfer 对应）。 */
export interface StoredTransfer {
  transferId: string;
  peerId: string;
  direction: TransferDirection;
  kind: TransferKind;
  filename: string;
  size: number;
  transferred: number;
  state: TransferState;
  path: string | null;
  /** 发送方向的本地源文件路径（v3：历史图片预览用）。 */
  sourcePath?: string | null;
  ts: number;
}

/** 历史快照（`get_history` 返回）。 */
export interface History {
  messages: StoredMessage[];
  transfers: StoredTransfer[];
}

/** 历史时间线条目（与 Rust `HistoryItem` 对应，v3 分页用）。 */
export type HistoryItem =
  ({ itemType: "message" } & StoredMessage) | ({ itemType: "transfer" } & StoredTransfer);

/** 历史分页结果（与 Rust `HistoryPageDto` 对应）。 */
export interface HistoryPage {
  /** 按时间升序的条目，最多 `limit` 条。 */
  items: HistoryItem[];
  /** 是否还有更早的记录。 */
  hasMore: boolean;
}

/** 批量发送结果（与 Rust PathSendResult 对应，拖拽入口）。 */
export interface PathSendResult {
  /** 原始路径。 */
  path: string;
  /** 是否已发起传输。 */
  ok: boolean;
  /** 失败原因；`is_dir` 表示目录（F31 提示压缩为 zip）。 */
  error: string | null;
  /** 成功时的任务快照。 */
  transfer: TransferInfo | null;
}

/** 更新状态（v3，与 Rust `UpdateStatusDto` 对应）。 */
export interface UpdateStatus {
  /** 当前运行版本（`tauri.conf.json` 的 version）。 */
  currentVersion: string;
  /** 平台标识：windows / android / macos / linux。 */
  platform: string;
  /** 是否已有可回滚的旧版本备份。 */
  backupAvailable: boolean;
  /** 待确认的更新（更新安装后尚未确认健康）。 */
  pending: UpdatePending | null;
  /** 上次自动回滚记录（提示一次后清除）。 */
  lastRollback: UpdateRollback | null;
}

/** 待确认的更新。 */
export interface UpdatePending {
  /** 更新前版本。 */
  from: string;
  /** 更新后版本。 */
  to: string;
  /** 已启动次数。 */
  attempts: number;
}

/** 自动回滚记录。 */
export interface UpdateRollback {
  /** 出问题的版本。 */
  from: string;
  /** 回滚到的版本。 */
  to: string;
  /** 回滚时间（毫秒时间戳）。 */
  at: number;
}

/** 检查到的可用更新（v3，来自 GitHub Releases 的 latest.json）。 */
export interface AvailableUpdate {
  /** 新版本号。 */
  version: string;
  /** 当前版本号。 */
  currentVersion: string;
  /** 更新说明（可为空）。 */
  notes: string | null;
  /** 发布时间（可为空）。 */
  date: string | null;
}
