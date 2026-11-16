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

## 8. M9 原生能力实现（①②③④，2026-10-29）

按 `① MulticastLock → ③ SAF/打开文件 → ④ 通知权限 → ② 前台服务` 的顺序全部实现，并在模拟器上逐项验证。

### 8.1 代码结构

| 层次 | 文件 | 职责 |
|---|---|---|
| Rust 桥 | `src-tauri/src/platform/android.rs` | 用 `PluginHandle::run_mobile_plugin` 调 Kotlin：`ping` / `acquireMulticastLock` / `releaseMulticastLock` / `openFile` / `saveFile` / `startForegroundService` / `stopForegroundService` |
| Rust 注册 | `src-tauri/src/lib.rs` | `tauri::plugin::Builder::<Wry, ()>::new("chatters-android")` + `api.register_android_plugin("com.chatters.app", "ChattersAndroidPlugin")`；启动后申请组播锁；传输事件驱动前台服务；`run_mobile()` 初始化 `env_logger`（Android 无 `main.rs`） |
| 命令层 | `src-tauri/src/commands.rs` | `open_transfer_file` → FileProvider + `ACTION_VIEW`；`save_transfer_as` → SAF；`reveal_transfer_file` → 明确提示“Android 不支持定位” |
| Kotlin 插件 | `gen/android/.../ChattersAndroidPlugin.kt` | 上述 7 个 `@Command` + `@ActivityCallback saveFileResult`（复制到用户选择的 Uri） |
| Kotlin 服务 | `gen/android/.../TransferService.kt` | `dataSync` 前台服务 + 低优先级常驻通知，`onDestroy` 摘通知 |
| Manifest | `AndroidManifest.xml` | `CHANGE_WIFI_MULTICAST_STATE`、`ACCESS_WIFI_STATE/NETWORK_STATE`、`POST_NOTIFICATIONS`、`FOREGROUND_SERVICE(_DATA_SYNC)`；声明 `.TransferService` |
| 启动项 | `MainActivity.kt` | Android 13+ 首次启动申请通知权限；插件由 Rust 注册（Kotlin 侧无 `registerPlugin`） |
| 共享路径 | `res/xml/file_paths.xml` | 新增 `<files-path>`，让 `app_data_dir/downloads` 可被 FileProvider 暴露 |

### 8.2 模拟器实测（API 36 / x86_64）

| 能力 | 验证方式 | 结果 |
|---|---|---|
| ① MulticastLock | `adb logcat` | ✅ `WifiService: acquireMulticastLock uid=10217 tag=chatters-discovery` |
| ④ 通知权限 | 首启弹窗 | ✅ 出现「Allow Chatters to send you notifications?」，允许后 `POST_NOTIFICATIONS` granted |
| ② 前台服务 | 传输期间 `logcat` / `dumpsys` | ✅ `Background started FGS: Allowed … com.chatters.app/.TransferService`，传输结束自动停止 |
| ③ 文件接收 | PC → Android 图片 | ✅ 10529 字节 `done`，会话内显示预览 |
| ③ SAF 导出 | 卡片「另存为」 | ✅ 弹出 DocumentsUI（`PickActivity`，默认 Downloads，文件名预填 `chatters-128.png`），保存后 `/sdcard/Download/chatters-128.png` 为 10529 字节 |
| ③ 打开文件 | 卡片「打开文件」 | ✅ 调起 Google Photos（`FileProvider` + `ACTION_VIEW` 生效） |
| ③ 定位文件 | 卡片「打开所在文件夹」 | ⚠️ 按设计返回提示「Android 不支持定位文件，请使用「另存为」」 |
| M10 重连 | 杀探测端后重启 | ✅ 界面显示「正在重连…（第 1 次）」 |

> 说明：模拟器 NAT 会丢弃 `255.255.255.255` 广播，**广播发现本身仍需真机验证**；本轮只验证了“锁已持有”这一必要条件。

### 8.3 已知限制（真机验收时关注）

1. 组播锁随进程存活，未在切后台时释放（后续可改成前后台切换时 acquire/release）；
2. 前台服务仅在“有传输任务”时启动，空闲时不常驻——符合省电要求，但**应用被系统杀死后不会自动恢复**；
3. SAF 导出是“复制”语义：私有目录原件保留，导出后不自动删除；
4. 通知权限若被用户拒绝，传输完成提醒不会出现（不影响传输本身）；
5. Android 端仍无「打开所在文件夹」，UI 按钮保留但会给出提示，后续可考虑按平台隐藏。

### 8.4 真机验收清单（2026-11-14 结果）

- [x] 同一 Wi-Fi 下 15 s 内自动发现 PC（组播锁生效）
- [x] AP 隔离网络下用手动 IP 仍可建连（兜底路径）
- [x] 传输中切后台 / 锁屏 10 分钟不中断，通知栏可见常驻通知
- [x] 1 GB 文件接收 → SAF 导出到用户目录，重名不覆盖
- [ ] 拒绝通知权限时传输仍正常，仅无提醒 —— 未单独复测，列入 `17-v3-followups.md`
- [ ] Android 11 / 13 / 14 各一台（OEM 后台策略差异） —— 仅在现有真机上验证，多版本矩阵列入 `17-v3-followups.md`

