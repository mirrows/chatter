# Chatters 开发环境配置指南（macOS 桌面端）

> 版本：v1.0　日期：2026-11-21　适用版本：1.0.0-rc.1（Tauri 2.12.5 + Vue 3.5 + Rust）
> 目的：说明运行 / 构建本项目所需的环境，以及在本机（macOS 26.5.1 arm64）上还缺什么、怎么补。
> 依据：`README.md`「开发与验证」、`rules.md` R3.1 / R7.x、`.github/workflows/ci.yml`、
> `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`、
> `src-tauri/tauri.conf.json`（含 `tauri.macos.conf.json`）。

## 1. 项目形态与技术栈

Chatters 是局域网 P2P 传输工具（文字 / 图片 / 文件），**无服务器、免登录**，网络通信全部由
Rust 后端完成（UDP 28100 发现 + TCP 28101 会话），前端只负责界面：

| 层 | 技术 | 版本（锁定） | 入口 |
|---|---|---|---|
| 桌面外壳 | Tauri | 2.12.5（`@tauri-apps/cli` 2.12.4） | `src-tauri/` |
| 前端 | Vue 3 + Pinia | 3.5.44 / 3.0.x | `src/` |
| 构建 | Vite + vue-tsc | 7.3.7 / 3.x | `vite.config.ts` |
| 测试（前端） | Vitest + jsdom | 4.1.12 / 27.x | `vitest.config.ts`，`tests/**/*.spec.ts` |
| 后端 | Rust（tokio） | 1.96.0（本机） | `src-tauri/src/` |
| 后端测试 | cargo test（集成测试登记在 `Cargo.toml`） | — | `src-tauri/tests/` |

运行 `npm run tauri dev` 时：Vite 在 **5173** 起开发服务器（`strictPort: true`），Tauri 编译并运行
Rust 二进制；首次编译会拉取全部 crate 并生成 `src-tauri/gen/schemas/`。

## 2. 环境要求

| 类别 | 要求 | 依据 |
|---|---|---|
| Node.js | **22.22.2（推荐）、24.15+ 或 26+**；最低不能低于 20.19 / 22.12 | `package-lock.json` 中 `abbrev@5` / `nopt@10` 的 `engines`（22.22.2 / 24.15 / 26 三档）；Vite 7 要求 20.19 / 22.12 |
| npm | ≥ 10（随 Node 22 自带；lockfileVersion 3） | `package-lock.json` |
| Rust | stable（Tauri 2 要求 ≥ 1.77.2，2.x 后续版本可能更高） | `Cargo.toml`（edition 2021）、CI `dtolnay/rust-toolchain@stable` |
| rustfmt / clippy | 必备（`cargo fmt --check`、`clippy -D warnings` 是门禁） | `rules.md` R3.1 |
| Xcode 或 Command Line Tools | 必备（链接器 + macOS SDK；Tauri 要求 macOS 10.15+） | Tauri 官方前置条件 |
| Git | 必备 | 仓库为 git，提交规范见 `rules.md` R4 |
| 端口 | 5173（Vite dev）、28100/UDP（发现）、28101/TCP（会话） | `vite.config.ts`、`tauri.conf.json`、README |
| 网络 | 仅依赖安装需要联网（npm 源 + crates.io）；日常构建/测试可离线 | `rules.md` R7.1 |
| 磁盘 | 建议 ≥ 5 GB（`node_modules` + Rust `target/`） | — |

> 系统版本：macOS 打包 `minimumSystemVersion` 为 **12.0**（`tauri.macos.conf.json`），
> 开发机需 macOS 10.15+ 且安装 Xcode/CLT。
>
> Node 版本提示：Vite 7 自身只要求 `^20.19 || >=22.12`，但锁定的 `abbrev@5.0.0` / `nopt@10.0.1`
> 要求 `^22.22.2 || ^24.15.0 || >=26.0.0`。本机用 **22.12.0** 执行 `npm ci` 会出现 3 条
> `EBADENGINE` 警告（实测），**22.22.2** 无警告且测试全绿。

## 3. 本机实测结果（2026-11-21）

环境：macOS **26.5.1 (25F80)**、**arm64**（Apple Silicon）、磁盘剩余 761 GiB。

