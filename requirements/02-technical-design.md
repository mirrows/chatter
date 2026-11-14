# Chatters 技术方案

> 版本：v0.2　日期：2026-10-28　对应需求文档：01 v0.4（历史里程碑报告仍引用当时的 v0.3，属正常）

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
- **每 10 秒广播一次 `query`**（v2/M9 补充）：Android 的 Wi-Fi 省电策略会过滤**入站广播**
  （`MulticastLock` 只覆盖组播），只依赖 announce 会让对端被误判离线；周期性 `query`
  迫使对端用**单播**回应 `announce`（单播不受过滤）。同一轮查询还会对已发现设备逐个**单播**
  `query`，作为广播收发全被拦截时的兜底。
- 收到 `query`：单播回应 `announce`。
- **在线判定**：45 秒未收到某设备任何包则标记离线（约 4 个查询周期，容忍广播丢失与短暂休眠）。
- 主动刷新：前端可在切回前台或发送失败时调用 `refresh_devices`，立即补发 `query + announce`。
- 多网卡（多 IP）时对每个非环回 IPv4 地址分别广播。
  - **v1 实现现状**：仅向受限广播地址 `255.255.255.255:<发现端口>` 发送（端口跟随配置）。
    按网卡分别广播需要枚举本机地址（如 `if-addrs` 依赖），当前离线构建环境无法引入，
    作为 M2-a 已知限制记录在 `04-milestone-m2-report.md`，不影响单网卡常见场景。
- F3 手动添加：直接对目标 IP 的 TCP 端口发起握手（见 4.2），成功即视为在线，也向其所在网段定向广播。

## 4. TCP 帧协议

### 4.1 帧格式（二进制，大端序）

