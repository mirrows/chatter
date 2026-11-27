# Chatters — 局域网点对点传输工具（文字 / 图片 / 文件）

> 无需服务器、免登录的局域网点对点传输工具，支持文字、图片与文件传输。
> 技术栈：Tauri 2.x + Vue 3 + Rust 后端。当前版本：**1.0.0-rc.1**
> （桌面 Windows / macOS 与移动 Android arm64 均已接入出包流程；macOS 为未签名 universal 包，
> v3 UI 重构完成，后续项见 `requirements/17-v3-followups.md`）。

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
  - `15-m9-spike-plan.md` — Android（M9）实施与真机验收记录、APK 产物哈希
  - `16-v3-plan.md` — v3 UI 重构计划与进度（I0–I6.9、真机验收、产物哈希）
  - `17-v3-followups.md` — **后续迭代与优化项（唯一待办入口）**
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
  模拟器 API 36 上构建/安装/启动并用手动 IP 打通文字双向收发（详见 `requirements/15-m9-spike-plan.md`）；
  v3 UI 重构（I0–I6.9）：时间线混排 + 120px 气泡、右键/长按菜单、图片预览（PC 滚轮缩放 + 拖拽、移动端双指/单指）、
  移动端卡片与安全区适配、无限滚动与新消息分割线、通用浮层（菜单/预览/确认/输入/提示条）、
  文件打开语义（发送方打开源文件、找不到按方向提示）、错误日志（按天归档 7 天 + 设置页入口）、
  移动端定时清理、清空历史即时生效
- 下一步：见 `requirements/17-v3-followups.md`
  （P1：正式签名 + 双 ABI、NSIS/签名、设备上下线提示改 message、M11 块级续传；
  P2：Linux 打包、E2E、大图 asset 协议；macOS 打包已于 2026-11-21 接入）

## 数据与文件位置

| 内容                                              | 位置                                                                                                                      |
| ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| 配置（昵称/端口/下载目录/主题/自动接收/定时清理） | `%APPDATA%\com.chatters.app\config.json`（Android：`<app_data>/config.json`）                                             |
| 历史（消息 + 传输记录）                           | `%APPDATA%\com.chatters.app\history.json`（Android 同上目录）                                                             |
| 接收文件（桌面）                                  | `%USERPROFILE%\Downloads\Chatters`（可在设置页修改）                                                                      |
| 接收文件（Android）                               | `<app_data>/download/chatter`（**应用私有目录**，文件管理器不可见；用「另存为」导出到公共目录）                           |
| 续传临时文件                                      | `<接收目录>\.chatters-partial\`                                                                                           |
| 错误日志（v3）                                    | `<app_data>/logs/YYYY-MM-DD/errors.log`，只保留最近 7 天；设置页在目录有日志时显示入口（桌面打开文件夹 / 移动端显示路径） |

> 移动端可在「设置 → 定时清理」按天自动清理超期接收文件（默认关闭，保留天数默认 7 天、可改）。
> 清理会删除文件**及其对应的图片/文件气泡记录**（文字记录不受影响）；重要文件请先用「另存为」备份。

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

> **自动更新产物**：`bundle.createUpdaterArtifacts` 已开启，本地 `npm run tauri build` 会额外尝试生成
> `.sig` 更新签名，因此需要先设置签名密钥（否则构建失败）：
>
> ```powershell
> $env:TAURI_SIGNING_PRIVATE_KEY = (Get-Content -Raw D:\secure\chatters-updater.key).Trim()
> $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Get-Content -Raw D:\secure\updater-key-password.txt
> ```
>
> 只想要安装包、不生成更新产物时，加 `--config src-tauri/tauri.ci.conf.json`。

### macOS（dmg + app，可 universal）

```bash
# 仅本机架构（Apple Silicon → arm64；Intel → x64）
bash tools/build-macos-dmg.sh

