# Chatters 里程碑报告：M2（设备发现 + 文字消息）

> 版本：v1.0　日期：2026-10-28　对应：`01-project-overview.md` v0.3、`02-technical-design.md`（本次同步修订）

## 1. 交付范围

### M2-a 工程脚手架与设备发现（F1 / F2 / F4）

| 层 | 交付内容 |
|---|---|
| Rust | `config.rs` 配置读写（首装生成并持久化 `device_id`）；`discovery.rs` UDP 广播/监听、设备表心跳与离线清理；`commands.rs` 的 `list_devices` / `get_local_device`，`device://update` 事件桥接；`lib.rs` 启动装配 |
| Rust | 修正脚手架遗留问题：广播端口跟随配置（原硬编码 28100）、移除死代码 `bye`、网络路径去掉 `unwrap()` |
| 配置 | 新增 `capabilities/default.json`（`core:default` 等），否则 Tauri 2 下前端 `listen` 会被 ACL 拒绝 |
| 前端 | `i18n` 基础模块（R3.3）、`onDeviceUpdate` 订阅、store 初始化（快照 + 事件）、设备列表显示本机昵称与错误提示 |

### M2-b TCP 帧协议 + HELLO + 文字消息（F10 / F11）

| 层 | 交付内容 |
|---|---|
| Rust | 新增 `protocol.rs`：帧编解码（magic 5B + type 1B + 大端长度 4B + payload）、4MB 上限拒绝、`Hello` / `TextMessage` / `TextAck` 载荷 |
| Rust | 新增 `session.rs`：TCP 监听、双向 HELLO 握手、连接表、TEXT/TEXT_ACK、3 秒 ACK 超时、30 秒单向心跳、连接断开清理 |
| Rust | `commands.rs` 新增 `send_text`（未连接时按发现结果自动建连）、`peer_tcp_addr`、`message://new` 与 `session://update` 事件 |
| 前端 | `api/types.ts` + `api/tauri.ts` 新增 `sendText` / `onMessage` / `onSessionUpdate`；`stores/chat.ts` 乐观发送与状态流转；`views/Chat.vue` 会话界面（气泡、状态、失败重发、Ctrl+Enter）；`App.vue` 设备列表 ↔ 会话切换 |

## 2. 测试状态

全部通过（2026-10-28 本地实测）：

| 门禁 | 命令 | 结果 |
|---|---|---|
| Rust 单测 | `cargo test`（lib） | 32 passed |
| 发现集成测试 | `cargo test`（tests/discovery） | 4 passed |
| 会话集成测试 | `cargo test`（tests/session） | 5 passed |
| 前端单测 | `vitest run --config vitest.config.ts` | 6 files / 36 passed |
| 类型检查 | `vue-tsc --noEmit` | 通过 |
| 格式 | `cargo fmt --all -- --check`、`prettier --check` | 通过 |
| Lint | `cargo clippy --all-targets -- -D warnings` | 通过 |
| ESLint | `eslint .` | **未执行**：离线环境缺少 `eslint-plugin-vue` / `typescript-eslint`，无法安装（见 §4） |

覆盖口径（R2.4）：每个功能均包含正常路径、边界与异常用例，例如
超长帧拒绝、半包 IO 错误、坏 magic、ACK 超时标记失败、非法握手拒绝、未建连接发送报错、
空消息不发、设备下线事件 camelCase 契约等。

## 3. 与文档的偏差及处理（R6.2）

| 项 | 原文档 | 实现 | 处理 |
|---|---|---|---|
| 帧头 magic 长度 | 标注 `4B`，但 `"CHTRS"` 实为 5 字节 | 采用 5 字节 magic，帧头 10B | 已修订 `02-technical-design.md` §4.1 |
| 心跳语义 | `PING / PONG` 同类型，双向 | 单向通知，收到不回应（否则无限回环） | 已修订 §4.2 并补充说明 |
| 事件清单 | 未列会话状态事件 | 新增 `session://update` | 已补入 §8 |
| `resend(msg_id)` | 命令清单中有 | M2-b 未实现，前端复用 `send_text` | 已在 §8 标注，M4 随历史存储补齐 |
| 多网卡广播 | 每个非环回 IPv4 分别广播 | 仅向 `255.255.255.255:<配置端口>` 广播 | 已在 §3 标注为已知限制 |

## 4. 已知限制与环境注意事项

1. **多网卡广播**：未按网卡分别广播（离线环境无法引入网卡枚举依赖）。单网卡/常见家庭路由场景可用；多网卡机器可先用 F3 手动添加兜底。
2. **历史未持久化**：消息仅存在于前端内存，重启即清空，属 M4 范围（F40/F41）。
3. **无传输加密**：符合 Q7（v1 不加密）。
4. **ESLint 门禁缺口**：`eslint-plugin-vue`、`typescript-eslint` 不在 `node_modules`，且环境禁止联网安装，故 R3.1 的 eslint 环节暂缺，需在可联网环境执行 `npm i -D eslint-plugin-vue typescript-eslint` 后补齐配置。
5. **Node 版本**：本机 PATH 首位 `D:\nodejs\node.exe` 为 Node 12，无法运行 Vitest；须使用 Node ≥ 20（本机 `C:\Program Files\nodejs` 为 v24）执行前端命令，README 已注明。

## 5. 下一里程碑（M3-a：图片与文件主流程）

1. `protocol.rs` 扩展 `0x10 FILE_OFFER` / `0x11 FILE_ACCEPT` / `0x12 FILE_REJECT` / `0x14 FILE_CHUNK` / `0x15 FILE_DONE` / `0x16 FILE_RESULT`；
2. 新增 `transfer/` 模块：任务状态机（`pending → offered → transferring → done/failed/cancelled`）、块读写（1MB）、`.part` 临时文件、接收目录防穿越（R5.2）；
3. 图片按 `kind=image` 自动接收（F34），前端缩略图预览（F23）；
4. 进度事件 `transfer://update` 节流 ≥100ms（§5、3.1）；
5. 测试：OFFER/ACCEPT 拒绝路径、超长块拒绝、路径穿越拒绝、进度事件与取消。

## 6. 建议提交拆分（R4.2）

```
feat(commands): expose device list commands and device://update event
feat(i18n): add minimal i18n module and localize device list
feat(protocol): add CHTRS frame codec with size limits
feat(session): add TCP session hub with HELLO, text and ACK
feat(chat-ui): add chat store and view with send status and retry
test(discovery): add UDP discovery integration tests
test(session): add text messaging integration tests
docs: update technical design and add M2 milestone report
chore(tooling): add prettier config, tauri capability, prettier/tsconfig normalization
```