| 项 | 实测 | 状态 |
|---|---|---|
| Node.js 默认版本 | 检查时为 v16.20.0（nvm `default -> 16.20.0`）；**已执行 `nvm alias default 22.22.2` 并在 `~/.zprofile` / `~/.zshrc` 显式 `nvm use default`，新终端（含 IDE）均为 v22.22.2** | ✅ 已修复 |
| Node.js 已装版本 | v22.22.2、v22.12.0（nvm 管理） | ✅ |
| npm | 检查时 8.19.4（跟随 Node 16）；切到 Node 22 后为 10.9.7 | ✅ 已修复 |
| Rust（桌面） | 1.96.0（Homebrew，arm64 原生二进制） | ✅ |
| rustfmt / clippy | rustfmt 1.9.0 / clippy 0.1.96 | ✅ |
| rustup / Android target | rustup 1.29.3 的 stable 工具链（rustc 1.100.0）+ target `aarch64-linux-android`（`~/.cargo/bin`） | ✅ 2026-11-21 配置 |
| Xcode / SDK | Xcode 26.6 (17F113)、CLT 指向 Xcode、macOS SDK 26.5、clang 21.0.0 | ✅ |
| Git | 2.54.0 | ✅ |
| `node_modules/` | 已安装（151 MB，vite 7.3.7 / vue 3.5.44 / vitest 4.1.12，与 lock 一致） | ✅ |
| `src-tauri/target/` | 已生成（debug 编译产物；首次 `cargo test --offline` 成功） | ✅ |
| Cargo 本地缓存 | 已补齐（`cargo fetch` 拉取 401 个 crate / 71.5 MiB；之后 `cargo fetch --offline` 通过） | ✅ |
| JDK | Temurin 17.0.21（`~/Library/Java/JavaVirtualMachines/temurin-17.jdk`） | ✅ 2026-11-21 配置 |
| Android SDK / NDK | `~/Library/Android/sdk`：platforms 36、build-tools 36.0.0、NDK 27.0.12077973、platform-tools 37.0.3 | ✅ 2026-11-21 配置 |
| Gradle | wrapper 9.6.1（`~/.gradle/wrapper/dists`，已用腾讯镜像预置） | ✅ 2026-11-21 配置 |
| 端口 5173 / UDP 28100 / TCP 28101 | 全部空闲 | ✅ |
| 网络 | `registry.npmmirror.com` 与 `index.crates.io` 均返回 200 | ✅ |

**结论**：Node 已切到 22.22.2，前端依赖、Rust 依赖与 Android 工具链均已就绪；
前端 + 后端测试全部通过（前端 236、Rust 147 + 2 ignored），Android arm64 APK 已在本机
成功构建并完成 debug 签名（见 §6.2）。当前可直接 `npm run tauri dev` 或
`bash tools/build-android-arm64.sh`。


## 4. 必做配置步骤

### 4.1 把 Node 22.22.2 设为默认（关键）

当前 shell 的 PATH 第一位若仍是 `~/.nvm/versions/node/v16.20.0/bin`，即使直接调用 Node 22 目录下的
`npm`，其 shebang 仍会解析到 Node 16（实测会打印 npm 10「不支持 Node.js v16」告警）。
因此必须让 PATH 指向 Node 22：

```bash
export NVM_DIR="$HOME/.nvm" && . "$NVM_DIR/nvm.sh"
nvm use 22.22.2
nvm alias default 22.22.2      # 新开终端生效；VS Code 需重开终端窗口
```

验证（新终端）：

```bash
node -v   # 期望 v22.22.2
npm -v    # 期望 10.9.7
```

> 不要用 22.12.0：`npm ci` 会为 `abbrev@5` / `nopt@10` 打印 `EBADENGINE` 警告（实测）。
>
> 本机已于 2026-11-21 执行 `nvm alias default 22.22.2`，并在 `~/.zprofile`（登录 shell）与
> `~/.zshrc`（交互 shell，覆盖 IDE 终端）中追加了显式的 `nvm use default`，
> 因此 `zsh -l` / `zsh -i` 均为 v22.22.2（已验证）。
> 若在极端情况下仍看到 v16，执行一次 `nvm use 22.22.2` 即可。
>
> 若只想临时验证、不改默认版本：`nvm exec 22.22.2 node -v` 或在子 shell 里
> `PATH="$HOME/.nvm/versions/node/v22.22.2/bin:$PATH" bash`。

### 4.2 安装前端依赖

```bash
cd /Users/david/Files/chatter
npm ci        # 严格按 package-lock.json 安装；tarball 指向 registry.npmmirror.com
```

> 若公司网络无法访问 npmmirror，可临时 `npm ci --registry=https://registry.npmjs.org`。

### 4.3 预取 Rust 依赖（首次必须联网）

```bash
cd /Users/david/Files/chatter/src-tauri
cargo fetch   # 补齐本地缓存缺失的 crate（见 §3）
```

完成后即可按 README 用离线方式日常构建：`cargo test --offline`。