```
┌────────┬────────┬──────────┬─────────────────┐
│ magic  │ type   │ length   │ payload         │
│ 5B     │ 1B     │ 4B       │ length 字节      │
└────────┴────────┴──────────┴─────────────────┘
magic = "CHTRS"（5 字节，与发现协议 magic 一致）；帧头共 10B；length ≤ 4MB（文件块最大 1MB）
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
| 0x17 | FILE_BLOCK_MAP | transfer_id(16B) + block_size(4B) + count(4B) + hashes(8B×count) | →（v2/M11，块级续传） |
| 0x18 | FILE_HAVE | transfer_id(16B) + count(4B) + bitmap(⌈count/8⌉B) | ←（v2/M11，已收块位图） |
| 0x19 | FILE_MISSING | transfer_id(16B) + count(4B) + indices(4B×count) | →（v2/M11，待发块，便于日志/UI） |
| 0x1F | PING / PONG | 空 | 双向心跳（30s 间隔） |

> 0x17–0x19 仅在双方 `HELLO.features` 均含 `chunkmap` 时使用（见 4.8），v1 客户端不会收到这些帧。

**心跳语义（v1 明确）**：`0x1F` 为单向"我还在"通知，收到后**不回应**；双方各自每 30 秒发送一次。
若双向互相回应会形成无限回环，故 v1 不使用"收到 ping 回 pong"的模式。

### 4.3 握手与接收策略（对应 F34）

1. 建连 → 双向 HELLO → 前端更新设备状态。
2. 发送文字/图片/文件前，发送方先本地创建 `transfer_id`（uuid）。
3. **图片**：`FILE_OFFER(kind=image)` → 接收端按"自动接收"策略直接回 `FILE_ACCEPT`，随后接收数据至缓存目录并在聊天窗预览。
4. **文件**：`FILE_OFFER` → 接收端弹确认框；用户接受 → `FILE_ACCEPT`（offset=0 或续传偏移）；拒绝/超时 → `FILE_REJECT`。
5. 文字：直接 `TEXT`，收到 `TEXT_ACK` 置为已送达，3 秒未 ACK 显示失败可重发。

### 4.4 简版断点续传（Q8，M3-b 已实现）

- 接收方在 `<下载目录>/.chatters-partial/` 下保存两份文件：
  - `<key>.part`：已收到的数据；
  - `<key>.json`：`{ filename, size, sha256 }` 元数据；
  - `key = sha256(filename + "|" + size)` 前 32 位十六进制（匹配依据为 filename+size，v1 不比中途哈希）。
- 发送方重试时重走 OFFER/ACCEPT；接收方命中可续传记录则返回 `FILE_ACCEPT.resume_offset = 已收字节数`，
  发送方 `seek` 到该偏移继续发送。
- 接收方续传时以 `append` 模式写入，并**先流式读取已有 `.part` 重建 sha256 累加器**，保证端到端校验仍然有效。
- 记录不匹配（文件名/大小/声明哈希不同）或长度异常时丢弃旧部分文件，从 0 开始。
- 清理时机：成功后删除 `.part.json` 并把 `.part` 改名为正式文件；用户取消/拒绝/校验失败时删除两份文件；
  空闲超时或连接断开时**保留**两份文件以便续传。

### 4.5 完整性校验（F36）

- 发送前计算全文件 sha256（流式），写入 `FILE_OFFER`。
- 接收端边收边以同一 sha256 计算器累积，`FILE_DONE` 后比对：一致 → 改名为正式文件名并回 `FILE_RESULT ok`；不一致 → 丢弃临时文件并报错。

### 4.6 中断与超时（M3-b）

- **空闲超时**：任务处于 `transferring` 且超过 30 秒没有数据进展 → 标记 `failed`（原因"传输超时，可重试"），保留部分文件。
- **连接断开**：会话层 `PeerDisconnected` → 该对端所有未完成任务立即标记 `failed`（原因"连接已断开，可重试"）。
- **重试**：`retry_transfer(transfer_id)` 以原源文件重新发起 OFFER，接收方据续传记录返回偏移。

### 4.7 块级续传（v2 / M11，经 `chunkmap` 协商启用）

**目标**：把 v1 的"字节偏移续传"升级为"块级校验 + 只补缺失块"，支持跨重启恢复与局部损坏重传。

1. **块表**：发送端一次顺序读文件，同时计算整文件 SHA-256 与每块 SHA-256；每块取摘要前 **8 字节** 写入 `FILE_BLOCK_MAP`。
   （2MB 块时 10GB = 5120 块 ≈ 40KB，远小于 4MB 帧上限；块哈希复用 `sha2`，无新增依赖。）
2. **流程**：`OFFER → ACCEPT → BLOCK_MAP → HAVE → （仅缺失块）CHUNK… → DONE → RESULT`。
3. **接收端**：`<key>.part` 用 `set_len(size)` 预分配；按 `seq × block_size` 随机写；写入前校验该块哈希，不匹配则丢弃并记为缺失。
4. **位图**：`<下载目录>/.chatters-partial/<key>.bitmap`（位序 = 块索引，尾部附 CRC32 校验）；每 32 块或 5s 原子落盘（临时文件 + 改名）。
5. **完成判定**：位图全满 **且** 整文件 SHA-256 匹配 → 改名为正式文件；否则回 `FILE_RESULT ok=false`，
   错误分类：`missing_blocks` / `block_hash_mismatch` / `file_hash_mismatch`。
6. **恢复**：重启加载位图（CRC 校验）；位图缺失/损坏 → 读 `.part` 逐块比对 BLOCK_MAP 重建；块大小不一致 → 丢弃旧块状态。
7. **降级**：对端未声明 `chunkmap` → 使用 v1 字节偏移续传（`FILE_ACCEPT.resume_offset`），其余流程不变。
8. **清理**：成功删位图并改名；取消/拒绝/校验失败删 `.part` + `.bitmap`；超时/断线保留两者。

### 4.8 版本与能力协商（v2 / M10）

- `HELLO` 的 JSON 增加可选字段 **`features: string[]`**；旧端无该字段时按空数组处理（serde 默认），因此**协议 `ver` 保持 1**。
- 已定义能力：

| 能力 | 作用 | 缺失时行为 |
|---|---|---|
| `chunk2m` | 使用 2MB 数据块 | 回落 1MB 块（v1 行为） |
| `chunkmap` | 启用块级续传（0x17–0x19） | 回落字节偏移续传 |

- 协商原则：**双方都声明才启用**；未知能力忽略并记录日志（便于后续扩展）；
- 兼容矩阵（必须回归）：

| 发送端 | 接收端 | 行为 |
|---|---|---|
| v2 | v2（均含两能力） | 2MB 块 + 块级续传 |
| v2 | v2（仅 `chunk2m`） | 2MB 块 + 偏移续传 |
| v2 | v1 | 1MB 块 + 偏移续传 |
| v1 | v2 | 1MB 块 + 偏移续传 |

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
| 配置 | JSON 文件（app config 目录 `config.json`） | 昵称、device_id、端口、下载目录、主题、自动接收开关 |
| 历史 | **JSON 文件 `history.json`**（M4；原计划 SQLite） | 消息数组（msgId/peerId/direction/text/ts/status）+ 传输数组（transferId/peerId/direction/kind/filename/size/transferred/state/path/ts）；写入采用 `临时文件 + 改名` 原子落盘，保留最近 2000 条消息 / 500 条传输 |
| 接收文件 | 下载目录（默认 `下载/Chatters`） | 传输中写 `.chatters-partial/<key>.part`，校验通过后改名 |
| 续传元数据 | 下载目录 `.chatters-partial/<key>.json` | `{ filename, size, sha256 }`，用于简版续传匹配 |
| 块续传位图（v2/M11） | 下载目录 `.chatters-partial/<key>.bitmap` | 块完成位图 + CRC32；每 32 块或 5s 原子落盘，重启后恢复 |

**偏差说明（R6.2）**：设计原定 SQLite（`rusqlite`/`sqlx`），但当前构建环境离线、无法引入新依赖；
v1 采用 JSON 文件实现同等语义（可读、按对端筛选、清空、重启后可见）。后续联网后可平滑替换为 SQLite。

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
| `list_devices` / `add_device_by_ip(addr)` | 设备列表 / 手动按 IP 添加（M4 之后补齐，F3） |
| `refresh_devices()` | 主动触发一次发现查询（v2/M9：切回前台或发送失败时刷新设备表） |
| `send_text(peer_id, text)` / `resend(msg_id)` | 文字消息 / 重发（**`resend` 未实现**，前端复用 `send_text`；见 §8 说明） |
| `send_file(peer_id, path)` | 图片或文件（后端按扩展名区分类型，返回任务快照） |
| `accept_file(transfer_id)` / `reject_file(transfer_id)` | 接收确认（F34） |
| `cancel_transfer(transfer_id)` / `retry_transfer(transfer_id)` | 终止 / 重试（重试为 M3-b 实现，按续传偏移继续） |
| `open_transfer_file(transfer_id)` / `reveal_transfer_file(transfer_id)` / `save_transfer_as(transfer_id)` | 打开文件 / 在资源管理器定位 / 另存为（P1，F23；v3：**发送方打开源文件、接收方打开落盘文件**） |
| `list_transfers()` | 传输任务列表（界面重建时使用） |
| `get_image_preview(transfer_id)` | 已接收图片的 data URL（F23 预览） |
| `pick_file(images_only)` | 打开系统文件选择框，返回绝对路径（F20/F30） |
| `send_paths(peer_id, paths)` | 批量发送拖拽路径（F22）；目录返回 `is_dir` 供前端提示压缩为 zip（F31） |
| `save_clipboard_image(data_base64, extension)` | 剪贴板图片落盘到缓存目录，返回路径（F21） |
| `get_settings` / `save_settings` | 设置读取/保存（M4；昵称/端口改动需重启生效） |
| `get_history(peer_id?)` / `clear_history(peer_id?, delete_files?)` | 历史读取/清空（M4，JSON 存储；`delete_files` 仅删下载目录内记录文件，P1/F41） |
| `open_download_dir()` | 打开下载目录（v3）：桌面端用文件管理器打开，目录不存在时先创建；Android 返回错误（前端隐藏入口） |
| `get_history_page(peer_id?, before_ts?, limit?)` | 分页读取历史（v3 向上无限滚动）：消息与传输按时间合并，默认 100 条/页、上限 500，返回 `{ items, hasMore }` |
| `get_autostart` / `set_autostart(enabled)` | 开机自启查询/设置（M4，F50） |
| `report_error(scope, message)` | 上报未知错误到本地日志（v3），返回 `{ dir, file, hasLogs }` |
| `get_error_log()` | 读取错误日志位置与「目录内是否已有日志」（v3，设置页据此显隐入口） |
| `open_error_log_dir()` | 打开错误日志目录（v3，桌面端；Android 返回错误，前端改为展示路径） |

事件：`device://update`、`message://new`、`transfer://update`、`session://update`（会话连接状态）。

