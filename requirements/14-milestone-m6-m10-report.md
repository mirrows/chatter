# M6 + M10 实施报告（v2 第一批）

> 版本：v2.0.0-dev　日期：2026-10-28　对应计划：`13-v2-plan.md` §4（M6）、§7（M10）
>
> 环境说明：本次开发仅在 Windows 10/11 本机、**无网络**、无 macOS/Linux/Android 设备与 SDK 的条件下进行。
> 因此按“可本机验证优先”的顺序先落地 M6 的代码部分与 M10 全部内容，M7/M8/M9 与 M11 见 §7。

## 1. 交付摘要

| 里程碑 | 内容 | 状态 |
|---|---|---|
| **M6** 跨平台地基 | `platform` 模块（路径 / 文件名规则 / 打开与定位文件）、按平台拆分的打包配置、三平台 CI 矩阵 | 代码完成，Windows 已验证；macOS/Linux 待实机 |
| **M10.1** 自动重连 | `HELLO` 增加 `tcp_port`（v2 可选字段），断线后 5/15/30s 退避重连 3 次，新增 `reconnecting` / `reconnectFailed` 事件与界面状态 | 完成并测试 |
| **M10.2** 设备指纹 | `device_id` 的 SHA-256 前 8 字节，按 `A1B2-C3D4-E5F6-7890` 展示；设备列表与本机信息均显示 | 完成并测试 |
| **M10.3** 性能 | `HELLO.features` 能力协商 + `chunk2m`（2MB 块）；未协商时回落 1MB；loopback 61.6–66.5 MiB/s（基线 36.6） | 完成并测试（吞吐见 §5） |
| **M11** 块级续传 | 未开始（`chunkmap` 能力**未声明**，避免对端发送当前无法处理的帧） | 待做 |
| **M7/M8/M9** | 仅落地平台化配置与 CI；构建需 macOS/Linux/Android 实机与 SDK | 阻塞于环境 |

## 2. M6 跨平台地基

### 2.1 新增模块

| 文件 | 职责 |
|---|---|
| `src-tauri/src/platform/mod.rs` | `file_name_allowed()`（跨平台文件名规则）、`tray_supported()`；附平台差异单元测试 |
| `src-tauri/src/platform/paths.rs` | `home_dir()`、`download_dir()`：统一解析 `USERPROFILE`/`HOME`，默认 `<home>/Downloads/Chatters` |
| `src-tauri/src/platform/shell.rs` | `open_command()` / `reveal_command()`：Windows `cmd start`/`explorer /select`、macOS `open`/`open -R`、Linux `xdg-open` |

### 2.2 改动点

- `config.rs`：`default_download_dir()` 改为调用 `platform::paths::download_dir()`，删除本地 `dirs_home()`。
- `commands.rs`：删除内联的 `open_command`/`reveal_command`，统一从 `platform::shell` 引入。
- `transfer/mod.rs`：`safe_file_name()` 改为复用 `platform::file_name_allowed()`：
  - 通用拒绝：空名、`.`、`..`、含 `/` 或 `\`、含 `..`、含控制字符；
  - **Windows 额外拒绝** `: < > " | ? *` 与结尾 `.`；
  - macOS/Linux 允许 `a:b.txt` 这类合法名字（v1 曾一律拒绝，属过度限制）。
- `lib.rs`：托盘创建增加 `platform::tray_supported()` 判断（Android 无托盘），并保留“图标缺失则跳过”的降级。

### 2.3 构建配置

| 文件 | 作用 |
|---|---|
| `src-tauri/tauri.windows.conf.json` | `bundle.targets = ["nsis"]` |
| `src-tauri/tauri.macos.conf.json` | `["dmg","app"]`，`minimumSystemVersion = 12.0`，图标改用 PNG（macOS 不用 `.ico`） |
| `src-tauri/tauri.linux.conf.json` | `["deb","appimage"]`，图标改用 PNG |
| `.github/workflows/ci.yml` | `frontend`（vitest + vue-tsc + prettier）+ `rust` 矩阵（windows / macos / ubuntu-22.04：fmt、clippy `-D warnings`、test、`tauri build --no-bundle`） |