### 4.4 启动桌面应用

```bash
cd /Users/david/Files/chatter
npm run tauri dev
```

- 首次会编译数百个 crate，耗时数分钟；之后增量编译。
- Vite 固定占用 5173，端口被占用会直接失败（`strictPort: true`）。
- 若需要现场日志：`CHATTERS_LOG=1 npm run tauri dev`（日志写入应用数据目录）。
- 桌面端关闭窗口**不会退出**，而是隐藏到托盘；真正退出走托盘菜单「退出」。

### 4.5 门禁自检（与 CI 一致）

```bash
# 前端
npm run test
npm run lint
npx vue-tsc --noEmit
npx prettier --check "src/**/*.{ts,vue}" "tests/**/*.ts"

# 后端
cd src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

> `npm run test` 只扫描 `tests/**/*.spec.ts`；Rust 集成测试需在 `Cargo.toml` 的 `[[test]]` 登记
> （`discovery_service` / `text_messaging` / `session_reconnect` / `file_transfer`）。
> 集成测试会真实绑定 UDP 28100 / TCP 28101，运行前请确认端口空闲。
>
> **本机实测（2026-11-21，Node 22.22.2）**：
> - 前端：`npm run test` → **236 passed（27 文件）**；`npm run lint` 退出码 0；
>   `npx vue-tsc --noEmit`、`prettier --check` 均通过。
> - 后端：`cargo test --offline` → **147 passed / 0 failed / 2 ignored**
>   （lib 121 + discovery 5 + file_transfer 14（2 ignored）+ reconnect 2 + text 5）；
>   `cargo fmt --all -- --check` 与 `cargo clippy --offline --all-targets -- -D warnings` 均通过。
> - 前端构建：`npm run build`（vue-tsc + vite build）通过（产物 `dist/`，已被 gitignore）。

## 5. macOS 运行期权限与数据目录

### 5.1 系统授权（不授权会导致「设备列表为空」）

| 授权 | 位置 | 说明 |
|---|---|---|
| 本地网络 | 系统设置 → 隐私与安全性 → 本地网络 | macOS 15+ 新增。未允许时 UDP 广播与 TCP 连接会被拦截；首次 `tauri dev` 启动后应弹窗，若错过需手动打开 |
| 防火墙传入连接 | 系统设置 → 网络 → 防火墙 → 选项 | 阻止传入连接会导致对端无法连上本机 TCP 28101 |
| 通知 | 系统设置 → 通知 → Chatters | 托盘通知、待确认文件、接收完成提示 |

### 5.2 数据位置（macOS 与 README 中 Windows 路径的对应关系）

| 内容 | macOS 路径 |
|---|---|
| 配置 | `~/Library/Application Support/com.chatters.app/config.json` |
| 历史（消息 + 传输记录） | `~/Library/Application Support/com.chatters.app/history.json` |
| 错误日志（保留 7 天） | `~/Library/Application Support/com.chatters.app/logs/YYYY-MM-DD/errors.log` |
| 现场日志（`CHATTERS_LOG=1`） | `~/Library/Application Support/com.chatters.app/chatters.log` |
| 接收文件（默认） | `~/Downloads/Chatters`（可在设置页修改） |
| 续传临时文件 | `<接收目录>/.chatters-partial/` |

> 卸载/重置：退出应用后删除 `~/Library/Application Support/com.chatters.app` 即可（会清空配置与历史）。

## 6. 可选环境（打包 / 其他平台）

### 6.1 macOS 打包（本机已实测）

```bash
cd /Users/david/Files/chatter
bash tools/build-macos-dmg.sh                              # 本机架构
bash tools/build-macos-dmg.sh --target universal-apple-darwin
```

- 打包目标来自 `tauri.macos.conf.json`（`app`，最低系统 12.0）；脚本同时传
  `--config src-tauri/tauri.ci.conf.json` 关闭 updater 产物（主配置
  `createUpdaterArtifacts=true`，未配置签名密钥时会失败）；DMG 由 `hdiutil` 生成，
  **不依赖 Finder AppleScript**（Tauri 自带 dmg 打包在本机实测报 `AppleEvent 超时 (-1712)`，
  CI 同样避免该路径）；
- 默认输出到 `src-tauri/target/macos/`（可用 `--out <dir>` 指定）；
- `bundle.icon` 已补充 `icons/icon.icns`（Tauri 官方要求 macOS 使用 `.icns`）；
- 产物：`src-tauri/target/macos/Chatters-<version>-macos-<arm64|universal>.dmg` 与 `.app.zip`；
- 未签名产物分发时，接收方需右键 →「打开」，或执行
  `xattr -dr com.apple.quarantine /Applications/Chatters.app`；
- 正式签名/公证（Developer ID + notarytool）属于发布项，见 `requirements/17-v3-followups.md` §1.3；
- GitHub Actions：
  - `ci.yml` 的 `macos` job（手动触发）只构建并上传 artifact `chatters-macos-universal`；
  - `release.yml` 的 `macos` job（tag / 手动触发）与 Windows / Android 一起发布到 Releases。

**本机实测（2026-11-21）**：

| 产物 | 大小 | SHA-256（前 16 位） |
|---|---|---|
| `Chatters-1.0.0-rc.1-macos-arm64.dmg` | 4.2 MB | `1cceb65f5e9cdbb2` |
| `Chatters-1.0.0-rc.1-macos-arm64.app.zip` | 3.8 MB | `f5d2239949cadd8a` |
| `Chatters-1.0.0-rc.1-macos-universal.dmg` | 8.6 MB | `53046d6bfc6c0ec8` |
| `Chatters-1.0.0-rc.1-macos-universal.app.zip` | 7.7 MB | `40ce7b89d98da107` |

- `hdiutil verify` 通过；DMG 内含 `Chatters.app` 与指向 `/Applications` 的快捷方式；
- `lipo -info` 显示 universal 可执行文件为 `x86_64 arm64`；
- 未配置签名身份时 **app 未签名**（`codesign -v` 报 not signed；只有可执行文件带链接器
  ad-hoc 签名）。macOS 15+ 首次打开需到「系统设置 → 隐私与安全性 → 仍要打开」，
  或先 `xattr -dr com.apple.quarantine /Applications/Chatters.app`。

### 6.2 Android（arm64 APK，macOS 已配置）

本机（2026-11-21）已完成配置，无需再装 Android Studio：

| 组件 | 版本 / 路径 |
|---|---|
| JDK | Temurin **17.0.21**（`~/Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home`，无需 sudo） |
| Android SDK | `~/Library/Android/sdk`（`ANDROID_HOME` / `ANDROID_SDK_ROOT`） |
| SDK 组件 | `platforms;android-36`、`build-tools;36.0.0`、`platform-tools` 37.0.3、`cmdline-tools;latest`（16111833）、`ndk;27.0.12077973` |
| Rust | rustup stable（rustc **1.100.0**）+ target `aarch64-linux-android` |
| Gradle | 项目自带 wrapper **9.6.1**（`src-tauri/gen/android/gradlew`，已补执行位） |
| 环境变量 | `~/.config/chatters/android-env.sh`（可重复 source），已在 `~/.zprofile` 与 `~/.zshrc` 中引入（新终端生效） |

构建（等价于 CI 的 `npx tauri android build --apk --target aarch64`）：

```bash
# release（未配置发布 keystore 时自动回退 debug 签名，仅可侧载）
bash tools/build-android-arm64.sh