补充说明（M2-b 实现口径）：

- `send_text(peer_id, text)`：后端在未建立会话时，用发现结果里的 **IP + 对端 TCP 端口** 自动建连；
  返回值 `{ msgId, status }`，`status ∈ delivered | failed`（3 秒未收到 TEXT_ACK 即 failed）。
- 重发：M2-b 未单独提供 `resend(msg_id)` 命令，前端对失败消息复用 `send_text`（消息内容仍在界面状态中）；
  M4 起历史已持久化，可在后续版本直接补 `resend(msg_id)` 以支持重启后重发。
- `message://new` 载荷：`{ peerId, msgId, text, ts }`；`session://update` 载荷：`{ kind: "connected" | "disconnected", peerId, name? }`。

补充说明（M3-a 实现口径）：

- `send_file(peer_id, path)`：先流式计算 sha256 再发 FILE_OFFER，返回 `{ transferId, state: "offered", ... }`；
  数据块与结束帧由后台任务发送，进度通过 `transfer://update` 推送，命令本身不阻塞 UI。
- `transfer://update` 载荷（字段 camelCase）：`{ transferId, peerId, direction: "send" | "receive", kind: "image" | "file", filename, size, transferred, state, path, error }`；
  `state ∈ pending | offered | transferring | done | failed | cancelled | rejected`，其中 `pending` 表示接收文件等待用户确认。
