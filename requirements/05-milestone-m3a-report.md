# Chatters 里程碑报告：M3-a（图片/文件传输主流程）

> 版本：v1.0　日期：2026-10-28　对应：`01-project-overview.md` v0.3、`02-technical-design.md`（本次同步修订）

## 1. 交付范围

| 层 | 交付内容 |
|---|---|
| 协议 | `protocol.rs` 扩展 `FILE_OFFER(0x10)` / `FILE_ACCEPT(0x11)` / `FILE_REJECT(0x12)` / `FILE_CANCEL(0x13)` / `FILE_CHUNK(0x14)` / `FILE_DONE(0x15)` / `FILE_RESULT(0x16)`；新增 `FileKind` / 各载荷结构；块编码 `transfer_id(16B) + seq(4B) + data(≤1MB)` |
| 传输 | 新增 `transfer/mod.rs`（任务表、状态机、帧路由、候选文件名处理）、`transfer/sender.rs`（等 ACCEPT → 分块 → DONE → 等 RESULT）、`transfer/receiver.rs`（OFFER 建任务、`.part` 写入、sha256 校验、改名、回 RESULT） |
| 命令 | `send_file` / `accept_file` / `reject_file` / `cancel_transfer` / `list_transfers` / `get_image_preview` / `pick_file`；事件 `transfer://update` |
| 接线 | `session.rs` 将 FILE_* 帧以 `SessionEvent::TransferFrame` 交给传输层；`lib.rs` 完成会话 → 传输路由与传输事件 → 前端事件桥接；`capabilities` 复用既有配置 |
| 前端 | `stores/transfers.ts`（任务合并、选择并发送、确认/拒绝/取消、预览缓存）、`components/TransferCard.vue`（进度、状态、动作、图片预览）、`Chat.vue` 增加"发送图片/发送文件"与任务卡片、`App.vue` 启动时订阅传输事件 |
| 交互 | 图片自动接收并预览；普通文件待用户接受/拒绝；传输中可取消；失败显示原因 |

## 2. 测试状态（2026-10-28 本地实测）

| 门禁 | 命令 | 结果 |
|---|---|---|
| Rust 单测 | `cargo test`（lib） | 46 passed |
| 发现集成 | `cargo test`（tests/discovery） | 4 passed |
| 会话集成 | `cargo test`（tests/session） | 5 passed |
| 传输集成 | `cargo test`（tests/transfer） | 6 passed |
| 前端单测 | `vitest run` | 8 files / 52 passed |
| 类型检查 | `vue-tsc --noEmit` | 通过 |
| 格式 | `cargo fmt --check`、`prettier --check` | 通过 |
| Lint | `cargo clippy --all-targets -- -D warnings` | 通过 |
| ESLint | `eslint .` | 仍未执行（离线环境缺少插件，见 04 报告 §4） |

传输集成用例覆盖（R2.4）：

1. 3MB 图片自动接收：sha256 通过、落盘内容逐字节一致、无 `.part` 残留；
2. 普通文件停在 `pending`，确认后才完成；
3. 拒绝接收后双方状态均为 `rejected`；
4. 路径穿越文件名 `../evil.txt` 被拒绝且不落盘（R5.2）；
5. 声明哈希与实际数据不一致 → `failed`（"哈希校验失败"）且无残留文件；
6. 待确认阶段取消 → 双方 `cancelled`。

## 3. 与文档的偏差及处理（R6.2）

| 项 | 文档原描述 | 实现 | 处理 |
|---|---|---|---|
| 图片预览 | 建议缓存目录 + 缩略图 | `get_image_preview` 直接读取已接收文件并返回 data URL（≤8MB） | 已写入 §8 补充说明；大图预览留待 asset 协议方案 |
| 同名文件 | 未说明 | 自动追加 ` (1)` 序号，不覆盖 | 已写入 §8 |
| `pick_file` | 文档未列 | 新增命令（Rust 侧 dialog 插件，前端无需新增依赖） | 已补入 §8 命令表 |
| 进度事件 | `transfer://update` | 已实现，字段与状态取值见 §8 | 已补字段说明 |

## 4. 已知限制与后续（M3-b）

1. **简版续传未接**：`FILE_ACCEPT.resume_offset` 恒为 0，收到 ACCEPT 后不 seek；M3-b 接入偏移记录与块级校验。
2. **无接收超时**：接收端在 `transferring` 后若对端消失，任务会停留在该状态；M3-b 增加空闲超时与清理。
3. **速度/剩余时间未展示**：F33 要求，M3-b 补齐（需要采样窗口）。
4. **拖拽/粘贴发送未接**（F21/F22）：当前通过系统文件选择框发送；拖拽需 Tauri drag-drop 事件与前端路径处理。
5. **图片预览上限 8MB**：超出时界面提示"图片预览不可用"，不影响文件落盘。
6. **尚无历史持久化**：任务与消息均在内存，M4 用 SQLite 落库。

## 5. 下一里程碑（M3-b）

1. 简版续传：接收端保存 `.part` 偏移 + 文件名/大小匹配，重连后 `resume_offset` 生效，发送端 `seek` 续传；
2. 速度与剩余时间（滑窗采样，节流 100ms 推送）；
3. 空闲超时与孤儿任务清理（含断开事件联动）；
4. 前端"重试"入口（复用 `retry_transfer` 语义）；
5. 拖拽/粘贴发送（F21/F22）与文件夹提示（F31）。

## 6. 建议提交拆分（R4.2）

```
feat(protocol): add file transfer frames and chunk codec
feat(transfer): add transfer manager with sender and receiver
feat(commands): expose file transfer commands and transfer events
feat(transfer-ui): add transfer store, card and chat attachments
test(transfer): add file transfer integration tests
docs: update design and add M3-a milestone report
```