## 9. 可安装 APK 产出（arm64 + debug keystore，2026-11-02）

前置环境（本机实测）：`JAVA_HOME=D:\tools\jdk17\jdk-17.0.2`、`ANDROID_HOME=%LOCALAPPDATA%\Android\Sdk`
（platforms `android-36`、build-tools `36.0.0`、NDK `27.0.12077973`）、Rust target `aarch64-linux-android`、
Gradle `D:\tools\gradle\gradle-9.6.1`。工程项目 `src-tauri/gen/android` 已入库（`.so` 与 `build/` 不入库）。

### 9.1 复现命令

```powershell
# 一条命令：前端构建 → Rust 交叉编译 → 复制 .so → Gradle 出包 → zipalign + 签名
powershell -ExecutionPolicy Bypass -File .\tools\build-android-arm64.ps1

# 已有 dist/ 与 aarch64 .so 时，仅重打包
powershell -ExecutionPolicy Bypass -File .\tools\build-android-arm64.ps1 -SkipFrontendBuild -SkipRustBuild
```

单步等价命令：

```powershell
$env:JAVA_HOME='D:\tools\jdk17\jdk-17.0.2'
Copy-Item 'src-tauri\target\aarch64-linux-android\release\libchatters_lib.so' `
          'src-tauri\gen\android\app\src\main\jniLibs\arm64-v8a\libchatters_lib.so' -Force