- 接收策略：图片（jpg/jpeg/png/gif/webp/bmp）自动接收；其他类型停在 `pending`，前端弹出接受/拒绝。
- 落盘：写入 `<下载目录>/<transfer_id>.part`，校验通过后改为 `<文件名>`；同名文件已存在时自动改用 `<文件名> (1).<扩展名>`，不覆盖用户文件。
- 图片预览：`get_image_preview` 返回 `data:image/*;base64,...`（上限 20MB），避免额外资源协议与权限配置；
  内存任务表缺失时（应用重启后）回退历史记录取落盘路径（v2/M9 修复），失败原因分为
  `image not received yet` / `preview file unavailable` / `image too large for preview`，前端据此提示。
  大图预览留待后续用 asset 协议实现。
- 平台能力：`get_local_device` 返回 `platform`（`android` / `windows` / `macos` / `linux`），
  前端据此隐藏平台不支持的操作（如 Android 无「打开所在文件夹」，v2/M9）。
- 清空语义（v3 修正）：`clear_history(peer_id=None, delete_files=true)` 除删除历史记录指向的文件外，
  还会**清空下载目录内所有单层普通文件**（不递归、跳过子目录与符号链接、canonical 前缀校验）；
  指定 `peer_id` 时保持原语义（只删该会话记录里的接收文件）。
- 传输任务 DTO 增加 `ts`（创建时间 Unix 毫秒，v3）：前端把图片/文件与文字消息按时间混排，
  历史记录同样使用该时间戳而非推送时间。
- 本地路径解析（v3，`transfer_local_path`）：接收方向取落盘路径；发送方向先取内存任务源路径，
  再回退历史 `source_path`；两者都没有时返回 `file unavailable`，前端按方向给出
  「源文件已被移动或删除」/「文件已被清理，如需重新获取请让对方重新发送」。
- 平台打开文件（v3）：Windows 改用 `ShellExecuteW`（不再 `cmd /C start`，无控制台闪窗）；
  macOS `open`、Linux `xdg-open`、Android 走 FileProvider + `ACTION_VIEW`。
- 设置项（v3）：`downloadDir` 在移动端只读（应用私有目录 `<app_data>/download/chatter`，
  点击给出 message 提示）；新增 `autoCleanEnabled` / `autoCleanDays`（默认 `false` / `7`，
  天数必须 > 0，启动时按 mtime 清理超期接收文件并移除对应传输记录）。
- 错误日志（v3）：`<app_data>/logs/YYYY-MM-DD/errors.log`，启动时清理超过 7 天的日期目录；
  设置页仅在目录内存在非空日志时展示入口。
- 未知错误兜底（v3）：`utils/errors.ts` 的 `friendlyError` 保留后端中文文案，其余统一
  「操作失败，请稍后重试」；原始信息通过 `report_error` 写入本地日志。

## 9. 风险与对策

| 风险 | 对策 |
|---|---|
| Windows 防火墙拦截 UDP/TCP 监听 | 首次启动弹窗引导放行；安装包内配置防火墙规则（NSIS 脚本） |
| 广播在某些路由器/AP 隔离下不可用 | F3 手动 IP 添加兜底 |
| 大文件写盘拖慢吞吐 | 1MB 块 + BufWriter 顺序写；后续可加 4MB 块 |
| 哈希计算占用 CPU | 发送端 sha256 在 OFFER 前一次性流式算好并缓存；接收端边收边算 |
| Windows WebView2 差异 | Tauri 自带 WebView2 引导安装 |

## 10. 开发里程碑细化

