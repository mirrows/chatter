# Chatters 里程碑报告：P1（体验一致性）+ P2（发布工程）

> 版本：v1.0　日期：2026-10-28　对应：`01-project-overview.md` v0.3、`11-acceptance-checklist.md`

## 1. P1 交付

| 需求 | 实现 |
|---|---|
| **F23** 打开 / 定位 / 另存为 | 新增命令 `open_transfer_file`、`reveal_transfer_file`、`save_transfer_as`（Windows 用 `cmd start` / `explorer /select`，macOS 用 `open`，Linux 用 `xdg-open`）；已完成接收卡片显示三个按钮 |
| **F40** 按会话清空历史 | 会话页「清空本会话记录」调用 `clear_history(peer_id)` 并清空本地消息/任务；设置页仍可清空全部 |
| **F41** 可选删除缓存 | `clear_history(peer_id?, delete_files?)`：只删除**下载目录内**、方向为接收且历史中记录的文件；`delete_files_under` 校验 canonical 路径前缀，目录外文件绝不删除；设置页新增勾选项并显示删除数量 |
| **F51** 最小化到托盘 | 监听 `WindowEvent::CloseRequested` → `prevent_close` + `window.hide()`；托盘菜单「退出」才真正退出；重复启动聚焦已有窗口（单实例） |
| **F42** 多会话并行感知 | 聊天 store 增加 `unread` 计数与 `markRead`，设备列表显示未读徽标；每对端独立会话/任务（后端本就并发） |
| **F24** 大图预览 | 预览上限 8MB → **20MB**；更大图片仍可打开/另存为（F23），不受预览限制 |
| **F33/F35** 大文件验证 | 新增 `throughput_256mb` 基准（`#[ignore]`），实测数据见 §3 |

## 2. P2 交付

| 项 | 结果 |
|---|---|
| NSIS 防火墙规则 | 新增 `src-tauri/nsis/installer-hooks.nsh`：安装时 `netsh advfirewall` 放行 UDP 28100 / TCP 28101（仅 domain/private），卸载时删除；`tauri.conf.json` 绑定 `bundle.windows.nsis.installerHooks`。**离线无法实际打包验证**，脚本按 NSIS 官方 hook 宏编写 |
| 验收清单 | 新增 `requirements/11-acceptance-checklist.md`（自动化门禁 + F1–F53 逐项 + 发布检查 + 已知不做） |
| NSIS 安装包 | 仍需联网下载打包器；本次交付便携 zip |
| 代码签名 | 未做（需要证书与联网），发布说明需提示 SmartScreen |
| ESLint | 仍缺 `eslint-plugin-vue` / `typescript-eslint`（离线无法安装），`npm run lint` 不可用；其余门禁（prettier、vue-tsc、vitest）已覆盖 |
| E2E | 未引入浏览器自动化（依赖缺失），改用集成测试 + 人工验收清单 |

## 3. 大文件基准（256 MiB，本机 loopback）

| 构建 | 结果 | 说明 |
|---|---|---|
| debug（`cargo test`） | 4.5 MiB/s | sha256 未优化，仅作功能验证 |
| release（`cargo test --release --test file_transfer throughput_256mb -- --ignored --nocapture`） | **36.6 MiB/s**（256 MiB / 6.99s） | 与发布包同优化等级；包含发送端整文件 sha256 预计算、接收端边收边校验、`.part` 落盘与改名 |

分析：

- 该数字包含 **两次 256MB 的 sha256**（发送前预计算 + 接收端校验）以及 Windows Defender 对 1MB 写入的实时扫描，因此低于纯网络吞吐；
- 千兆网 50MB/s 目标需要在**真实两机 + 无杀软干扰**环境下复核（验收清单 F35）；
- 后续优化方向（v2）：块大小 1MB → 2/4MB、发送/接收改为带缓冲的专用阻塞写线程、提供"关闭实时扫描"安装提示。

## 4. 测试状态

| 门禁 | 结果 |
|---|---|
| Rust 单测 | 59 passed（新增 3：平台打开命令、下载目录内删除、缺失路径） |
| 集成测试 | discovery 4 / transfer 10（+1 ignored 基准）/ session 5 |
| 前端 vitest | 14 文件 / 98 passed（新增 F23、F42、F40、F41、F51 相关 10 例） |
| fmt / clippy / vue-tsc / prettier | 通过 |

## 5. 剩余缺口

1. NSIS 安装包与代码签名需联网/证书；
2. ESLint 与 E2E 依赖缺失；
3. 千兆网真实吞吐、10GB 级文件、开机自启注册表需实机验证（验收清单 C 节）；
4. 跨平台客户端仍不在 v1 范围（见 `01` §3.3 / Q1）。