> macOS 打包仍需 `npx tauri icon` 生成 `.icns`（联网环境或安装了 CLI 的机器上执行一次），已在报告 §7 记为遗留项。

## 3. M10.1 自动重连

### 3.1 协议与状态

- `Hello` 新增两个**可选**字段（`#[serde(default)]`）：`features: Vec<String>`、`tcp_port: Option<u16>`。
  v1 端不发送这两个字段，解析结果为空/`None`，协议 `ver` 保持 `1`（Q14：不让 v1 端因版本号拒连）。
- `SessionConfig` 增加 `features`（默认 `LOCAL_FEATURES = ["chunk2m"]`）与 `reconnect_delays`（默认 `[5,15,30]`，测试可缩短）。
- 新增事件：`SessionEvent::PeerReconnecting{device_id, attempt}`、`SessionEvent::PeerReconnectFailed{device_id}`；
  对应前端 `SessionUpdate` 的 `reconnecting` / `reconnectFailed`。

### 3.2 重连流程

1. 建连时记录 `reconnect_addr = (对端 IP, HELLO.tcp_port)`；**对端未声明 `tcp_port`（v1）时不设重连地址**。
2. 读任务结束 → 移除对端 → 发 `PeerDisconnected` → 若存在重连地址则启动后台任务。
3. 后台任务按 `reconnect_delays` 逐个等待；每次尝试前检查“是否已有新连接”（用户手动重连或对端主动连入）→ 有则直接退出。
4. 每次尝试前发 `PeerReconnecting`，成功则 `establish()` 重新登记（自然触发 `PeerConnected`），三次全失败发 `PeerReconnectFailed`。

### 3.3 界面

- `chat` store 新增 `reconnecting: Record<peerId, number>` 与 getter `reconnectAttempt()`；`connected` 事件到达时清除计数。
- 会话头部状态文案三态：已连接 / 正在重连…（第 n 次）/ 未连接（发送时自动连接）。

## 4. M10.2 设备指纹

- `commands::fingerprint(device_id)`：`SHA-256(device_id)` 取前 4 字节，按 `A3F2-91C0` 两组展示（与 `13-v2-plan.md` §7.2 一致）。
- `DeviceDto` / `LocalDeviceDto` 增加 `fingerprint` 字段（camelCase `fingerprint`），前端 `DeviceEntry` / `LocalDevice` 同步。
- 设备列表逐行显示“指纹 A3F2-91C0”，本机指纹显示在标题下方，便于双端人工核对（对应 `01` F5）。
- 说明：指纹用于**人工核对**，不是加密或认证；真正的身份认证仍需后续签名方案（`01` §3.5 保留项）。
- 未做可选子项“首次连接核对弹窗 + 信任列表”（`13` §7.2 标注为 +1 人日可选项），留待 M11 之后评估。

## 5. M10.3 性能

### 5.1 已实施

| 项 | 说明 |
|---|---|
| 块大小协商 | 双方都声明 `chunk2m` 时使用 **2MB** 块，否则 **1MB**（v1 兼容）；`protocol::MAX_CHUNK_DATA` 提升到 2MB，帧上限 4MB 仍有余量 |
| 块序号 | `chunk_seq_with(offset, chunk_size)`，按实际块大小计算；旧 `chunk_seq()` 删除，避免误用 |
| 接收端兼容 | 接收端不依赖块序号（顺序写入 + 流式 sha256），因此 1MB/2MB 混发不会破坏校验 |

### 5.2 明确未采用：`BufWriter` 顺序写

计划中“缓冲写”经评估后**放弃**，原因是正确性冲突：v1 续传以 `.part` 文件长度作为已收字节数，
若数据滞留在用户态缓冲区，重启后长度会偏小并向发送端索要重复偏移，造成重复写入。

替代做法：把“批量写”的收益放在**更大的块**上（每次 `write_all` 2MB，系统调用减少一半），
同时保持 `.part` 长度始终等于真实已落盘字节数，续传语义不变。

### 5.3 吞吐