1. **M2-a**：工程脚手架（Tauri+Vue）、配置与发现模块，设备列表可见。**状态：已完成**（含命令/事件接入与 i18n 基础）。
2. **M2-b**：TCP 帧协议 + HELLO + 文字消息收发与 ACK。**状态：已完成**（含聊天界面、超时失败与重发）。
3. **M3-a**：FILE_OFFER/ACCEPT/CHUNK/DONE 主流程，图片自动接收与预览。**状态：已完成**（含文件确认、取消、sha256 校验）。
4. **M3-b**：文件确认接收、进度/取消、sha256 校验、简版续传。**状态：已完成**（含空闲超时、断线重试、速度/剩余时间；拖拽/粘贴与文件夹提示仍待做）。
5. **M4**：SQLite 历史、托盘/通知/自启、设置页、清空历史。**状态：已完成**（历史改用 JSON 文件，见 §6 偏差说明）。
6. **M5**：Windows 安装包（NSIS）、防火墙引导、使用说明；此后进入 macOS/Linux/Android 移植评估。**状态：已完成免安装版交付**（NSIS 安装包需联网构建，见 `08-milestone-m5-report.md`）。

M2 交付与测试状态见 `requirements/04-milestone-m2-report.md`。

### v2 里程碑（计划，详见 `13-v2-plan.md`）

| 里程碑 | 内容 | 人日 |
|---|---|---|
| M6 | 跨平台地基：平台能力层、文件名/路径规则、构建目标、CI matrix | 5 |
| M7 | macOS 客户端：dmg/app、托盘、通知、LaunchAgent、签名（可延后） | 4 |
| M8 | Linux 客户端：deb/AppImage、.desktop、AppIndicator 降级 | 4 |
| M10 | 自动重连、设备指纹、性能优化（`chunk2m` 2MB 块 + 缓冲写） | 6–9 |
| M11 | 完整块级续传（§4.7，0x17–0x19 + 位图恢复 + v1 降级） | 8–12 |
| M9 | Android：MulticastLock 广播、SAF 保存、前台服务、移动 UI | 18–25 |

顺序：**M6 → M7 ∥ M8 → M10 → M11 → M9**（M9 的 3 天 spike 第 1 周并行）。
版本：`v2.0.0-rc.1` = M6+M7+M8+M10+M11；`v2.1.0` = M9；GA 需安装包 + 签名（阶段性可未签名，见 `01` Q13）。

## 11. 跨平台适配（v2 / M6–M9）

### 11.1 平台能力层

```
platform/
├── mod.rs        // 统一导出：download_dir / cache_dir / config_dir / open_file / reveal_file
├── windows.rs    // %USERPROFILE%\Downloads\Chatters；cmd start / explorer /select
├── macos.rs      // ~/Downloads/Chatters；open / open -R
├── linux.rs      // XDG 下载目录回退 ~/Downloads/Chatters；xdg-open
└── android.rs    // 私有目录 + SAF 导出（M9）
```

- `safe_file_name`：**按平台判定非法字符**（Windows 禁 `: < > " | ? *` 与控制字符；Unix 仅禁 `/` 与 NUL）；
- 托盘/通知：提供 `tray_supported()` / `notification_supported()`，失败时静默降级并记录日志；
- 单实例：macOS/Linux 行为需实测（Linux 依赖 DBus/XDG）。

### 11.2 各平台差异清单

| 项 | Windows | macOS | Linux | Android |
|---|---|---|---|---|
| 打包 | NSIS | dmg + app | deb + AppImage | APK/AAB |
| 图标 | png + ico(256) | icns | png 多尺寸 | 自适应图标 |
| 托盘 | 原生 | 原生菜单 | AppIndicator（GNOME 需扩展） | 无（改通知） |
| 通知 | 系统 toast | 需授权 | libnotify/DBus | 13+ 需授权 |
| 自启 | 注册表 | LaunchAgent | ~/.config/autostart | 不适用 |
| 文件打开/定位 | cmd start / explorer | open / open -R | xdg-open | Intent + SAF |
| 防火墙 | netsh（安装钩子） | 系统弹窗授权 | ufw/firewalld（文档） | 不适用 |
| 接收落盘 | 下载目录直写 | 下载目录直写 | 下载目录直写 | 私有目录 → SAF 导出 |

### 11.3 签名与交付口径（Q13）

- 阶段性允许**未签名** EXE / dmg / APK；发布说明必须标注 SmartScreen、Gatekeeper、Android 未知来源提示；
- 证书到位后补签：Windows（Authenticode）、macOS（Developer ID + notarytool）、Android（keystore）。
