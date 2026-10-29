# Chatters — 局域网点对点传输工具（文字 / 图片 / 文件）

> 无需服务器、免登录的局域网点对点传输工具，支持文字、图片与文件传输。
> 技术栈：Tauri 2.x + Vue 3 + Rust 后端。当前版本：**1.0.0-rc.1**（M2–M4 + P0/P1/P2 收尾完成，Windows 免安装版可交付）。

## 目录结构

- `requirements/` — 需求与设计的唯一基准（文件名英文，正文中文）
  - `01-project-overview.md` — 需求文档（v0.5，含 Q1–Q15、未完成项 §3.5、v2 路线图 §7）
  - `02-technical-design.md` — 架构、协议、模块划分
  - `03-doc-index.md` — 文档索引与状态
  - `04-milestone-m2-report.md` — M2 里程碑报告（交付内容与测试状态）
  - `05-milestone-m3a-report.md` — M3-a 里程碑报告（图片/文件传输）
  - `06-milestone-m3b-report.md` — M3-b 里程碑报告（续传/超时/重试）
  - `07-milestone-m4-report.md` — M4 里程碑报告（设置/历史/托盘/自启）
  - `08-milestone-m5-report.md` — M5 里程碑报告（打包与交付）
  - `09-user-guide.md` — 使用说明（安装、防火墙、收发、FAQ）
  - `10-milestone-p0-report.md` — P0 收尾报告（手动加 IP / 粘贴 / 拖拽 / 提示 / 单实例 / 图标）
  - `11-acceptance-checklist.md` — v1.0.0-rc.1 验收清单
  - `12-milestone-p1-p2-report.md` — P1/P2 报告（文件操作、历史清理、托盘、发布工程）
  - `13-v2-plan.md` — v2 迭代计划（多平台 / 网络与性能 / 完整块级续传）
  - `14-milestone-m6-m10-report.md` — M6 + M10 实施报告（平台层、自动重连、设备指纹、2MB 块）
- `src-tauri/src/` — Rust 后端：`config` / `discovery` / `platform` / `protocol` / `session` / `transfer` / `commands`
- `src/` — Vue 3 前端：`views`（设备列表、会话）、`components`、`stores`、`api`、`i18n`
- `tests/` — 前端统一测试目录（布局见 rules.md R2.2）
- `src-tauri/tests/` — Rust 集成测试（`tests/<module>/<feature>.rs`，需在 `Cargo.toml` 登记）
- `rules.md` — 对 AI 代码生成助手的强制工程约束

## 当前状态

- 需求：已确认（Q1–Q8）— 见 `requirements/01-project-overview.md` 第 4 节
- 技术栈：Tauri 2.x + Vue 3 + Rust（已确认）
- 范围排除项：不做消息撤回、不支持传输记录导出；仅保留传输终止（取消）
- 已完成：M2-a 设备发现 + 设备列表（`list_devices` / `device://update`）；
  M2-b TCP 帧协议 + HELLO + 文字消息与 ACK（`send_text` / `message://new` / `session://update`）；
  M3-a 图片/文件传输主流程（`send_file` / `accept_file` / `cancel_transfer` / `transfer://update`，图片自动接收与预览、sha256 校验）；
  M3-b 简版续传、空闲超时与断线重试（`retry_transfer`）、速度与剩余时间；
  M4 设置页（昵称/目录/端口/主题/自动接收/自启）、历史持久化与清空、复制消息、托盘与系统通知；
  P0 收尾：手动按 IP 添加设备（F3）、剪贴板粘贴图片（F21）、拖拽发送与文件夹提示（F22/F31）、上下线提示（F4）、文件通知（F51）、单实例；
  P1/P2：已接收文件打开/定位/另存为（F23）、按会话清空历史（F40）、可选删除已接收文件（F41）、关闭窗口最小化到托盘（F51）、未读徽标（F42）、20MB 预览、NSIS 防火墙钩子与验收清单；
  v2 第一批（M6 代码部分 + M10）：`platform` 平台能力层与按平台打包配置、三平台 CI 矩阵、
  断线自动重连（5/15/30s，事件与界面状态）、设备指纹（`A3F2-91C0`）、`chunk2m` 协商的 2MB 块（loopback 36.6 → 61.6/66.5 MiB/s）；
  v2 Android spike（M9）：工程门控（桌面插件 `cfg` 隔离 + `mobile_entry_point`）、`gen/android` 工程、
  模拟器 API 36 上构建/安装/启动并用手动 IP 打通文字双向收发（详见 `requirements/15-m9-spike-plan.md`）
- 下一步：v1 实机验收（`requirements/11-acceptance-checklist.md`）与联网打包/签名；
  v2 剩余：M11 完整块级续传 → M7/M8 实机构建 → M9 原生能力（MulticastLock / 前台服务 / SAF）+ 真机验收

## 数据与文件位置

| 内容 | 位置 |
|---|---|
| 配置（昵称/端口/下载目录/主题/自动接收） | `%APPDATA%\com.chatters.app\config.json` |
| 历史（消息 + 传输记录） | `%APPDATA%\com.chatters.app\history.json` |
| 接收文件 | `%USERPROFILE%\Downloads\Chatters`（可在设置页修改） |
| 续传临时文件 | `<下载目录>\.chatters-partial\` |

## 打包

需要联网下载 NSIS 打包器；离线环境可只构建可执行文件：

```powershell
# 完整安装包（NSIS，需联网）
npm run tauri build

# 仅可执行文件（离线可用）
npm run tauri build -- --no-bundle
# 产物：src-tauri/target/release/chatters.exe
```

当前离线环境已产出免安装版：`src-tauri/target/release/package/Chatters-1.0.0-rc.1-portable.zip`
（含 `Chatters.exe` 与使用说明）。NSIS 打包失败原因为打包器下载超时，联网后重跑即可。

> 首次运行请在 Windows 防火墙弹窗中允许「专用网络」，否则 UDP 28100 / TCP 28101 会被拦截。

## 开发与验证

要求：Node **≥ 20**（本机若 PATH 中存在旧版 Node，请先把新版加入 PATH）、Rust stable、Windows 10/11 x64。

```powershell
npm install
npm run tauri dev          # 启动桌面应用
```

```powershell
# 前端门禁
npm run test               # Vitest（tests/**/*.spec.ts）
npx vue-tsc --noEmit
npx prettier --check "src/**/*.{ts,vue}" "tests/**/*.ts"

# 后端门禁
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

> 依赖安装需联网；日常构建与测试均可离线执行（`cargo test --offline`）。
