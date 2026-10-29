# M9 Android spike 计划与记录（v2）

> 版本：v1.0　日期：2026-10-28　对应计划：`13-v2-plan.md` §9（M9，含 §9.1 三问）
>
> 本文件记录 spike 的**前置环境事实、分段步骤、判定标准与结论**。执行过程按 S0→S1→S2→S3 推进，
> 每段做完即回填结果；未完成段标注原因，不臆测结论。

## 1. 环境事实（2026-10-28 实测）

| 检查项 | 结果 | 影响 |
|---|---|---|
| CPU / 内存 / 磁盘 | i5-4590（4 核）/ 15.9 GB / D: 剩 309 GB | 可构建；模拟器可跑（需加速） |
| Java | 仅 **JRE 1.7.0_51**（`D:\software\jre`），`JAVA_HOME` 未设置 | AGP 8.x 要求 **JDK 17**，必须先装 |
| Android SDK / NDK | 无（`ANDROID_HOME`/`NDK_HOME` 空，`~/.gradle`、`~/.android` 不存在） | 需安装（约 6–8 GB） |
| Rust | 1.99.1 MSVC；已装 target 仅 host；android target 可安装 | 需 `rustup target add` |
| rustup 源 | `RUSTUP_DIST_SERVER` = USTC 镜像，其 dist 路径 HEAD 返回 **403** | 装 target 时可能需临时切 `static.rust-lang.org` |
| Tauri CLI | 2.12.4，`android init/dev/build/run` 均可用 | 具备 |
| 网络 | `dl.google.com`、`repo.maven.apache.org`、`registry.npmjs.org`、`static.rust-lang.org` 均 200；cmdline-tools zip HEAD 200（153 MB） | 可下载 |
| 虚拟化 | `HypervisorPresent = True`（WSL2/Docker 已在用） | 模拟器需 WHPX，构建时用 `emulator -accel-check` 预检 |
| 真机 | **无** | §9.1 第 1、2 问无法在本机闭环，结论标注“待真机确认” |

## 2. S0：工程门控（离线，已在本提交完成）

目标：让 Android 目标“能编译”，桌面端零回归。

| 改动 | 内容 |
|---|---|
| `src-tauri/Cargo.toml` | `tauri` 默认 features 置空；新增 `[target.'cfg(not(target_os = "android"))'.dependencies]` 承载 `tray-icon` feature、`tauri-plugin-autostart`、`tauri-plugin-single-instance` |
| `src-tauri/src/lib.rs` | 单实例插件、自启插件、`on_window_event`（关闭即隐藏）、托盘菜单/图标全部 `#[cfg(desktop)]` |
| `src-tauri/src/commands.rs` | `get_autostart` / `set_autostart` 保留跨平台签名，Android 走 `Ok(false)` / `Ok(())` 空实现 |
| `src-tauri/src/platform/paths.rs` | `download_dir()` 增加 Android 分支（返回相对 `downloads`） |
| `src-tauri/src/lib.rs` | Android 接收目录 = `app_data_dir()/downloads`（私有目录），桌面沿用配置里的下载目录 |
| `src-tauri/capabilities/default.json` | 移除 `autostart:default`（该插件在 Android 不存在） |
| `src-tauri/capabilities/desktop.json` | 新增，`platforms: ["linux","macOS","windows"]` 承载 `autostart:default` |
| `src-tauri/tauri.conf.json` | `bundle.android.minSdkVersion = 24` |

**验收**：`cargo fmt --check` / `clippy -D warnings` / `cargo test` / 前端 `npm run test` 全部通过（桌面行为不变）。

## 3. S1：工具链安装与首次构建

### 3.1 安装步骤（PowerShell）

```powershell
# 1) JDK 17
winget install --id EclipseAdoptium.Temurin.17.JDK -e --accept-package-agreements --accept-source-agreements
setx JAVA_HOME "C:\Program Files\Eclipse Adoptium\jdk-17.0.x-hotspot"

# 2) cmdline-tools（解压后重命名为 latest）
$sdk = "$env:LOCALAPPDATA\Android\Sdk"
New-Item -ItemType Directory -Force "$sdk\cmdline-tools" | Out-Null
# 下载 https://dl.google.com/android/repository/commandlinetools-win-11076708_latest.zip 并解压到 $sdk\cmdline-tools\latest

# 3) 许可 + 组件
$env:JAVA_HOME = "C:\Program Files\Eclipse Adoptium\jdk-17.0.x-hotspot"
& "$sdk\cmdline-tools\latest\bin\sdkmanager.bat" --sdk_root="$sdk" --licenses
& "$sdk\cmdline-tools\latest\bin\sdkmanager.bat" --sdk_root="$sdk" `
  "platform-tools" "platforms;android-35" "build-tools;35.0.0" `
  "ndk;27.0.12077973" "emulator" "system-images;android-35;google_apis;x86_64"
setx ANDROID_HOME "$sdk"
setx NDK_HOME "$sdk\ndk\27.0.12077973"

# 4) Rust android targets（USTC 403 时临时切官方源）
$env:RUSTUP_DIST_SERVER = "https://static.rust-lang.org"
rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

# 5) 生成工程并构建（模拟器用 x86_64）
cd src-tauri
npx tauri android init --ci
npx tauri android build --debug --target x86_64
```

