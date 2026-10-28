# Chatters v2 迭代计划

> 版本：v1.0　日期：2026-10-28　对应需求：`01-project-overview.md` v0.5（Q13–Q15）、`02-technical-design.md`（§4.7/§4.8/§11）
> 状态：**进行中** —— M6（代码部分）与 M10 已完成，见 `14-milestone-m6-m10-report.md`；M7/M8/M9/M11 待做。
> 本文件是 v2 范围、验收、测试、依赖与工作量的唯一基准。

## 1. 范围与优先级

| 优先级 | 内容 | 里程碑 |
|---|---|---|
| **P0** | 跨平台地基（路径/文件名/构建/CI） | M6 |
| **P0** | macOS 客户端 | M7 |
| **P0** | Linux 客户端 | M8 |
| **P0** | Android 客户端（含 3 天技术验证 spike） | M9 |
| **P1** | 自动重连（`01` 3.4） | M10 |
| **P1** | 设备指纹（`01` F5） | M10 |
| **P1** | 性能优化（块大小 1MB→2MB、缓冲写） | M10 |
| **P1** | 完整块级续传（`01` Q8 的 v2 项） | M11 |

**明确不做（本迭代）**：传输加密、群发/广播、iOS、消息撤回、历史导出、分屏视图、`resend(msg_id)`、额外 UI 主题。

**阶段性交付口径（Q13）**：允许**未签名**的 EXE / dmg / APK 作为阶段性交付，发布说明需标注 SmartScreen、
Gatekeeper 或 Android 未知来源提示；证书到位后补签，不阻塞功能验收。

## 2. 里程碑顺序与版本节奏

```
M6 跨平台地基
   ├─ M7 macOS ─┐（与 M8 并行）
   ├─ M8 Linux ─┘
M10 自动重连 / 设备指纹 / 性能优化
M11 完整块级续传
M9 Android（spike 第 1 周先做，实现排最后）
```

| 版本 | 包含里程碑 |
|---|---|
| `v2.0.0-rc.1` | M6 + M7 + M8 + M10 + M11 |
| `v2.1.0` | M9（Android） |
| `v2.0.0` GA | 各平台安装包 + 签名 + 验收通过 |

> 说明：M11 排在 M10 与 M9 之间——它与 M10 同属协议/数据层，先完成可让 Android 直接复用，避免返工。
> 若后续决定 Android 提前，只需调整 M11 与 M9 的相对顺序，不改各里程碑内容。

## 3. 兼容性总策略（Q14）

- 协议 `ver` 保持 **1**，通过 `HELLO.features` 做**能力协商**，避免破坏性升级：
  - `chunk2m`：启用 2MB 数据块（否则 1MB）；
  - `chunkmap`：启用块级续传（否则 v1 字节偏移续传）。
- 协商规则：**双方都声明**某能力才启用；任一方缺失即降级；未知能力忽略并记日志。
- 必须保证：**v1 ↔ v2 可互传文字/图片/文件**（使用 1MB 块 + 偏移续传）。

## 4. M6 跨平台地基（5 人日）

> **实施进展（2026-10-28）**：已落地 `platform` 模块（`mod.rs`/`paths.rs`/`shell.rs`）、
> 平台化 `safe_file_name`、按平台拆分的 `tauri.{windows,macos,linux}.conf.json` 与三平台 CI 矩阵；
> Windows 已通过全部门禁，macOS/Linux 构建待实机。详见 `14-milestone-m6-m10-report.md` §2。
> 与下述范围的差异：`cache_dir`/`config_dir` 继续由 Tauri `path()` API 提供，未在 `platform` 重复封装；
> `notification_supported` 暂未引入（通知插件自身在各平台可降级，无调用点）。

### 目标
把"事实上 Windows 专用"的实现抽成平台无关层，保证 Windows/macOS/Linux 可编译、可测试、可出包。