# debug 构建
bash tools/build-android-arm64.sh --debug
```

脚本会自动：校验 Node/JDK/SDK/NDK/rustup target → 调用 Tauri CLI（前端构建 + Rust 交叉编译 +
Gradle）→ 收集 APK → `zipalign` + debug 签名（`~/.android/debug.keystore`，缺失时生成）→
`apksigner verify` 并校验 `lib/arm64-v8a/libchatters_lib.so`。

**本机实测产物**（2026-11-21）：

- `src-tauri/target/android/Chatters-1.0.0-rc.1-arm64-debugsigned.apk`（12 MB）
- APK Signature Scheme **v2 + v3** 校验通过；证书 `CN=Android Debug`
- SHA-256：`b9616fe6c6ce1a8462affbd3a409034542c2f96ce4bf6fdd742de728ea5b242d`
- 安装：`adb install -r src-tauri/target/android/Chatters-1.0.0-rc.1-arm64-debugsigned.apk`

> Windows 仍使用 `tools/build-android-arm64.ps1`（PowerShell）；macOS/Linux 用上面的 `.sh`。
> 正式签名需 `CHATTERS_KEYSTORE` / `CHATTERS_KEYSTORE_PASSWORD` / `CHATTERS_KEY_ALIAS` /
> `CHATTERS_KEY_PASSWORD`，见 `requirements/02-technical-design.md` §11.3；debug 签名不可上架。
>
> 网络说明（本机实测）：`dl.google.com` 与 `services.gradle.org` 直连极慢（约 0.1–0.2 MB/s，
> SDK 组件几乎无法下载）。本次使用镜像完成安装：Android SDK 组件来自
> `https://mirrors.cloud.tencent.com/AndroidSDK/`，Gradle 发行版来自
> `https://mirrors.cloud.tencent.com/gradle/`，Maven 依赖通过
> `~/.gradle/init.d/chatters-mirrors.gradle` 优先走阿里云/腾讯/华为镜像。
> `sdkmanager` 在本机会因访问 `dl.google.com` 长时间卡住，新增组件时建议用镜像手动下载，
> 或在能访问 Google 的网络下再使用。