# 通用包（Apple Silicon + Intel；首次需 rustup target add x86_64-apple-darwin）
bash tools/build-macos-dmg.sh --target universal-apple-darwin
```

产物在 `src-tauri/target/macos/`：

- `Chatters-<version>-macos-<arm64|universal>.dmg`
- `Chatters-<version>-macos-<arm64|universal>.app.zip`
- `SHA256SUMS.txt`

> Tauri 自带的 dmg 打包依赖 Finder AppleScript，headless 环境容易报
> `AppleEvent 超时 (-1712)`；脚本改用 `hdiutil` 生成 DMG（附 `/Applications` 快捷方式），
> CI 与本地都不依赖 Finder。
> 未签名包首次打开需右键 →「打开」；macOS 15+ 若被拦截，到
> 「系统设置 → 隐私与安全性」点「仍要打开」，或执行
> `xattr -dr com.apple.quarantine /Applications/Chatters.app`；
> 签名/公证需配置 `APPLE_SIGNING_IDENTITY` 等 Secrets（见 `requirements/17-v3-followups.md` §1.3）。

> GitHub Actions：
> - **CI** 工作流手动触发（`Run workflow`）会构建 macOS universal 包并上传 artifact
>   `chatters-macos-universal`（不创建 Release）；
> - **Release** 工作流（tag `v*` 或手动触发）会构建 macOS / Windows / Android 并发布到 Releases。

> release 双击启动**不会**弹控制台窗口。需要现场日志时设置环境变量 `CHATTERS_LOG=1`，
> 日志追加写入 `%APPDATA%\com.chatters.app\chatters.log`。Windows 上已禁用 WebView 的右键菜单。

### Android（arm64，debug 签名，可 sideload）

前置：JDK **17**（AGP 8.x 要求）、`ANDROID_HOME`（需 `platforms;android-36`、`build-tools;36.0.0`、`ndk;27.0.12077973`）、
Rust target `aarch64-linux-android`。工程已在 `src-tauri/gen/android`（`tauri android init` 产物）。

```powershell
# 一条命令：前端构建 → Rust 交叉编译 → 复制 .so → Gradle 出包 → zipalign + 签名
powershell -ExecutionPolicy Bypass -File .\tools\build-android-arm64.ps1