### 范围
| 项 | 改造 |
|---|---|
| 平台能力层 | 新增 `platform` 模块：`download_dir` / `cache_dir` / `config_dir` / `open_file` / `reveal_file` / `tray_supported` / `notification_supported` |
| 文件名规则 | `safe_file_name` 按平台判定：Windows 禁 `: < > " | ? *` 与控制字符；Unix 仅禁 `/` 与 NUL |
| 默认目录 | macOS/Linux 用 `~/Downloads/Chatters`（Linux 遵循 XDG）；Android 由 M9 走 SAF |
| 打开/定位文件 | 现有 `open_command` / `reveal_command` 移入 `platform`，Linux 用 `xdg-open` |
| 托盘/通知降级 | Linux 无 AppIndicator → 仅通知；通知权限失败 → 记录日志并静默降级 |
| 构建配置 | `bundle.targets` 按平台：Windows `nsis`、macOS `dmg`+`app`、Linux `deb`+`appimage`；补 `.icns` 与 Linux 图标尺寸 |
| CI | GitHub Actions matrix（windows-latest / macos-latest / ubuntu-22.04）：fmt、clippy、test、`build --no-bundle` |

### 验收标准
1. 三平台 `cargo fmt --check`、`cargo clippy -D warnings`、`cargo test` 全绿；
2. 三平台 `tauri build --no-bundle` 产出可执行文件；
3. 文件名/目录解析单测按平台断言通过（如 Unix 允许 `a:b.txt`，Windows 拒绝）；
4. Windows 现有回归全绿：Rust 59 单测 + 19 集成、前端 14 文件/98 用例。

### 测试策略
- 单元测试：`#[cfg(target_os)]` 断言文件名与目录解析；
- CI：三平台跑全量测试与构建（首次配置约 1 人日）；
- 手工：三平台各启动一次，日志无 `warn/error`。

### 依赖环境
联网（Rust target、npm 依赖）、macOS 与 Linux 机器/VM、CI 账号。

### 风险
图标与打包格式差异；Linux 托盘库缺失导致的降级逻辑需实测；CI 首次配置耗时。

## 5. M7 macOS 客户端（4 人日）

### 范围
1. 打包 `dmg` + `app`，图标 `.icns`（1024→16），`minimumSystemVersion` 建议 12.0；
2. 签名与公证：`codesign` + `notarytool`（需 Apple Developer 账号）；**无账号时交付未签名 dmg（Q13）**；
3. 托盘：菜单（显示主窗口/退出）、关闭窗口隐藏到托盘；
4. 通知：首次授权请求，失败降级；
5. 自启：LaunchAgent（`MacosLauncher::LaunchAgent` 已配置，需实测注册/注销）；
6. 文件操作：`open` / `open -R` 验证；
7. 本地网络权限：macOS 15+ 首次广播需用户允许，写入使用说明。

### 验收标准
1. dmg 安装后启动正常（签名版无 Gatekeeper 阻断；未签名版按说明放行）；
2. **mac ↔ Windows 互传全通过**：文字、图片（自动接收+预览）、文件（接受/拒绝）、续传、取消；
3. 托盘、关闭到托盘、开机自启（登录项可见）、通知、打开/另存为正常；
4. 发现 ≤3s、文字 ≤1s（与 Windows 同基线）。

### 测试策略
- CI macOS runner 跑 `cargo test`（协议/传输逻辑）；
- 实机按 `11-acceptance-checklist.md` 执行，补 macOS 项（本地网络权限、LaunchAgent）；
- 打包冒烟：挂载 dmg → 拖入 Applications → 启动 → 与 Windows 互传一次。

### 依赖环境
Mac 设备（建议 Apple Silicon 与 Intel 各一次）、Apple Developer 账号（可延后）、联网。

### 风险
公证流程耗时；macOS 15 本地网络权限拦截广播；双架构构建（可先出 arm64）。

## 6. M8 Linux 客户端（4 人日）

### 范围
1. 打包 `deb` + `AppImage`，`.desktop` 文件（图标、分类、启动项）；
2. 托盘：依赖 `libayatana-appindicator3`；GNOME 需 AppIndicator 扩展 → 文档说明；无托盘时降级为"仅通知 + 关闭即退出"；
3. 通知：libnotify/DBus，失败降级；
4. 自启：`~/.config/autostart/*.desktop`；
5. 文件操作：`xdg-open`（文件/目录）；
6. 防火墙：不做自动脚本（发行版差异），文档给出 `ufw` / `firewalld` 放行命令。

