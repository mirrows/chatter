# Chatters 里程碑报告：M5（打包与交付）

> 版本：v1.1　日期：2026-10-28　对应：`01-project-overview.md` v0.3、`09-user-guide.md`

## 1. 打包结果（当前版本 1.0.0-rc.1）

| 目标 | 结果 |
|---|---|
| release 可执行文件 | ✅ `src-tauri/target/release/chatters.exe`（13.3 MB，LTO 开启） |
| 免安装便携包 | ✅ `src-tauri/target/release/package/Chatters-1.0.0-rc.1-portable.zip`（4.2 MB，含 exe + 使用说明） |
| NSIS 安装包 | ❌ 当前环境不可用：`Downloading nsis-3.13.zip` → `failed to bundle project: timeout: global`（离线无法下载打包器） |
| 启动冒烟（release） | ✅ 运行无异常；单实例验证：重复启动时第二个进程立即退出、仅保留 1 个实例 |

> 联网后执行 `npm run tauri build` 即可产出 `Chatters_1.0.0-rc.1_x64-setup.exe`；
> `tauri.conf.json` 已配置 `targets: ["nsis"]` 与图标（32/128/512 PNG + 含 256 帧的 ico）。

## 2. 交付物清单

```
src-tauri/target/release/
├── chatters.exe                       # 可直接运行的程序（1.0.0-rc.1）
└── package/
    ├── Chatters.exe                   # 重命名后的程序
    ├── README-使用说明.md              # 用户指南（安装/防火墙/收发/设置/FAQ）
    └── Chatters-1.0.0-rc.1-portable.zip  # 分发包
```

用户指南见 `requirements/09-user-guide.md`（安装、防火墙放行、设备发现、文字/图片/文件收发、
续传与重试、设置、历史、托盘通知、数据位置、常见问题）。

## 3. 打包前后的质量门禁（实测）

| 项目 | 结果 |
|---|---|
| Rust 单测 | 51 passed |
| discovery / session / transfer 集成 | 4 / 5 / 9 passed |
| 前端 vitest | 12 files / 75 passed |
| `cargo fmt --check` | 通过 |
| `cargo clippy --all-targets -D warnings` | 通过 |
| `vue-tsc --noEmit` | 通过 |
| `prettier --check` | 通过 |
| `vite build` | 通过（约 88 kB JS） |
| release 构建 + 启动冒烟 | 通过 |

## 4. 已知限制（发布说明需体现）

1. **无正式安装包**：离线环境无法下载 NSIS；当前以便携版交付（解压即用）。
2. **端口/昵称改动需重启**（设置页有提示）。
3. **未实现**：手动 IP 添加（F3）、剪贴板粘贴图片（F21）、拖拽发送（F22）、文件夹压缩提示（F31）、
   托盘最小化（关闭即退出）、历史中图片缓存随历史删除（F41 部分）。
4. **图标**：当前使用项目内置图标（`tools/gen-icons` 生成），正式发布前建议替换为设计稿。
5. **代码签名**：未签名，Windows SmartScreen 会提示"未知发布者"，需在发布说明中提示用户选择"仍要运行"。

## 5. 后续建议

1. 联网环境产出 NSIS 安装包，配置防火墙规则（NSIS 脚本放行 UDP 28100 / TCP 28101）；
2. 补 M3 遗留 UX：拖拽/粘贴发送、文件夹提示、手动 IP 添加；
3. 引入 SQLite 替换 JSON 历史（依赖可获取后），补 `resend(msg_id)` 与图片缓存清理；
4. macOS/Linux/Android 移植评估（Rust 网络层可直接复用）。
