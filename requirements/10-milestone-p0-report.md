# Chatters 里程碑报告：P0 收尾（手动加 IP / 粘贴 / 拖拽 / 提示 / 单实例 / 版本与图标）

> 版本：v1.0　日期：2026-10-28　对应：`01-project-overview.md` v0.3、`02-technical-design.md`（本次同步修订）

## 1. 交付内容

| 需求 | 实现 |
|---|---|
| **F3** 手动输入 IP 添加设备 | 新命令 `add_device_by_ip(addr)`：`parse_manual_addr` 支持 `1.2.3.4` / `1.2.3.4:28101` / `[::1]:28101`；握手成功后写入设备列表并广播 `device://update`；设备页新增输入框 +「添加设备」 |
| **F21** 剪贴板图片粘贴发送 | 新命令 `save_clipboard_image(base64, extension)`：扩展名白名单 + base64 解码（上限 50MB）+ 写入 `<app cache>/clipboard/<uuid>.<ext>`；会话输入框 `paste` 事件提取图片后走 `send_file` |
| **F22** 拖拽发送 | 新命令 `send_paths(peer_id, paths)`：逐条判断文件/目录并发起传输；`Chat.vue` 订阅 `getCurrentWebview().onDragDropEvent`，离开会话自动取消订阅 |
| **F31** 文件夹提示 | `send_paths` 对目录返回 `is_dir`；前端汇总后提示"请将文件夹压缩为 zip 后发送（已忽略 N 个文件夹）" |
| **F4** 上下线提示 | 设备 store 新增 `notice`：仅**新**设备上线/任意设备下线时提示，渲染在设备页并 3 秒自动消失（心跳重复上报不刷屏） |
| **F51** 文件通知 | 接收侧 `pending`（待确认）与 `done`（接收完成）在窗口不在前台时弹系统通知；文案不含文件名/路径（R5.1） |
| 单实例 | `tauri-plugin-single-instance` 启用（最先注册），重复启动只聚焦已有窗口，避免端口冲突 |
| 版本与图标 | 版本统一 `1.0.0-rc.1`（package.json / Cargo.toml / tauri.conf.json）；图标生成器改为渐变圆角底 + 双气泡 + 对勾，ICO 增加 256 帧 |

## 2. 测试状态（2026-10-28 本地实测）

| 门禁 | 结果 |
|---|---|
| Rust lib 单测 | **56 passed**（新增 base64 解码 2、地址解析 2、通知文案 1） |
| discovery / session / transfer 集成 | 4 / **10** / 5 passed（新增目录拒绝 1） |
| 前端 vitest | **14 files / 88 passed**（新增设备添加与提示 7、拖拽 2、粘贴工具 4） |
| `cargo fmt --check` / `cargo clippy -D warnings` | 通过 |
| `vue-tsc --noEmit` / `prettier --check` | 通过 |

用例要点：手动地址三种写法与非法输入、base64 往返与非法字符、目录发送被拒、`send_paths` 汇总与目录计数、
新设备上线提示一次而心跳不再提示、下线提示带昵称、粘贴工具识别图片/忽略纯文本、jpg→jpeg 扩展名映射。

## 3. 取舍与限制

1. **F4 用页面内提示，不用系统通知**（避免设备频繁上下线刷屏）；系统通知只用于新消息与文件事件。
2. **F31 只覆盖拖拽**：系统文件选择框本身不能选目录，属平台限制。
3. **粘贴图片走 base64 IPC**：50MB 图片会产生约 67MB 字符串传输，属一次性开销；后续可改 asset 协议。
4. **拖拽依赖 WebView2 支持**：`onDragDropEvent` 初始化失败时静默降级（仍可用按钮选文件）。
5. **图标为程序化生成**：如需品牌稿，直接替换 `src-tauri/icons/*` 并保持文件名即可。

## 4. 打包与冒烟（1.0.0-rc.1）

- release 构建通过（13.3 MB），产物：`src-tauri/target/release/chatters.exe`；
- 便携包：`src-tauri/target/release/package/Chatters-1.0.0-rc.1-portable.zip`；
- 单实例验证：连续启动两次，第二次进程立即退出，系统中仅保留 1 个实例（端口不冲突）。

## 5. 后续（P1/P2）

- P1：F23 打开/另存为、F40 按会话清空、F41 缓存清理、最小化到托盘、多会话并行视图、大文件实测；
- P2：NSIS 安装包（联网）、代码签名、ESLint 补齐、E2E 与人工验收。