### 验收标准
1. `deb` 安装与 `AppImage` 双击均可启动；
2. **Linux ↔ Windows / Linux ↔ macOS 互传全通过**；
3. 托盘/通知/自启/打开文件在 Ubuntu 22.04 与 24.04 实测记录（KDE 至少一次）；
4. 无托盘环境下仍可正常收发，不崩溃。

### 测试策略
- CI ubuntu runner 跑全量测试与 `--no-bundle` 构建；
- 实机（Ubuntu GNOME、KDE）+ 验收清单；
- 打包冒烟：`dpkg -i` → 启动 → 互传一次。

### 依赖环境
Linux 机器/VM（Ubuntu + KDE 各一）、联网（AppImage/deb 打包依赖）。

### 风险
Wayland 限制、GNOME 默认无托盘、发行版库版本差异。

## 7. M10 自动重连 / 设备指纹 / 性能优化（6–9 人日）

> **实施进展（2026-10-28）**：三个子项均已实现并有自动化测试覆盖，详见 `14-milestone-m6-m10-report.md` §3–§5。
> 关键实现选择：
> - 重连改用 `HELLO.tcp_port` 还原对端监听端口（入站连接的对端源端口是临时端口，不能直接复用）；
>   未声明该字段的 v1 对端**不重连**；
> - 指纹按本节 §7.2 的 8 位十六进制（`A3F2-91C0`）实现；
> - 块大小经 `chunk2m` 协商提升到 2MB，实测 loopback 36.6 → 61.6/66.5 MiB/s；
> - “接收端专用阻塞写线程 / `BufWriter`”**未采用**（会破坏以 `.part` 长度为准的续传语义），
>   “发送端双缓冲预读”未做，二者仍是达到 100 MiB/s 的后续手段。

### 7.1 自动重连（2 人日，`01` 3.4）
- 会话层记录对端最近地址；断线后 **5s → 15s → 30s** 退避重连，最多 3 次；
- 仅对"本次运行中曾活跃"的对端重连；应用重启后不自动重连；
- `session://update` 增加 `reconnecting` 状态（含第几次），前端显示"重连中…"；
- 断线时未完成任务保持 `failed`（可手动重试），重连后不自动续传。

**验收**：拔网 30s 内恢复 → 30s 内自动恢复会话并可发消息；3 次失败后停止并提示；不产生重复连接。
**测试**：集成测试模拟断开（drop 连接）+ 退避序列单测；前端状态渲染单测。

### 7.2 设备指纹（1.5 人日，`01` F5）
- 短码 = `sha256(device_id)` 前 8 位十六进制，格式 `A3F2-91C0`；
- 设备列表与会话页显示短码；两端一致即视为同一设备；不匹配时界面提示；
- 可选子项（+1 人日）：首次连接核对弹窗 + 信任列表。

**验收**：短码稳定一致；改昵称/端口不影响短码；不匹配有提示。
**测试**：短码生成单测（固定输入→固定输出）；组件渲染单测。

### 7.3 性能优化（3–5 人日，`01` 3.1）
| 项 | 方案 |
|---|---|
| 块大小 | 1MB → **2MB**（帧上限 4MB，含 20B 头仍安全），经 `chunk2m` 协商启用 |
| 兼容 | 任一方不支持 `chunk2m` → 回落 1MB（保证 v1↔v2 互通） |
| 接收写入 | 专用阻塞写线程 + 1–2MB 缓冲（或大容量 `BufWriter`），减少 blocking-pool 往返 |
| 发送读取 | 双缓冲预读（2×2MB），减少每块调度开销 |
| 通道容量 | 写通道按块大小调整，避免 64×2MB 的无效堆积 |
| 度量 | 扩展 `throughput_256mb`，新增 1GB 基准（`#[ignore]`） |