### 6.3 Windows / Linux

- **Windows**：必须 Windows 10/11 x64 主机（NSIS 打包、`ShellExecuteW`、WebView2）。
  macOS 无法产出 `chatters.exe` / NSIS 安装包。
- **Linux**：需 `libwebkit2gtk-4.1-dev`、`libayatana-appindicator3-dev`、`librsvg2-dev`、
  `libxdo-dev`、`libssl-dev`、`build-essential`（与 CI 的 ubuntu-22.04 job 一致），产物为 deb/AppImage。

### 6.4 可选：IDE 扩展

VS Code 安装 rust-analyzer、Vue - Official、ESLint、Prettier 可获得与门禁一致的实时提示（非必需）。

## 7. 已知风险与注意事项

| 风险 | 说明 / 对策 |
|---|---|
| Node 版本被 nvm 默认值拉回 16 | 必须 `nvm alias default 22.22.2`；VS Code 终端要重开，否则仍是旧 PATH（非登录 shell 不读 `~/.zprofile`） |
| Node 22.12.0 也会告警 | 锁定依赖 `abbrev@5` / `nopt@10` 要求 Node 22.22.2、24.15 或 26 以上，`npm ci` 会打印 `EBADENGINE`（仅警告，但建议直接升到 22.22.2） |
| 首次构建必须联网 | 本地 Cargo 缓存缺少 tauri 2.12.5 及部分插件 crate；`cargo fetch` 后即可离线 |
| macOS 15+ 本地网络权限 | 未允许时表现为「设备列表一直为空」，不是代码 bug；见 §5.1 |
| Homebrew 版 Rust 无 rustup | 桌面开发无影响；Android 交叉编译使用的是 `~/.cargo/bin` 下的 rustup（1.100.0），两者互不冲突 |
| `dl.google.com` / `services.gradle.org` 直连极慢 | 已改用镜像：SDK 走腾讯镜像手动安装、Gradle 发行版预置到 `~/.gradle/wrapper/dists`、Maven 走 `~/.gradle/init.d/chatters-mirrors.gradle`；必要时给 Gradle 加 `GRADLE_OPTS=-Dorg.gradle.internal.http.socketTimeout=180000` |
| `sdkmanager` 长时间无输出 | 本机访问 `dl.google.com` 会被卡住；组件已装齐，新增组件建议用镜像手动下载，或在可访问 Google 的网络下执行 |
| Gradle 复用了失败时的仓库解析 | 删除 `~/.gradle/caches/9.6.1/configuration-cache` 后重试（本次已遇到并解决） |
| 集成测试占用固定端口 | 28100/28101 被占用时测试会失败；先在系统设置或 `lsof` 中确认空闲 |
| 未签名 macOS 应用 | Gatekeeper 会拦截，需右键打开或移除 quarantine 属性 |
| `.icns` 打包图标 | 已加入 `tauri.macos.conf.json` 的 `bundle.icon`；后续修改图标列表时需保留 `icons/icon.icns` |
| 本项目网络行为仅限局域网 | `rules.md` R5.4 禁止任何外网连接；排查防火墙时不要放开公网 |

## 8. 附录：一键环境自检

新增脚本 `tools/check-env.sh`（只读检查，不修改系统）：

```bash
cd /Users/david/Files/chatter
bash tools/check-env.sh
```

检查项与判定：Node 22.22.2 / ^24.15 / ≥26（低于时给 EBADENGINE 警告或失败）、npm ≥ 10、
`rustc`/`cargo`/`rustfmt`/`cargo clippy`、Xcode 命令行工具与 macOS SDK、Git、
5173/28100/28101 是否空闲、npm 与 crates 源连通性、`node_modules` 与 `src-tauri/target` 是否就绪；
Android 部分会检查 SDK 目录、`platforms;android-36`、`build-tools;36.0.0`、NDK 27.0.12077973、
JDK 17、rustup target `aarch64-linux-android` 与 `gradlew` 执行位（均为提示项）。
必需项任一失败时脚本退出码为 1。
脚本已于 2026-11-21 在本机验证：Node 16 环境下正确报 2 项失败（退出码 1）；
Node 22.22.2 + Android 环境变量加载后 **0 失败 0 警告**（退出码 0）。