cd src-tauri\gen\android
D:\tools\gradle\gradle-9.6.1\bin\gradle.bat assembleArm64Release -x rustBuildArm64Release --console=plain
# → app\build\outputs\apk\arm64\release\app-arm64-release-unsigned.apk
```

### 9.2 产物清单（2026-11-02 实测）

| 项 | 值 |
|---|---|
| 签名包 | `src-tauri/target/android/Chatters-1.0.0-rc.1-arm64-debugsigned.apk` |
| 大小 | 11.48 MB（中间件 aligned 11.42 MB） |
| SHA-256 | `83BE398593120C60480FE35C289B438112C0FA5D51939CFAD1AD051F802A2F65`（2026-11-16 v3/M11 最终包） |
| 原生库 | `lib/arm64-v8a/libchatters_lib.so`（9.38 MB，含当前前端资源） |
| 包名 / 版本 | `com.chatters.app` / versionCode `1000000`，versionName `1.0.0-rc.1` |
| SDK | minSdk `24`、targetSdk `36`、compileSdk `36` |
| 签名方案 | v2 + v3（未启用 v1），证书 `C=US, O=Android, CN=Android Debug` |
| 安装验证 | ⏳ 未执行：`adb devices` 无设备（仍无真机） |

校验命令：

```powershell
$bt="$env:ANDROID_HOME\build-tools\36.0.0"
& "$bt\aapt2.exe" dump badging <apk> | Select-String 'package:|sdkVersion|native-code'
& "$bt\apksigner.bat" verify --verbose --print-certs <apk>
```

### 9.3 已知限制与排错

1. **只出 arm64**：armv7/x86 需另编 `armv7-linux-androideabi` / `i686-linux-android`，复制对应 `.so` 后再
   `assembleArmRelease` / `assembleX86Release`（脚本支持 `-AndroidAbi` / `-GradleVariant` / `-ApkAbi` 覆盖）。
2. **历史空壳包**：`build/outputs/apk/{arm,arm64,x86}/release/app-*-unsigned.apk` 曾只有 1.63 MB 且不含 `lib/`。
   原因是 `jniLibs` 当时只手工放过 `x86_64` 的 `.so`，Gradle 跳过 rust 任务后只打出无 native 库的空包。
   **该目录旧产物不可用，以 `src-tauri/target/android/` 为准。**
3. **不要直接 `tauri android build`**：本机 Windows 下 rust 任务找 `npm.bat`（实际只有 `npm.cmd`）失败，
   `.so` 软链需开发者模式/管理员（§7.2）。故固定「手工 copy `.so` + `-x rustBuild*`」。
4. **跳过 rust 任务不会更新 `.so`**：改过 Rust 代码后必须去掉 `-SkipRustBuild`（或手工
   `cargo build --release --target aarch64-linux-android --features tauri/custom-protocol`）。
   判断 `.so` 是否含最新前端：在二进制里搜 `dist/assets/index-*.js` 的文件名（本次为 `index-DPub4BO6.js`）。
5. **debug keystore 仅用于 sideload**：`~/.android/debug.keystore`（pass/alias 均为 `android`），
   上架需真实 keystore + `signingConfigs`（`13` §9.5、`01` Q13）。
6. **APK 不入库**：`gen/android/.gitignore` 忽略 `build` 与 `jniLibs`，根 `.gitignore` 忽略 `target/`；
   新克隆仓库无 APK，需按 §9.1 重建。

### 9.4 后续（真机验收结果，2026-11-14）

- [x] `adb install -r` 安装并启动，确认不白屏、可手动 IP 建连
- [x] 接 §8.4 真机清单（广播发现 / 锁屏保活 10 min / 1 GB SAF 导出）
- [ ] 正式签名：真实 keystore + `signingConfigs`，同时产出 armv7/arm64 双 ABI 或 AAB
  —— **未做，转入后续迭代**（见 `17-v3-followups.md` 第 1 节）


## 10. 真机验收问题与修复（2026-11-02）

首轮真机联调（PC ↔ 手机同一局域网）暴露 5 个问题，全部已定位到代码并修复。

| # | 现象 | 根因 | 修复 |
|---|---|---|---|
| 1 | PC 启动弹出命令行窗口 | `src-tauri/src/main.rs` 未声明 `windows_subsystem`，二进制默认 console 子系统 | 加 `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`；release 下日志可用 `CHATTERS_LOG=1` 落到 `%APPDATA%\com.chatters.app\chatters.log` |
| 2 | 手机端约 15–20 s 后设备从列表消失，Android→PC 发送报 `peer not found: <uuid>` | `MulticastLock` 只保组播，Android 省电策略过滤**入站广播**；发现只在启动时发一次 `query`，之后靠 5 s 广播 announce，`OFFLINE_AFTER=15s` 把对端 sweep 掉；`ensure_session` 只查发现表 | ① 每 10 s 发 `query`（对端必然**单播**回应），并对已知设备逐个单播 query；② 离线阈值 15 s → 45 s；③ 会话层缓存上次握手地址（`SessionHub::known_addr`），发送时三级回退；④ 新增 `refresh_devices` + 切回前台/发送失败自动刷新 |
| 3 | Android「打开所在文件夹」无意义 | 前端无平台信息，按钮无条件渲染 | `get_local_device` 增加 `platform`；Android 隐藏该按钮，保留「打开文件 / 另存为」 |
| 4 | Windows 图片预览不可用 | `get_image_preview` 只查内存任务表；重启后表为空（前端从 `history.json` 重建卡片）→ `unknown transfer` | 回退历史记录（`HistoryStore::find_transfer`）；失败原因分档（未完成 / 文件丢失 / 过大）并在卡片上显示 |
| 5 | Windows 右键弹出 WebView 默认菜单 | 未禁用 WebView2 上下文菜单 | Windows 前端拦截 `contextmenu`（`src/utils/contextmenu.ts`） |

### 10.1 复测清单

- [ ] 手机打开 App 等 60 s，设备列表仍稳定显示 PC（广播过滤不再导致误离线）
- [ ] 手机→PC 连发 3 条文字全部「已送达」；切后台 2 分钟回前台再发仍成功
- [ ] PC 重启后打开旧会话，历史图片有预览
- [ ] PC release exe 双击无黑框、右键无菜单
- [ ] Android 接收卡片只有「打开文件 / 另存为」


### 10.2 第二轮真机问题（2026-11-04）

| # | 现象 | 根因 | 修复 |
|---|---|---|---|
| 6 | PC 发送图片后，**发送方**自己也看不到预览（要等对端接收完成） | `TransferCard` 只在 `state=done` 渲染预览；`get_image_preview` 只认 `info.path`（接收落盘路径），发送任务的源文件路径存在 `Task.source` 里没被使用 | `TransferManager::source_path()` 暴露源文件；`preview_source` 增加「发送方向 → 用源文件，且不要求 done」分支；卡片改为「发送方向始终可预览，接收方向需 done」 |
| 7 | 移动端能收，但**发**图片/文件失败且无任何提示，PC 无响应、手机也不出现传输卡片 | ① Android 文件选择器只返回 `content://` Uri，`pick_file` 直接 `into_path()` 失败；Rust 无法读 SAF 流；② `pickAndSend` 把错误写进 `transfers.error`，但 Chat 页面从不渲染该字段 | ① Kotlin 新增 `importFile`（复制 SAF 流到 `cacheDir/imports`，重名自动加序号）＋ Rust `platform::android::import_file`＋`pick_file` 按 `FilePath::Path/Url` 分支处理；② Chat 页新增传输错误横幅（可关闭），i18n `chat.transferFailed` |
| 8 | 正在与该对端聊天时收到消息，返回列表仍显示未读徽标 | `chat.receive()` 无条件 `unread[peerId] += 1`，只有挂载时 `markRead` 一次 | `chat` 增加 `activePeer` + `setActive()`：进入会话即清零，会话内收到该对端消息不累计；离开会话后恢复累计。`Chat.vue` 挂载/卸载时设置 |

### 10.3 复测清单（第二轮）

- [ ] PC 发送图片后，**PC 端立即**看到缩略图（不必等手机接收完成）
- [ ] 手机点「发送图片/发送文件」能选中相册/文件并成功发出，PC 收到
- [ ] 手机发送失败时，会话页显示「操作失败：…」横幅（可关闭）
- [ ] 停留在与该对端的会话页时收到消息：返回设备列表**无**未读徽标
- [ ] 停留在设备列表或别的会话时收到消息：未读徽标正常累计，进入会话后清零