| 场景 | 结果 | 说明 |
|---|---|---|
| release + 1MB 块（v1 基线，见 `12` §3） | 36.6 MiB/s | 256 MiB / 6.99s |
| release + 2MB 块（本次，两次测量） | **61.6 / 66.5 MiB/s** | 256 MiB，4.15s / 3.85s，含发送端整文件 sha256 预计算 |

结论：块大小 1MB→2MB 带来约 **+70%~80%** 的 loopback 吞吐提升，仍未达到 `13` §7.3 的 100 MiB/s 目标。

剩余差距主要来自：
1. 发送端每次 `tokio::fs::read` 仍是一次 blocking-pool 往返（计划中的“双缓冲预读”未做）；
2. 接收端 `write_all` 同样走 blocking-pool（计划中的“专用阻塞写线程”未做）；
3. 发送端在传输前对整文件做一次 sha256 预计算，基准把它算进了总耗时。

> 注：本机为单机 loopback 且无网络，无法验证“千兆 ≥50MB/s”；该目标仍需两台千兆机器实测（`11` 验收清单）。

## 6. 测试与门禁

| 门禁 | 结果 |
|---|---|
| Rust 单测 | **70 passed**（新增：平台文件名规则 4、路径 1、shell 2、HELLO v2 字段 2、指纹 2、重连事件序列化 2、块序号 2MB 1） |
| 集成测试 | discovery 4 / session 5 / **session_reconnect 2** / file_transfer 10（+1 ignored 基准） |
| 前端 vitest | **14 文件 / 103 passed**（新增：设备指纹渲染 2、重连状态 4） |
| `cargo fmt --check` / `clippy -D warnings` | 通过 |
| `vue-tsc --noEmit` / `prettier --check` | 通过 |

关键新增用例：

1. `platform::tests::platform_specific_rules`：Windows 拒绝 `a:b.txt`/`report.`，Unix 允许。
2. `protocol::tests::hello_without_v2_fields_parses`：v1 HELLO 缺字段仍可解析。
3. `session_reconnect::reconnects_after_peer_restart`：伪造 v2 对端断开 → 对端同端口重启 → 自动重连成功。
4. `session_reconnect::v1_peer_is_not_reconnected`：v1 对端（无 `tcp_port`）不触发重连。
5. `chat-store` / `chat-view`：重连中显示尝试次数、重连失败回到未连接。

## 7. 未完成项与环境阻塞

| 项 | 原因 | 后续动作 |
|---|---|---|
| M11 块级续传（`0x17/0x18/0x19`、`.bitmap`、随机写） | 单独立项（8–12 人日），本次未开工；`chunkmap` 能力**未声明**，对端不会发送无法处理的帧 | 下一批实现，完成后再把 `chunkmap` 加入 `LOCAL_FEATURES` |
| M7 macOS / M8 Linux 构建 | 无 macOS/Linux 机器、无跨平台工具链；本地无法交叉编译 Tauri（需目标系统 WebView 依赖） | 在有机器/CI runner 的环境执行 `npx tauri build`；macOS 需先 `npx tauri icon` 生成 `.icns` |
| M9 Android | 无 Android SDK/NDK 与真机 | 先做 spike（`13` §9.1） |
| 千兆吞吐、10GB 长稳、真实两机验收 | 无第二台设备/网络 | 按 `11-acceptance-checklist.md` 执行 |
| 代码签名/公证 | 无证书 | 保持未签名交付（Q13） |

## 8. 兼容性与回归风险

- **v1 ↔ v2**：`ver` 仍为 1；v1 端忽略未知 JSON 字段，v2 端对缺失字段取默认值。
- **块大小**：仅当对端声明 `chunk2m` 才发 2MB；因此 v1 端永远不会收到超过 1MB 的数据块。
- **重连**：仅对声明 `tcp_port` 的对端生效；v1 对端保持“断开即断开”的旧行为。
- **文件名**：Windows 行为比 v1 更严格（新增拒绝 `: < > " | ? *` 与结尾 `.`），属安全加固；
  macOS/Linux 行为比 v1 更宽松（允许 `:`），需在 M7/M8 实机回归一次含冒号的文件名。
