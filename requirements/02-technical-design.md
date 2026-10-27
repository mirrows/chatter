# Chatters 技术方案

> 版本：v0.1　日期：2026-10-27　对应需求文档：01 v0.3

## 1. 总体架构

```
┌──────────────────────────── 客户端 A ────────────────────────────┐
│  ┌──────────────┐   invoke/event   ┌───────────────────────────┐ │
│  │  Vue 3 前端   │ ◄──────────────► │  Rust 后端 (Tauri)        │ │
│  │  聊天/列表/设置 │                  │  discovery / session /    │ │
│  └──────────────┘                  │  transfer / storage       │ │
└────────────────────────────────────┴───────────────────────────┴─┘
        ▲ UDP 发现(广播)                │ TCP (消息/图片/文件)
        ▼                              ▼
┌──────────────────────────── 客户端 B ────────────────────────────┐
│              同样的结构（Rust 网络层完全对等复用）                    │
└──────────────────────────────────────────────────────────────────┘
```

- **前端**：Vue 3 + Vite + TypeScript，Pinia 状态管理，只负责 UI 与交互，不接触网络。
- **后端**：Rust（tokio 异步运行时），承载全部网络 IO、文件读写、协议解析，通过 Tauri `command` / `event` 与前端通信。
- 对等架构：每个客户端既是服务端（监听）也是客户端（发起连接），无中转服务器。

## 2. 端口与协议总览

| 用途 | 协议 | 端口 |
|---|---|---|
| 设备发现 | UDP 广播 | 28100（可配置） |
| 通信连接 | TCP | 28101（可配置） |

一个 TCP 连接承载一次"业务会话"，连接内部使用统一的帧协议，通过帧头 `frame_type` 区分消息、图片元数据、文件块等。文件大数据与控制帧共用连接（分块发送，控制帧可在块间隙插入），简化连接管理。

## 3. 设备发现协议（UDP）

### 3.1 发现包结构（JSON over UDP，v1 简化选择；v2 可换二进制）

```json
{
  "magic": "CHTRS",          // 校验，过滤无关包
  "ver": 1,                  // 协议版本
  "type": "announce",        // announce | query | bye
  "device_id": "uuid-v4",    // 本机唯一 ID（首装生成，持久化）
  "name": "PC-8f3a",         // 设备昵称
  "tcp_port": 28101
}
```

- 启动时：广播 `query`，并每 5 秒广播一次 `announce`（携带 TTL 心跳语义）。
- 收到 `query`：单播回应 `announce`。
- **在线判定**：15 秒未收到某设备任何包则标记离线。
- 多网卡（多 IP）时对每个非环回 IPv4 地址分别广播。
- F3 手动添加：直接对目标 IP 的 TCP 端口发起握手（见 4.2），成功即视为在线，也向其所在网段定向广播。

## 4. TCP 帧协议

### 4.1 帧格式（二进制，大端序）

```
┌────────┬────────┬──────────┬─────────────────┐
│ magic  │ type   │ length   │ payload         │
│ 4B     │ 1B     │ 4B       │ length 字节      │
└────────┴────────┴──────────┴─────────────────┘
magic = "CHTRS"；length ≤ 4MB（文件块最大 1MB）
```

### 4.2 帧类型

| type | 名称 | payload | 方向 |
|---|---|---|---|
| 0x01 | HELLO | JSON：device_id/name/ver | 双向，连接后首个帧 |
| 0x02 | TEXT | JSON：{msg_id, text, ts} | → |
| 0x03 | TEXT_ACK | JSON：{msg_id} | ←（送达回执，F11） |
| 0x10 | FILE_OFFER | JSON：{transfer_id, kind(image/file), filename, size, sha256} | → |
| 0x11 | FILE_ACCEPT | JSON：{transfer_id, resume_offset} | ←（简版续传：带已收偏移） |
| 0x12 | FILE_REJECT | JSON：{transfer_id} | ← |
| 0x13 | FILE_CANCEL | JSON：{transfer_id} | 双向（F33 取消，终止传输） |
| 0x14 | FILE_CHUNK | transfer_id(16B) + seq(4B) + data | →（≤1MB/块） |
| 0x15 | FILE_DONE | JSON：{transfer_id, sha256} | → |
| 0x16 | FILE_RESULT | JSON：{transfer_id, ok, path} | ←（校验结果） |
| 0x1F | PING / PONG | 空 | 双向心跳（30s 间隔） |

### 4.3 握手与接收策略（对应 F34）

1. 建连 → 双向 HELLO → 前端更新设备状态。
2. 发送文字/图片/文件前，发送方先本地创建 `transfer_id`（uuid）。
3. **图片**：`FILE_OFFER(kind=image)` → 接收端按"自动接收"策略直接回 `FILE_ACCEPT`，随后接收数据至缓存目录并在聊天窗预览。
4. **文件**：`FILE_OFFER` → 接收端弹确认框；用户接受 → `FILE_ACCEPT`（offset=0 或续传偏移）；拒绝/超时 → `FILE_REJECT`。
5. 文字：直接 `TEXT`，收到 `TEXT_ACK` 置为已送达，3 秒未 ACK 显示失败可重发。

### 4.4 简版断点续传（Q8）

- 双方在磁盘/内存记录：`transfer_id, filename, size, sha256, received_offset`（接收方）。
- 传输中连接断开：发送方保留任务为"已中断"，展示重试按钮。
- 重试时重走 OFFER/ACCEPT；`FILE_ACCEPT` 中接收方返回 `resume_offset`（按接收方收到的最后一个完整块边界），发送方 `seek` 到该偏移继续发送。
- 匹配依据为 filename+size（v1 不比 hash 中途值，sha256 仅端到端校验）；对端无记录则 offset=0 从头传。

