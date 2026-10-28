# Chatters 里程碑报告：M3-b（续传 / 超时 / 重试 / 进度细化）

> 版本：v1.0　日期：2026-10-28　对应：`01-project-overview.md` v0.3、`02-technical-design.md`（本次同步修订）

## 1. 交付范围

| 项 | 实现 |
|---|---|
| 简版续传（Q8/F32） | 接收端在 `<下载目录>/.chatters-partial/` 保存 `<key>.part` + `<key>.json`（filename/size/sha256），`key = sha256(filename|size)` 前 32 位；命中则 `FILE_ACCEPT.resume_offset` 返回已收字节数，发送端 `seek` 续传；接收端 `append` 写入并重建 sha256 累加器 |
| 空闲超时 | 传输中心每秒巡检，`transferring` 且 30 秒无进展 → `failed`（"传输超时，可重试"），保留部分文件 |
| 断线联动 | 会话 `PeerDisconnected` → 该对端未完成任务立即 `failed`（"连接已断开，可重试"） |
| 重试 | 新增 `retry_transfer(transfer_id)`：以原源文件重新发起 OFFER；前端失败/取消的发送任务显示「重试」 |
| 速度/剩余时间（F33） | 前端按进度采样（≥300ms 间隔）计算速度与 ETA，单位格式化独立为 `src/utils/format.ts` |
| 清理策略 | 成功：删元数据、`.part` 改名；取消/拒绝/校验失败：删两份；超时/断线：保留以便续传 |

## 2. 测试状态（2026-10-28 本地实测）

| 门禁 | 结果 |
|---|---|
| Rust lib 单测 | 46 passed |
| discovery 集成 | 4 passed |
| session 集成 | 5 passed |
| **transfer 集成** | **9 passed**（新增续传、空闲超时、断线 3 例） |
| 前端 vitest | **9 files / 57 passed**（新增 format 3 例、retry 2 例） |
| `cargo fmt --check` / `cargo clippy -D warnings` | 通过 |
| `vue-tsc --noEmit` / `prettier --check` | 通过 |
| 启动冒烟（`RUST_LOG=info` 运行 exe） | 通过；期间修复一处启动期 panic：空闲巡检曾 `tokio::spawn` 于运行时之外，现改为在 `tauri::async_runtime::spawn` 内启动 |

新增用例要点：

1. 预置 1MB `.part` + 元数据后重传同一文件：接收端 `transferred` 起始即为 1MB，最终内容与哈希一致，元数据清理；
2. 空闲 200ms 超时：任务转 `failed` 且保留部分文件；
3. 对端断开：任务立即 `failed`（"连接已断开，可重试"）；
4. `formatBytes/formatSpeed/formatEta` 边界值；
5. 失败发送任务显示重试按钮、接收方向不显示重试。

## 3. 仍未实现（后续里程碑）

| 需求 | 说明 |
|---|---|
| F12 复制已收到消息 | 前端按钮 + 剪贴板 API，待做（M4 一并做） |
| F21 剪贴板粘贴图片 / F22 拖拽发送 / F31 文件夹提示 | 需要 Tauri drag-drop 事件 + 临时文件写入命令，待做 |
| F40-F42 历史 | 依赖存储模块（SQLite 依赖无法离线获取，将用 JSON 存储替代，需更新设计文档） |
| F50-F53 托盘/通知/自启/设置页 | M4 |
| M5 安装包 | 待验证离线环境下 NSIS 打包是否可行（否则先出 release exe） |

## 4. 下一步

1. **M4**：设置（昵称/下载目录/端口/主题/自动接收）、历史存储与清空、复制消息、托盘/通知/自启；
2. **M5**：`npm run tauri build`（NSIS）验证与使用说明；若离线缺少 NSIS 打包器，先交付 release exe + 手动安装说明。