**验收**：loopback release ≥100 MiB/s（当前 36.6）；真实千兆两机 ≥50 MB/s；10GB 完成且峰值内存 <200MB；v1 互通可用。
**测试**：基准测试 + 真实两机测速 + 10GB 长稳；协商单测（缺失/不匹配）。
**依赖**：复核需两台千兆机器；代码无新依赖。
**风险**：块大小变更影响旧 `.part` 偏移兼容（需回归测试）；真实网络波动。

## 8. M11 完整块级续传（8–12 人日，`01` Q8 的 v2 项）

### 8.1 协议扩展（详见 `02` §4.7）
| type | 名称 | payload |
|---|---|---|
| 0x17 | `FILE_BLOCK_MAP` | `transfer_id(16B) + block_size(4B) + count(4B) + hashes(8B×count)` |
| 0x18 | `FILE_HAVE` | `transfer_id(16B) + count(4B) + bitmap(⌈count/8⌉)` |
| 0x19 | `FILE_MISSING` | `transfer_id(16B) + count(4B) + indices(4B×count)` |

块哈希 = **SHA-256 前 8 字节**（复用 `sha2`，无新依赖；整文件 SHA-256 兜底）。经 `chunkmap` 协商启用。

### 8.2 流程
1. 发送端一次读盘同时算整文件哈希与块表 → `OFFER`；
2. 接收端 `ACCEPT` → 发送端 `BLOCK_MAP` → 接收端回 `FILE_HAVE`；
3. 发送端计算缺失块并**按序补发**（`FILE_CHUNK.seq` = 块索引）；
4. 接收端按 `seq × block_size` 随机写（`set_len` 预分配），写前校验块哈希；
5. 每 32 块或 5s 原子落盘位图 `<key>.bitmap`（含 CRC）；
6. `FILE_DONE` → 校验位图全满 + 整文件 SHA-256 → 改名或报错（区分"缺块/块哈希不符/整文件不符"）。

### 8.3 恢复与降级
- 重启加载位图；位图缺失/损坏 → 逐块比对 `.part` 重建；
- 对端无 `chunkmap` → 走 v1 字节偏移续传；
- 对端块大小不同 → 丢弃旧块状态（或按新块大小重建）。

### 8.4 验收标准
| # | 场景 | 标准 |
|---|---|---|
| 1 | 中断后重试 | 只重传缺失块，最终整文件哈希通过 |
| 2 | 传输中杀进程 → 重启 | 位图恢复，已收块不重传 |
| 3 | 篡改 `.part` 单块 | 仅该块重传，其余不动 |
| 4 | 10GB 传到 50% 中断后恢复 | 只传剩余字节；内存峰值 <200MB |
| 5 | 对端为 v1 | 自动降级偏移续传，可完成 |
| 6 | 位图缺失/损坏 | 自动重建，可完成或明确提示全量重传 |
| 7 | 校验失败 | 状态 `failed`，原因分类明确，无残留 |
| 8 | 磁盘不足 | 明确报错，任务失败，无半成品正式文件 |

### 8.5 测试策略
- 单测：位图编解码（0 块/非 8 整除/满位图）、块表生成、缺失块计算、块哈希稳定性、`features` 解析；
- 集成：中断后发送块数断言、篡改单块重传、位图持久化恢复、v1 降级、10GB 基准（`#[ignore]`）；
- 故障注入：断在第 1/中间/最后一块；30 次随机中断循环（`#[ignore]`）；
- 兼容矩阵：v2↔v2、v2↔v1、1MB 回退、2MB 协商。

### 8.6 工作量拆解
| 工作项 | 人日 |
|---|---|
| 协议 3 帧 + `features` 协商 + 单测 | 2 |
| 发送端块表与缺失补发 | 2 |
| 接收端预分配/随机写/块校验/位图 | 3 |
| 恢复与降级 | 1.5 |
| 集成与故障注入测试 | 2 |
| 10GB 长稳 + 性能回归 + 文档 | 1.5 |
| **合计** | **8–12** |

