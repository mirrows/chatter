# Chatters 里程碑报告：M4（设置 / 历史 / 托盘 / 通知 / 自启）

> 版本：v1.0　日期：2026-10-28　对应：`01-project-overview.md` v0.3、`02-technical-design.md`（本次同步修订）

## 1. 交付范围

| 需求 | 实现 |
|---|---|
| F52 设置页 | 新 `views/Settings.vue` + `stores/settings.ts`：昵称、设备 ID（只读）、下载目录、UDP/TCP 端口、主题、自动接收图片/文件、开机自启、清空历史；保存后下载目录与自动接收**即时生效**，昵称/端口提示需重启 |
| F34 接收策略 | `AppConfig` 增加 `auto_accept_images`（默认 true）/ `auto_accept_files`（默认 false）并持久化；`TransferConfig` 改为 `RwLock` 支持运行时更新 |
| F40/F41 历史 | 新增 `storage.rs`：`<config dir>/history.json` 持久化消息与传输记录（原子写入、按对端筛选、清空全部/单对端、最多 2000 条消息 / 500 条传输）；会话页打开时载入历史 |
| F12 复制消息 | 气泡新增「复制」按钮；`utils/clipboard.ts` 优先 `navigator.clipboard`，失败回退 `execCommand`，并提示"已复制" |
| F50 开机自启 | `get_autostart` / `set_autostart` 命令（tauri-plugin-autostart），设置页开关 |
| F51 托盘/通知 | 托盘图标 + 菜单（显示主窗口 / 退出）；窗口不在前台时，收到新消息弹出系统通知（正文不含明文，R5.1） |
| 主题 | 设置页切换 light/dark，立即写入 `data-theme`；CSS 变量三态（系统默认 / light / dark） |

## 2. 数据存储偏差（R6.2）

设计原定 SQLite；离线环境无法获取 `rusqlite`/`sqlx`，v1 使用 **JSON 文件**（`history.json`）实现同等语义，
已在 `02-technical-design.md` §6 记录，后续联网可平滑替换。

## 3. 测试状态（2026-10-28 本地实测）

| 门禁 | 结果 |
|---|---|
| Rust lib 单测 | **51 passed**（新增 storage 5 例） |
| discovery / session / transfer 集成 | 4 / 5 / 9 passed |
| 前端 vitest | **12 files / 75 passed**（新增 clipboard 3、settings store 8、settings view 6、chat 历史 1） |
| `cargo fmt --check` / `cargo clippy -D warnings` | 通过 |
| `vue-tsc --noEmit` / `prettier --check` | 通过 |

storage 用例覆盖：跨实例持久化、同 msgId 去重更新、按对端/全部清空、损坏文件降级为空、超上限裁剪最旧记录。

## 4. 已知限制

1. **端口/昵称需重启**：发现与会话在启动时初始化，保存后仅落盘并提示重启，未做热重启。
2. **无"关闭到托盘"**：关闭窗口即退出；托盘提供显示/退出菜单。
3. **通知内容**：只看"有消息/文件"，不显示具体文本或文件名（隐私优先）。
4. **历史不含图片缓存清理**（F41 的"同时删除图片"）：接收文件位于下载目录，清空历史不删除文件，避免误删用户数据。
5. **未做拖拽/粘贴发送**（F21/F22）与文件夹提示（F31）。

## 5. 下一步（M5 打包）

1. `tauri build` 产出 release 可执行文件与安装包（NSIS 需联网下载打包器，离线环境以 portable exe 交付）；
2. 使用说明与防火墙引导；
3. 打包后冒烟：启动、发现、文字、图片/文件、设置、历史、托盘。