### 4.5 完整性校验（F36）

- 发送前计算全文件 sha256（流式），写入 `FILE_OFFER`。
- 接收端边收边以同一 sha256 计算器累积，`FILE_DONE` 后比对：一致 → 改名为正式文件名并回 `FILE_RESULT ok`；不一致 → 丢弃临时文件并报错。

## 5. Rust 端模块划分

```
src-tauri/src/
├── main.rs            // Tauri 入口，注册 commands
├── config.rs          // 配置读写（昵称/端口/下载目录/主题）
├── discovery.rs       // UDP 广播/监听，在线设备表（tokio 广播通道通知前端）
├── protocol.rs        // 帧编解码、帧类型常量
├── session.rs         // TCP 监听 accept、连接管理、心跳、HELLO 握手
├── transfer/
│   ├── mod.rs         // 传输任务表、任务状态机、进度事件
│   ├── sender.rs      // OFFER/分块发送/续传/取消
│   └── receiver.rs    // 确认流程/落盘/校验
├── storage.rs         // 历史记录（SQLite，见下）、消息与传输记录 CRUD
└── commands.rs        // 前端可调用的 Tauri command 封装
```

- **关键依赖**：`tokio`（异步 IO）、`serde/serde_json`、`uuid`、`sha2`、`rusqlite`（或 `sqlx`）、`tauri-plugin-dialog/notification/autostart/single-instance`。
- **状态机（transfer）**：`pending → offered → accepting(等确认) → transferring → done | failed | cancelled | interrupted`，所有变更通过 Tauri event `transfer://update` 推给前端。
- **并行会话（F42）**：每个对端连接一个 task；每个传输任务独立 task + 取消令牌（`CancellationToken`），多任务并发互不阻塞。
- **进度事件**：每收到/发出一个块向前端发一次节流事件（≥100ms 间隔），UI 不卡顿（3.1）。

## 6. 数据存储

| 数据 | 方式 | 说明 |
|---|---|---|
| 配置 | JSON 文件（app data 目录） | 昵称、device_id、端口、下载目录、主题 |
| 历史 | SQLite `history.db` | 消息表（id/peer/type/text/状态/时间）、传输表（transfer_id/peer/文件名/大小/状态/offset/时间） |
| 接收文件 | 下载目录（默认 `下载/Chatters`） | 传输中写 `.part` 临时文件，校验通过后改名 |
| 历史图片缓存 | app cache 目录 | 按 transfer_id 存放，清空历史时可一并删除（F41） |

## 7. 前端结构（Vue 3）

```
src/
├── views/    DeviceList.vue  Chat.vue  Settings.vue
├── stores/   devices.ts  chat.ts  transfers.ts  settings.ts  (Pinia)
├── api/      tauri.ts   // invoke 封装 + 事件订阅（类型化）
└── components/  MessageBubble.vue  TransferCard.vue  ...
```

主要页面：

1. **设备列表**：在线设备、手动添加 IP、上下线提示（F1/F3/F4）。
2. **聊天页**：消息气泡、图片缩略图预览、文件传输卡片（进度/速度/取消/重试）、文件接收确认弹窗（F34）。
3. **设置页**：昵称、下载目录、端口、主题、开机自启、清空历史。

关键交互：

- Ctrl+V 粘贴图片 → 前端读剪贴板 → 调后端 command 发送（F21）。
- 拖拽：前端处理 drop 事件，路径交后端；目录则前端提示"请压缩为 zip"（F31）。
- 托盘与通知：Tauri 插件，最小化到托盘 + 新消息/文件系统通知（F51）。

## 8. Tauri Command / Event 接口清单（草案）

| Command | 说明 |
|---|---|
| `list_devices` / `add_device_by_ip(ip)` | 设备列表 / 手动添加 |
| `send_text(peer_id, text)` / `resend(msg_id)` | 文字消息 / 重发 |
| `send_file(peer_id, path)` | 图片或文件（后端按扩展名/大小分流） |
| `accept_file(transfer_id)` / `reject_file(transfer_id)` | 接收确认 |
| `cancel_transfer(transfer_id)` / `retry_transfer(transfer_id)` | 终止 / 重试（简版续传） |
| `get_history(peer_id)` / `clear_history(peer_id?)` | 历史读取/清空 |
| `get_settings` / `save_settings` | 配置 |

事件：`device://update`、`message://new`、`transfer://update`。

## 9. 风险与对策

| 风险 | 对策 |
|---|---|
| Windows 防火墙拦截 UDP/TCP 监听 | 首次启动弹窗引导放行；安装包内配置防火墙规则（NSIS 脚本） |
| 广播在某些路由器/AP 隔离下不可用 | F3 手动 IP 添加兜底 |
| 大文件写盘拖慢吞吐 | 1MB 块 + BufWriter 顺序写；后续可加 4MB 块 |
| 哈希计算占用 CPU | 发送端 sha256 在 OFFER 前一次性流式算好并缓存；接收端边收边算 |
| Windows WebView2 差异 | Tauri 自带 WebView2 引导安装 |

## 10. 开发里程碑细化

1. **M2-a**：工程脚手架（Tauri+Vue）、配置与发现模块，设备列表可见。
2. **M2-b**：TCP 帧协议 + HELLO + 文字消息收发与 ACK。
3. **M3-a**：FILE_OFFER/ACCEPT/CHUNK/DONE 主流程，图片自动接收与预览。
4. **M3-b**：文件确认接收、进度/取消、sha256 校验、简版续传。
5. **M4**：SQLite 历史、托盘/通知/自启、设置页、清空历史。
6. **M5**：Windows 安装包（NSIS）、防火墙引导、使用说明；此后进入 macOS/Linux/Android 移植评估。