### 8.7 风险
| 风险 | 对策 |
|---|---|
| Windows 随机写 + 杀软扫描放大耗时 | 顺序补发；建议排除下载目录并记录数据 |
| 预分配在部分文件系统成稀疏文件 | `set_len` 后顺序落块；异常时回退追加并警告 |
| 位图落盘频繁影响性能 | 每 32 块或 5s 落盘（崩溃最多丢 32 块） |
| 位图/块表被篡改 | 位图 CRC + 整文件 SHA-256 双重校验 |
| 与 `chunk2m` 耦合 | 先完成协商再做块级；1MB 块同样支持（块数翻倍） |

## 9. M9 Android 客户端（18–25 人日，含 3 天 spike）

### 9.1 前置技术验证（spike，3 人日，第 1 周并行启动）
必须先用最小 Demo 回答：
1. 同一 Wi-Fi 下 UDP 广播 + `MulticastLock` 能否稳定发现 PC 客户端？
2. TCP 监听在切后台/锁屏时是否被杀？前台服务能否保活？
3. SAF 保存大文件（1GB）与块级续传组合是否可行？

> 若"广播不可用"，Android 端以 F3 手动 IP 为主路径，产品说明需提前标注。

### 9.2 范围
| 模块 | 改造 |
|---|---|
| 工程 | `tauri android init`、SDK/NDK、`aarch64-linux-android`/`armv7-linux-androideabi` |
| 权限 | `INTERNET`、`ACCESS_NETWORK_STATE`、`CHANGE_WIFI_MULTICAST_STATE`、`POST_NOTIFICATIONS`(13+)、`FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_DATA_SYNC` |
| 发现 | UDP 广播 + `WifiManager.MulticastLock`；AP 隔离时走手动 IP（已有 F3） |
| 接收落盘 | 应用私有目录写 `.part` + `.bitmap` → 校验 → SAF（`ACTION_CREATE_DOCUMENT`）导出最终文件；重名不覆盖 |
| 后台传输 | 前台服务 + 常驻通知（显示进度摘要），锁屏/切后台不中断 |
| 通知 | 渠道创建、13+ 权限请求 |
| UI | 响应式（当前固定 `max-width:720px`）、触控目标 ≥44px、软键盘避让、返回键映射为"返回设备列表" |
| 分享入口 | 可选（系统分享 → Chatters），+2 人日，未做则列为后续 |

### 9.3 验收标准（真机）
1. **Android ↔ Windows**：文字、图片（自动接收+预览）、文件（接受/拒绝）、续传（含块级）、取消全通过；
2. **后台 10 分钟传输不中断**（切后台 + 锁屏），完成通知可见；
3. SAF 保存到用户选择目录成功、重名不覆盖、取消保存无残留；
4. 断网/切 Wi-Fi/进程被杀后界面有明确提示，可手动重试；
5. AP 隔离环境下可用手动 IP 建立连接并完成一次文件传输；
6. 1GB 文件传输记录速率与耗电（5GHz 目标 ≥15MB/s，耗电仅记录）。

### 9.4 测试策略
| 层级 | 方式 |
|---|---|
| 协议/传输 | 复用 Rust 单测与集成测试（host 编译） |
| Android 专属 | Kotlin 单测（SAF Uri 解析、前台服务状态机）；Rust JNI 桥接 smoke |
| 真机矩阵 | Android 11 / 13 / 14 各一台；Wi-Fi 5G 与 2.4G 各一次 |
| 手工清单 | `11-acceptance-checklist.md` + Android 专属 6 项 |

### 9.5 依赖环境
Android Studio + SDK/NDK、2–3 台真机、USB 调试、联网（Gradle）、发布用 keystore（未签名 APK 可先交付，Q13）。

### 9.6 风险（按影响排序）
1. 后台被杀/OEM 限流 → 前台服务 + 电池白名单引导；
2. UDP 广播受限 → 手动 IP 兜底 + 产品说明；
3. SAF 与续传组合复杂 → `.part/.bitmap` 固定私有目录，完成后再导出；
4. 设备碎片化（小米/华为后台策略）；
5. 多架构 NDK 打包体积。

## 10. 验收与测试总表