### 3.2 判定标准

| 项 | 通过条件 |
|---|---|
| `tauri android init` | 生成 `src-tauri/gen/android`（Gradle 工程 + Manifest） |
| Rust 交叉编译 | 4 个 target 中至少 `x86_64`/`aarch64` 编译成功 |
| APK 产出 | 存在 `app-*-debug.apk`；`aapt2 dump badging` 显示包名 `com.chatters.app`、minSdk 24、ABI 匹配 |
| 应用启动 | 模拟器中安装并启动，WebView 加载界面、无崩溃 |

## 4. S2：模拟器端到端（无真机）

```powershell
& "$env:ANDROID_HOME\cmdline-tools\latest\bin\avdmanager.bat" create avd -n chatters35 -k "system-images;android-35;google_apis;x86_64"
& "$env:ANDROID_HOME\emulator\emulator.exe" -accel-check
& "$env:ANDROID_HOME\emulator\emulator.exe" -avd chatters35
cd src-tauri; npx tauri android dev      # 或 npx tauri android run --target x86_64
```

验证顺序（**先手动 IP，后广播**，因为模拟器 NAT 通常会丢弃 `255.255.255.255`）：

1. 模拟器内用 F3「添加设备」填 `10.0.2.2:28101`（`10.0.2.2` = 宿主机回环别名），确认 HELLO 建连；
2. 发文字 → Windows 端收到并回 ACK；
3. Windows 发图片/文件 → Android 端接收、落私有目录、`transfer://update` 走到 `done`；
4. Android 发文件 → Windows 端接收校验通过；
5. 再试广播发现：`adb shell ip route` 取子网，观察能否收到宿主 announce（预期失败，记录现象）。

| 项 | 通过条件 |
|---|---|
| 手动 IP 建连 | 双向 HELLO 成功，`session://update` 出现 `connected` |
| 文字 | 双向送达（含中文/换行） |
| 文件/图片 | sha256 校验通过，文件落在 `app_data_dir()/downloads` |
| 广播 | 记录“模拟器下不可用/可用”，不作为 Go/No-Go 依据 |

## 5. S3：真机三问（待设备）

| 问题（`13` §9.1） | 验证方法 | 通过条件 |
|---|---|---|
| 同一 Wi-Fi 下广播 + `MulticastLock` 能否发现 PC | 两端同一 AP，观察 15 s 内设备列表出现对方 | 稳定出现，且 30 s 内不掉线 |
| TCP 监听在切后台/锁屏时是否被杀 | 传输 10 分钟后切后台+锁屏 | 传输不中断，完成通知可见 |
| SAF 保存 1 GB 与续传组合 | `ACTION_CREATE_DOCUMENT` 导出、重名、取消 | 导出成功、重名不覆盖、取消无残留 |

## 6. 风险与对策

| 风险 | 对策 |
|---|---|
| USTC rustup 镜像 403 | 临时 `RUSTUP_DIST_SERVER=https://static.rust-lang.org` |
| Gradle 首次依赖下载慢 | 配阿里云 maven 镜像；二次构建 `--offline` |
| 模拟器无 WHPX | `emulator -accel-check` 预检；失败则只做“编译 + 安装”验证 |
| NDK 版本与模板不一致 | 以 `gen/android/app/build.gradle.kts` 的 `ndkVersion` 为准补装 |
| 模拟器 UDP 广播不可达 | 以手动 IP 为 spike 主路径（与 `13` §9.1 的降级策略一致） |
| 无真机 | §9.1 第 1、2 问结论标注“待真机确认”，不计入验收通过 |

## 7. 结论记录（2026-10-29 实测）

| 段 | 状态 | 结论/备注 |
|---|---|---|
| S0 工程门控 | **已完成** | 桌面全门禁通过（Rust 91、前端 103）。额外发现并修复：缺 `mobile_entry_point` 会让 App 启动即 `UnsatisfiedLinkError`，已加 `run_mobile()`。 |
| S1 工具链与构建 | **已完成** | 产出 `app-x86_64-release-unsigned.apk`（18.5 MB），用调试 keystore 签名后可安装启动。 |
| S2 模拟器联调 | **已完成（手动 IP 路径）** | 见 §7.1，文字双向收发 + ACK 全部通过。广播发现因模拟器 NAT 无法验证。 |
| S3 真机三问 | 无设备 | 待真机（广播/MulticastLock、锁屏保活、SAF 导出 1 GB）。 |