# 已构建过前端 / Rust 时可跳过，仅重打包（最常用）
powershell -ExecutionPolicy Bypass -File .\tools\build-android-arm64.ps1 -SkipFrontendBuild -SkipRustBuild
```

macOS / Linux（环境变量由 `~/.config/chatters/android-env.sh` 提供，详见
`requirements/18-environment-setup.md` §6.2）：

```bash
bash tools/build-android-arm64.sh           # release + debug 签名回退
bash tools/build-android-arm64.sh --debug   # debug 构建
```

产物：`src-tauri/target/android/Chatters-1.0.0-rc.1-arm64-debugsigned.apk`（本机实测 12 MB，含 `lib/arm64-v8a/libchatters_lib.so`）。

```powershell
# 真机安装（首次需在系统设置里允许「未知来源」/「安装未知应用」）
adb install -r src-tauri\target\android\Chatters-1.0.0-rc.1-arm64-debugsigned.apk
```

> - 该包用 Android **debug keystore** 签名（`~/.android/debug.keystore`），仅用于 sideload 测试，
>   **不可用于应用商店**；正式发布需换真实 keystore（`01-project-overview.md` Q13）。
> - `gen/android/.../jniLibs/<abi>/libchatters_lib.so` 与 `build/` 产物均被 `.gitignore` 忽略，
>   新克隆的仓库**不会有 APK**，需按上面命令重新构建。
> - 只出 arm64；armv7/x86 需另编对应 Rust target 并复制 `.so` 后再 `assembleArmRelease` / `assembleX86Release`。
> - 图标：源头是 `src-tauri/icons/chatters-512.png`（`npx tauri icon` 可重生成全套）；
>   打包脚本会自动把 `icons/android/**` 同步进 `gen/android/.../res/**` 并校验 SHA-256，
>   无需手工复制（遗漏会导致 APK 仍是 Tauri 默认启动器图标）。
> - Android 无「打开所在文件夹」能力，接收卡片只保留「打开文件 / 另存为」。
> - 移动端收不到 UDP 广播时（Android Wi-Fi 省电策略），发现服务会每 10 s 主动 query 让对端单播回应；
>   45 s 无包才判定离线。切回前台会自动刷新设备列表。
> - 手机发送图片/文件走系统选择器（SAF）：选中项会先复制到应用缓存 `cacheDir/imports/` 再发送，
>   发送失败不会静默——会话页会显示「操作失败：…」。
> - 未读徽标只在**不在该会话页**时累计；进入会话即标记已读。

## 发版与自动更新

### 版本号

发版前同步三处（当前 `1.0.0-rc.1`）：

- `src-tauri/tauri.conf.json` → `version`（决定安装包名与更新版本比较）
- `src-tauri/Cargo.toml` → `version`
- `package.json` → `version`

### 触发 Release

```powershell
git add -A
git commit -m "chore(release): 1.0.0-rc.2"
git push origin main

git tag v1.0.0-rc.2
git push origin v1.0.0-rc.2   # 打 tag 即自动构建 + 发布
```

或 Actions → **Release** → Run workflow：

| 输入         | 说明                                                                  |
| ------------ | --------------------------------------------------------------------- |
| `tag`        | 如 `v1.0.0-rc.2`                                                      |
| `prerelease` | 测试版勾选（不参与 `releases/latest`）                                |
| `targets`    | `all` / `windows` / `android`：只跑一侧用于快速重试（例如只重打 APK） |

### 产物

| 文件                               | 说明                                                            |
| ---------------------------------- | --------------------------------------------------------------- |
| `Chatters_<version>_x64-setup.exe` | Windows NSIS 安装包（未签名前 SmartScreen 会提示）              |
| `chatters.exe`                     | 绿色免安装版                                                    |
| `Chatters-arm64-release.apk`       | Android arm64（release keystore 签名，首次安装需卸载 debug 包） |
| `*.exe.sig`、`latest.json`         | Tauri 自动更新签名与清单（配置了签名密钥时生成）                |
| `SHA256SUMS.txt`                   | 全部产物的 SHA-256                                              |

### 自动更新（桌面端）

- 客户端启动 5 秒后静默检查；设置页「版本与更新」可手动检查 / 下载安装 / 回滚
- 更新源：`https://github.com/mirrows/chatter/releases/latest/download/latest.json`
- 更新包用 minisign 密钥校验，私钥 `D:\secure\chatters-updater.key`
  （密码 `D:\secure\updater-key-password.txt`，**切勿提交仓库**）
- CI 需要在仓库 Secrets 配置：

| Secret                               | 值                                    |
| ------------------------------------ | ------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | `chatters-updater.key` 文件的全部内容 |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 该密钥的密码                          |

> 未配置 `TAURI_SIGNING_PRIVATE_KEY` 时，Release workflow 自动改用
> `--config '{"bundle":{"createUpdaterArtifacts":false}}'` 构建普通安装包（不产出
> `latest.json`），不会让 CI 变红；配置后自动生成更新产物。

### 自动回滚

1. 安装更新前把当前 `chatters.exe` 备份到 `<app_data>/update/previous.exe` 并写 `pending.json`；
2. 新版本启动 8 秒后前端确认健康 → 删除 `pending.json`；
3. 连续 **2 次**启动都未确认（Rust panic / 前端白屏 / 秒退）→ 下次启动自动用备份覆盖并重启旧版本，界面提示一次；
4. 设置页「版本与更新 → 回滚到上一版本」可手动回滚（有备份时显示）。

### 撤销一次有问题的发布

Actions → **Rollback Release** → Run workflow：

- `bad_tag`：有问题的 tag
- `good_tag`：上一个已知良好的 tag

工作流会用旧版本的 `latest.json` 覆盖坏版本，并把坏版本标记为 pre-release：

- 尚未升级的用户检查更新时看到旧版本号 → 不会被引导升级；
- 已升级的用户由客户端本地自动回滚兜底。

## 开发与验证

要求：Node **≥ 20**（推荐 **22.22.2**：锁定依赖 `abbrev@5` / `nopt@10` 要求 22.22.2 / 24.15 / 26+；
本机若 PATH 中存在旧版 Node，请先把新版加入 PATH）、Rust stable、Windows 10/11 x64。

> macOS 开发环境（Node/Rust/Xcode 版本要求、必做配置、系统权限、打包差异）见
> `requirements/18-environment-setup.md`；可执行 `bash tools/check-env.sh` 一键自检。

```powershell
npm install
npm run tauri dev          # 启动桌面应用
```

```powershell
# 前端门禁
npm run test               # Vitest（tests/**/*.spec.ts）
npm run lint               # ESLint 9（eslint.config.js，含 .vue）
npx vue-tsc --noEmit
npx prettier --check "src/**/*.{ts,vue}" "tests/**/*.ts"

# 后端门禁
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

> 依赖安装需联网；日常构建与测试均可离线执行（`cargo test --offline`）。