| 需求 | 里程碑 | 验收摘要 | 测试方式 |
|---|---|---|---|
| 多平台桌面 | M6/M7/M8 | 三平台启动、互传、托盘、自启、打开文件 | CI matrix + 实机清单 |
| Android 客户端 | M9 | 真机互传、后台保活、SAF 保存、AP 隔离兜底 | 真机矩阵（11/13/14）+ 清单 |
| 自动重连（3.4） | M10 | 30s 内自动恢复；失败停止并提示 | 集成测试 + 拔网手测 |
| 设备指纹（F5） | M10 | 两端短码一致且稳定 | 单测 + 组件测试 |
| 性能（3.1/F35） | M10 | loopback ≥100 MiB/s；千兆 ≥50MB/s；10GB 完成 | 基准 + 真实两机 + 长稳 |
| 完整续传（Q8 v2） | M11 | 只补缺失块、跨重启恢复、单块损坏重传、v1 降级 | 单测 + 集成 + 故障注入 + 10GB |

**每平台 DoD**：`fmt`/`clippy`/`test` 全绿 + 安装包可用（可未签名）+ 手工清单全勾 + 已知问题登记到 `01` §3.5。

## 11. 依赖与环境

| 类别 | 内容 | 是否阻塞 |
|---|---|---|
| 联网 | Cargo/npm/Gradle、Tauri 打包器（NSIS/dmg/AppImage 工具） | 是 |
| 硬件 | macOS 机器、Linux 机器、2–3 台 Android 真机、两台千兆 PC（性能复核） | 对应里程碑阻塞 |
| 账号 | Apple Developer（可延后，未签名可交付）、Android keystore、Windows 签名证书（可选） | 不阻塞 |
| CI | GitHub Actions matrix（win/mac/linux） | 建议 |
| 磁盘 | 10GB 测试文件 + 副本（预留 30GB） | M10/M11 验证 |

## 12. 工作量与排期

| 里程碑 | 人日 |
|---|---|
| M6 跨平台地基 + CI | 5 |
| M7 macOS | 4 |
| M8 Linux | 4 |
| M10 自动重连/指纹/性能 | 6–9 |
| M11 完整块级续传 | 8–12 |
| M9 Android（含 spike） | 18–25 |
| **合计** | **45–59** |

```
第 1–2 周：M6 地基；并行启动 M9 spike（3 天）
第 3 周  ：M7 macOS 与 M8 Linux 并行
第 4–5 周：M10（协商 + 2MB 块 + 缓冲写 + 重连 + 指纹）
第 6–7 周：M11（协议 → 发送端 → 接收端 → 恢复/降级 → 故障测试）
第 8–10 周：M9 Android 完整实现
第 11 周 ：各平台安装包（可未签名）、10GB 长稳、验收与文档
```

## 13. 风险登记

| 风险 | 影响 | 对策 |
|---|---|---|
| Android 后台被杀 | 传输中断 | 前台服务 + 白名单引导 + 块级续传兜底 |
| UDP 广播受限/AP 隔离 | 发现失败 | 手动 IP（F3）+ spike 先验证 + 说明 |
| SAF 与续传组合 | 续传失效 | `.part/.bitmap` 私有目录，完成后再导出 |
| macOS 公证依赖账号 | 分发受限 | 未签名 dmg 阶段性交付（Q13） |
| GNOME 无托盘 | 功能缺失 | 降级通知 + 文档 |
| 性能目标受环境影响 | 50MB/s 不达 | 2MB 块 + 缓冲写 + 真实两机复测 |
| 协议块大小变更 | v1 无法接收 | `HELLO.features` 协商，双端一致才启用 |
| 位图持久化崩溃丢失 | 少量重传 | 每 32 块/5s 落盘，上限 32 块损失 |

## 14. 文档同步清单

| 文档 | 同步内容 |
|---|---|
| `01-project-overview.md` | v0.5：Q13–Q15、§3.5 排期调整、§7 v2 路线图 |
| `02-technical-design.md` | §4.2 新增 0x17–0x19；§4.7 块级续传；§4.8 能力协商；§6 位图；§11 跨平台适配；§10 v2 里程碑 |
| `03-doc-index.md` | 登记本文件 |
| `11-acceptance-checklist.md` | 后续补充 v2 验收项（启动 M6 时更新） |
| `README.md` | 状态与文档索引 |