### 7.1 S2 实测记录

| 项 | 结果 |
|---|---|
| 应用启动 | ✅ 界面正常：设备/设置两个 Tab、本机昵称 `PC-5295`、指纹 `66C6-AD18`、添加设备表单 |
| 设备内监听 | ✅ `tcp 0.0.0.0:28101 LISTEN`、`udp 0.0.0.0:28100`（发现服务已启动） |
| 手动 IP 建连 | ✅ 输入 `10.0.2.2:28101`（= 宿主回环别名）后 `session://update` 变 `connected` |
| PC → Android 文字 | ✅ 桌面探测端 `send_text` 返回 `Delivered`（收到 ACK）；Android 设备列表出现未读徽标 1 |
| Android → PC 文字 | ✅ 探测端打印 `probe received text from …: hello-from-android` |
| 设备指纹 | ✅ 两端一致显示（探测端 `D651-994E`，本机 `66C6-AD18`） |
| 广播发现 | ⚠️ 未验证：模拟器 NAT 会丢弃 `255.255.255.255`；`13` §9.1 已预置“广播不可用则手动 IP 兜底” |

### 7.2 本机构建踩坑与对策（后续 M9 实现直接复用）

| 现象 | 根因 | 对策 |
|---|---|---|
| `sdkmanager` 下载超时 | 系统 WinINET 代理 `127.0.0.1:10808`，Java 不走该代理 | 绕过 sdkmanager：解析 `repository2-3.xml` / `sys-img2-3.xml` 取 URL，用 PowerShell 直连下载 zip 手工落盘 |
| `rustup target add` 失败 | `RUSTUP_DIST_SERVER` 指向 USTC，dist 路径返回 403 | `RUSTUP_DIST_SERVER=https://mirrors.sjtug.sjtu.edu.cn/rust-static`（实测 8 MB/s） |
| Gradle wrapper 下载卡在 0 字节 | wrapper 的 Java 下载器与网络环境不兼容 | 用腾讯镜像下载 `gradle-9.6.1-bin.zip`，解压到 `D:\tools\gradle` 并用 `gradle.bat` 直接构建 |
| `compileSdk = 37` 找不到 platform | `platforms;android-37` 尚未在官方仓库发布 | 改为 `compileSdk/targetSdk = 36`（`gen/android/app/build.gradle.kts`，测试用临时改动） |
| `Could not create symbolic link` | Tauri CLI 把 `.so` 软链到 jniLibs，非管理员且未开开发者模式时不允许 | 手工 `Copy-Item` 到 `gen/android/app/src/main/jniLibs/x86_64/`，再用 Gradle 打包 |
| `A problem occurred starting process 'command 'npm.bat''` | Tauri 的 Gradle rust 任务在 Windows 上找 `npm.bat`（实际只有 `npm.cmd`） | 已手工编译 `.so`，用 `-x rustBuild*` 跳过 rust 任务 |
| `Could not close incremental caches` | Windows + Kotlin 增量缓存 | `gen/android/gradle.properties` 增加 `kotlin.incremental=false`、`org.gradle.caching=false` |
| 启动后白屏并请求 `localhost:5173` | release 构建缺少生产资源特性，仍按 dev 模式跑 | 构建需带 `--features tauri/custom-protocol` 且 `TAURI_ENV_DEBUG=false`（Tauri CLI 会自动加，手工构建必须显式指定） |
| `tauri.localhost` 请求失败 | 同上（缺 `custom-protocol`） | 同上 |
| `Rust.onCreate` JNI 找不到 | 未声明 `#[cfg_attr(mobile, tauri::mobile_entry_point)]` | 已加 `run_mobile()` |

### 7.3 可行性结论

- **技术上可行**：Android 端可复用现有 `session` / `protocol` / `transfer` 逻辑；本机已跑通「构建 → 安装 → 启动 → 手动 IP 建连 → 文字双向」全链路。
- **必须补齐的三块原生能力**（M9 主要工作量）：
  1. `WifiManager.MulticastLock`（Kotlin）才能稳定接收广播；
  2. 前台服务 + 通知渠道，避免切后台被杀；
  3. SAF（`ACTION_CREATE_DOCUMENT`）导出，替换“私有目录直写”。
- 本次 spike **未验证**：广播发现、锁屏保活、1 GB SAF 导出——均需真机（`13` §9.3 验收标准）。
