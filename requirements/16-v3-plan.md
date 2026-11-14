# v3 迭代计划：界面重构与通用交互组件

> 版本：v1.0　日期：2026-11-04　对应需求：本轮 8 条真机/使用反馈
> 交付方式：**按 I0 → I6 逐个迭代交付**，每个迭代跑完门禁并出一次安装包，便于逐步验机。

## 0. 范围

| # | 需求 | 归属迭代 |
|---|---|---|
| 1 | PC 与 Android 打包使用 `src-tauri/icons` 中的新图标（当前仍是 Tauri 默认） | I1 |
| 2 | 图片/文件不再单独置顶；与文本一起按发送/接收时间混排，接收在左、发送在右 | I2 |
| 3 | 图片缩略图 120px、contain 居中、不拉伸、不显示文件名；点击预览（通用单例弹窗）；完成后移除「打开文件/打开文件夹」；另存为改到右键/长按菜单；失败时方形左侧显示重试 | I2 + I3 |
| 4 | 文件改为 120px 方形 + 图标 + 底部单行文件名（中间省略、保留后缀）；进度与操作按钮同图片；菜单为「打开文件 / 另存为」 | I2 + I3 |
| 5 | 进度条仅在传输中展示；完成后隐藏「接收 · 已完成」等字样 | I2 |
| 6 | 设置页下载目录增加「打开文件夹」 | I5 |
| 7 | 「清空全部历史」二次确认弹窗（通用、函数式、支持多个）；同时清空下载目录全部文件 | I3 + I5 |
| 8 | 文本隐藏复制按钮，改为右键（PC）/长按（移动）复制 + 1.5s Message 提示 | I3 + I4 |

**明确不做**：群聊、历史导出、传输加密、Android 文件管理器定位（平台不支持，保持隐藏）。

## 1. I0 · 地基（0.5 人日，阻塞全部）

| 项 | 内容 |
|---|---|
| 数据模型 | `TransferInfo` 增加 `ts`（创建时间 Unix 毫秒）；`TransferDto` / `src/api/types.ts` 同步；`lib.rs` 落历史改用 `info.ts` |
| 历史回填 | `stores/transfers.ts::loadHistory` 保留 `ts`，避免重启后排序丢失 |
| 平台判断 | 新增 `src/composables/usePlatform.ts`（`platform/isAndroid/isWindows/isDesktop`），后续组件统一使用 |
| 验收 | 双端测试通过；同一会话的文本与传输条目可按 `ts` 稳定排序 |

> I0 无用户可见变化，**不出安装包**，与 I1 合并出第一版包。

## 2. I1 · 应用图标（0.5 人日）

现状：`src-tauri/icons/android/**` 已是新图标，但 `src-tauri/gen/android/app/src/main/res/mipmap-*/` 仍是 Tauri 默认图标
（因为我们用 Gradle 直编，绕过了 `tauri android build` 的图标复制步骤）；`icon.ico` 也需按新图标重生成。

| 步骤 | 命令/文件 |
|---|---|
| 重新生成全套图标 | `npx tauri icon src-tauri/icons/chatters-512.png` |
| 同步 Android 资源 | `icons/android/**` → `gen/android/app/src/main/res/**`（写进 `tools/build-android-arm64.ps1` 的前置步骤） |
| 保持引用 | `tauri.conf.json::bundle.icon` = `chatters-32/128/512.png` + `icon.ico` |
| 打包后校验 | 脚本比对 `res/mipmap-xxxhdpi/ic_launcher.png` 与 `icons/android/...` 的 SHA-256，不一致即失败 |
| 验收 | Windows 桌面/任务栏图标、Android 启动器图标均为新图标 |

## 3. I2 · 时间线与 120px 气泡（2 人日）

### 3.1 布局
```ts
type TimelineEntry =
  | { kind: "text"; key: string; ts: number; message: ChatMessage }
  | { kind: "transfer"; key: string; ts: number; transfer: TransferInfo };
```
- `Chat.vue` 删除顶部独立的 `.transfers` 区块，改为单一列表 `merge(messages, transfersFor(peer)).sort(by ts, key)`。
- 新增 `components/MessageBubble.vue`（从 Chat.vue 抽出）与 `components/TransferBubble.vue`（替换 `TransferCard.vue`，测试文件同步改名）。
- 接收（in / receive）左对齐，发送（out / send）右对齐，沿用现有 `.bubble.in/.out` 配色。

### 3.2 图片气泡
- 固定 `120×120`，`object-fit: contain`，居中，圆角；**不显示文件名**。
- 点击 → 打开图片预览（I3 的 store）。
- 传输中显示进度条；`done` 后完全隐藏进度与状态文案。
- 无任何常驻按钮（打开/另存为走菜单）。

### 3.3 文件气泡
- `120×120` 方形：居中文件图标（内联 SVG，按类型配色）+ 底部单行文件名。
- 文件名 `utils/filename.ts::middleEllipsis(name, max)`：保留后缀、中间省略，`title` 提供全名。
- 进度/按钮规则同图片。

### 3.4 按钮矩阵

| 方向 | 状态 | 左 | 右 | 说明 |
|---|---|---|---|---|
| 发送 | offered | 「待接收」小标签 | — | 等待对端接受 |
| 发送 | transferring | — | — | 仅进度条 |
| 发送 | failed | **重试** | — | 方形左侧 |
| 接收 | pending | — | **接收 / 取消** | 方形右侧 |
| 接收 | transferring | — | 取消 | 保留现有能力 |
| 接收 | done | — | — | 无按钮（打开/另存为走菜单） |
| 任意 | cancelled/rejected | 小角标 | — | 仅状态提示 |

### 3.5 文案
- 不再显示「接收 · 已完成」；`done` 状态**无任何状态文字**。
- 失败显示原因（`transfer.error`）为小字或角标；不再整块占据卡片高度。

## 4. I3 · 通用浮层四件套（2 人日）

全部 **store 单例 + 宿主组件挂 `App.vue`**，PC/移动端自适应，`z-index` 阶梯：
`预览 1000 > 弹窗 900 > 菜单 800 > Message 1100`。

### 4.1 `useContextMenuStore` + `ContextMenuHost.vue`
```ts
interface ContextMenuAction { key: string; label: string; danger?: boolean; disabled?: boolean; onSelect(): void | Promise<void>; }
openAtPointer(point: { x: number; y: number }, items: ContextMenuAction[]): void
openCentered(items: ContextMenuAction[]): void
close(): void
```
- 单例：同一时刻只展示一个，`open` 覆盖。
- PC：`position: fixed` 在鼠标附近；**边缘检测**（右/下越界则翻转，保留 8px 间距）；点击其他区域/滚动/Esc/失焦自动关闭。
- 移动端：遮罩 + 屏幕正中卡片（`max-width: min(80vw, 320px)`）。
- 长按触发：`src/composables/useLongPress.ts`（500ms、位移 > 8px 取消）。

### 4.2 `useImagePreviewStore` + `ImagePreviewHost.vue`
```ts
open(src: string, alt?: string): void
close(): void
```
- 全屏遮罩 `rgba(0,0,0,.85)`；图片 `max-width: 80vw; max-height: 80vh; object-fit: contain`，居中。
- 关闭：点击遮罩 / 关闭按钮 / Esc。全局单例。

### 4.3 `useConfirmStore` + `ConfirmHost.vue`（参考 Element Plus MessageBox）
```ts
interface ConfirmOptions {
  title: string; message?: string;
  confirmText?: string; cancelText?: string;
  danger?: boolean; maxWidth?: number;   // 默认 420，clamp(280, 92vw, 560)
  onConfirm?(): void | Promise<void>;    // 异步期间按钮 loading，防重复点击
  onCancel?(): void;
}
export function confirmDialog(options: ConfirmOptions): Promise<boolean>;
```
- 多次调用入队，一次展示一个，结束后自动出队（满足「支持多个弹窗」）。
- 两端统一：遮罩 + 正中；移动端宽度 `92vw`。

### 4.4 `useMessageStore` + `MessageHost.vue`（参考 Element Plus Message）
```ts
message({ type: "success" | "error" | "info", text: string, duration = 1500 }): void
```
- 位置：PC 顶部居中；移动端右上贴边。
- `transition-group` 滑入/淡出。
- **同屏最多 3 条**，超出排队：前一条 `duration` 结束消失后立刻显示下一条。

## 5. I4 · 文本复制改造（0.5 人日）
- 移除消息气泡里的「复制」按钮与「已复制」内联提示。
- PC `@contextmenu.prevent`、移动端长按 → 菜单 `[复制]`（复用 I3，移动端 `openCentered`）。
- 复制成功 → `message.success("已复制")`（1.5s）；失败 → `message.error("复制失败")`。

## 6. I5 · 设置页与清空历史（1 人日）

### 6.1 下载目录「打开文件夹」
- 新增命令 `open_download_dir()`：读配置的 `download_dir` → `create_dir_all` → 桌面 `open_command(dir)`；Android 返回明确错误，前端按平台隐藏按钮。
- `Settings.vue` 下载目录右侧加按钮，交互复用现有「打开文件夹」（含失败提示）。

### 6.2 清空全部历史
- **保留** 现有「同时删除已接收文件」勾选框；点击按钮**始终**弹二次确认，文案随勾选变化：
  - 未勾选：`将删除全部聊天记录（下载目录内的文件保留），此操作不可恢复。`
  - 已勾选：`将删除全部聊天记录，并删除下载目录内的全部文件，此操作不可恢复。`
- 后端语义修正（修复「勾选后仍残留文件」）：`clear_history(peer_id=None, delete_files=true)` 时
  **清空下载目录内所有单层普通文件**（含历史里没有的），跳过子目录与符号链接，canonical 前缀校验，目录不存在返回 0。
- 指定 `peer_id` 的会话内清空保持原语义（只删该会话记录里的接收文件）。
- 完成后 `message.success("历史已清空，删除 N 个文件")`。

## 7. I6 · 回归与出包（1 人日）
- 门禁：`npm run test`、`vue-tsc`、`prettier`、`cargo fmt/clippy -D warnings/test`。
- 打包：`tools/build-android-arm64.ps1`（含图标同步校验）、`npx tauri build --no-bundle`。
- 真机逐条复测 8 条需求，结果回填本文档与 `15-m9-spike-plan.md`。

## 8. 测试矩阵（R2.2 / R2.4）

| 层 | 用例 |
|---|---|
| Rust | `TransferInfo.ts` 透出与历史落库使用创建时间；`clear_history` 清空下载目录三档（正常/子目录/目录不存在）；`open_download_dir` 命令构造 |
| 前端 store | `usePlatform`；`useContextMenuStore`（单例覆盖、关闭）；`useImagePreviewStore`；`useConfirmStore`（队列、异步 onConfirm）；`useMessageStore`（≤3 条排队、自动移除） |
| 前端组件 | `TransferBubble`（按钮矩阵 8 种状态、文件名中间省略、图片点击预览）；`MessageBubble`（右键/长按复制、无复制按钮）；四个 Host 的定位/边缘翻转/遮罩关闭 |
| 前端工具 | `middleEllipsis`（无后缀/极短/超长中文）；`useLongPress`（长按触发、移动取消） |

## 9. 风险与对策

| 风险 | 对策 |
|---|---|
| 旧历史缺 `ts` | 排序回退：无 ts 用插入序号；不做数据迁移 |
| 窄屏 120px 方形溢出 | 气泡区 `max-width:100%`，必要时整体横向收缩 |
| 长按与文本选择冲突 | 禁止长按区域文本选择，或长按后 `preventDefault` |
| 误删下载目录 | 只删单层普通文件 + canonical 前缀校验 + 单测 |
| 浮层互相遮挡 | 固定 `z-index` 阶梯（见 §4） |
| 图标再次遗漏 | 复制步骤固化进打包脚本 + 哈希校验失败即中止 |

## 10. 进度记录

### I0 · 地基（已完成，2026-11-04）
- `TransferInfo` 增加 `ts`（Rust/DTO/TS/历史全链路打通），`lib.rs` 落历史改用 `info.ts`。
- 新增 `src/composables/usePlatform.ts` + 单测。
- 门禁：`npm run test` 119、`cargo test`（lib 79 + 集成 22）、`clippy -D warnings`、`fmt`、`vue-tsc`、`prettier` 全绿。
- **未出包**（无可见变化），与 I1 合并。

### I1 · 应用图标（已完成，2026-11-04）
- 用 `npx tauri icon src-tauri/icons/chatters-512.png` 从自绘 logo 重生成全套图标。
- `tauri.conf.json::bundle.icon` 统一为标准集：`icons/32x32.png`、`icons/128x128.png`、`icons/icon.png`、`icons/icon.ico`。
- **关键修复**：`icons/android/**` → `gen/android/app/src/main/res/**` 的复制此前缺失，导致 APK 启动器图标仍是 Tauri 默认；
  现已在 `tools/build-android-arm64.ps1` 增加 `[0/5] sync android launcher icons`，并做 SHA-256 一致性校验（不一致直接失败）。
- 产物与校验：
  - APK `Chatters-1.0.0-rc.1-arm64-debugsigned.apk`：11.05 MB，SHA-256 `679C357D…5E4F3C6A`；
    APK 内自适应图标前景 `res/as.png` 与 `icons/android/mipmap-xxxhdpi/ic_launcher_foreground.png` **哈希一致**（`64954311…`）。
  - exe `chatters.exe`：12.94 MB，SHA-256 `D43C4E58…A3E5A620`；已校验 `icon.ico` 最大图元载荷确实嵌入 exe（偏移 13521944），PE 子系统仍为 GUI。



### I2 · 时间线与 120px 气泡（已完成，2026-11-04）
- **时间线**：`Chat.vue` 移除顶部独立的传输区块，改为 `merge(消息, 传输).sortBy(ts)` 单一列表；
  文本气泡抽到 `components/MessageBubble.vue`，传输气泡替换为 `components/TransferBubble.vue`（原 `TransferCard.vue` 已删除）。
- **气泡规格**：容器 `120×120`；图片 `object-fit: contain` 居中、不显示文件名；文件为图标 + 底部单行名
  （`utils/filename.ts::middleEllipsis` 中间省略并保留后缀）；进度条与百分比**仅 `transferring` 显示**，完成态无任何文案。
- **按钮矩阵**：发送 `offered` → 左侧「待接收」；发送 `failed` → 左侧重试；接收 `pending` → 右侧接收/取消；
  接收 `transferring` → 右侧取消；`done` → 无按钮（打开/另存为走菜单）。
- **配套通用件**（需求 3/4 必需，提前到本迭代）：`stores/contextMenu.ts` + `ContextMenuHost.vue`（PC 鼠标定位 + 边缘翻转、
  移动端遮罩居中、全局单例、Esc/滚动/失焦关闭）、`stores/imagePreview.ts` + `ImagePreviewHost.vue`（80vw/80vh contain 居中）、
  `composables/useLongPress.ts`（500ms、位移容差 8px）、`components/MessageBubble.vue`。
- **门禁**：`npm run test` **141 passed**（新增 filename 6 / long-press 4 / context-menu 9 / image-preview 5 / transfer-bubble 9 /
  时间线混排 1）；`vue-tsc`、`prettier`、`cargo fmt/clippy/test` 全绿。
- **产物**：
  - APK：11.05 MB，SHA-256 `C1AE008C…B5C14980`（`.so` 9.30 MB，内嵌 `index-BKwGJgdW.js`）。
  - exe：12.94 MB，SHA-256 `67CA4FEE…AE95A9D5`（子系统 GUI，内嵌同一份前端资源）。

> 注：重建 exe 前关闭了 10:49 启动的旧实例（旧的 I1 构建），避免文件占用导致链接失败。


### I2 修订（同日，评审反馈）
- **图片菜单收敛**：图片点击即可预览，右键/长按菜单**只保留「另存为」**；文件菜单为「打开文件 + 另存为」；
  「打开所在文件夹」一律不出现（需求 3/4 原文）。
- 新增测试：`chat-view` 断言图片气泡无 `transfer-open`/`transfer-save-as`/`transfer-reveal`，且菜单项为 `["saveAs"]`；
  文件菜单项为 `["open","saveAs"]`。前端测试总数 **143**。
- 重出包：
  - APK 11.05 MB，SHA-256 `36B01D32…77615BA0`（`.so` 9.30 MB，内嵌 `index-DWGMUGBF.js`）。
  - exe 12.94 MB，SHA-256 `002DFC50…6CB6189A`（子系统 GUI）。


### I2.5 · 布局与移动端交互修复（已完成，2026-11-12）

真机/桌面验收提出 6 项体验问题，均在本轮修复。

| # | 问题 | 处理 |
|---|---|---|
| 1 | 气泡贴边，长按时描边被裁 | 时间线左右内边距 8px；气泡关闭系统长按高亮与文本选择（`-webkit-tap-highlight-color` / `-webkit-touch-callout` / `user-select`） |
| 2 | 对话区要铺满且隐藏滚动条，提示改为浮层 | 三段式布局：头部固定 / `.body { flex:1; min-height:0 }` / 输入区固定；时间线 `height:0 + overflow` 并隐藏滚动条；所有错误/提示合并为绝对定位浮层（下滑动画 + 手动关闭，关闭后自动展示下一条） |
| 3 | 输入区固定底部、按钮图标化、标题居中 | `textarea` 在上、操作行在下：左侧图标（图片/文件/清空），右侧发送；移动端占位文案去掉「Ctrl+Enter」；返回改为图标并绝对定位，标题与连接状态两行居中（不受返回按钮宽度影响） |
| 4 | 移动端顶部/底部系统栏适配 | `AndroidManifest` 增加 `windowSoftInputMode="adjustResize"`；`MainActivity` 用 `WindowInsetsCompat` 把系统栏/刘海/IME 高度转成内容 padding；CSS 侧 `.app` 使用 `100dvh` + `env(safe-area-inset-*)` 兜底 |
| 5 | 键盘遮挡输入框 | 同上（insets 取 `max(systemBars.bottom, ime.bottom)`，键盘弹出即把内容顶起） |
| 6 | 双指缩放（全局）与预览缩放 | 全局 `body { touch-action: pan-x pan-y }` + viewport 禁止缩放；图片预览内 `touch-action:none` 并自实现双指缩放（1–4 倍），关闭后复位为 1 倍 |

- **门禁**：`npm run test` **146 passed**（新增 4：平台占位文案、提示浮层关闭切下一条、预览双指缩放与复位）；`vue-tsc`、`prettier`、`cargo fmt/clippy/test` 全绿。
- **产物**：
  - APK 11.05 MB，SHA-256 `B507DD65…9D29699A`（`.so` 9.30 MB，内嵌 `index-Cw0aXlft.js`）。
  - exe 12.94 MB，SHA-256 `EEA39150…6DCCA2E3`（子系统 GUI）。
- **真机待确认**：安全区/键盘行为需在 Android 15（edge-to-edge）与带手势条机型上各验一次。


### I2.6 · 真机反馈三项（已完成，2026-11-12）

| # | 问题 | 处理 |
|---|---|---|
| 1 | 移动端长按会先闪一个「鼠标位置」菜单再变居中 | Android WebView 在长按末会同时派发 `contextmenu`，与我们的 500ms 长按计时器竞争。`TransferBubble` 增加 `touchOnly` prop（Chat 传 `isAndroid`），移动端忽略 `contextmenu`，只走长按 → 只出现居中菜单 |
| 2 | 收发消息后对话区没有停在最下方 | `Chat.vue` 给时间线加 `ref`，以「条目 key 序列」为监听依据：**新增一条内容**（收到/发送的文字、图片、文件——文件在「接受/取消」询问出现时）置底一次；**同一传输的状态推进**（接受 → 传输中 → 100%）不改变 key，因此不再影响用户滚动 |
| 3 | 收到图片/文件有通知，但设备列表徽标没有 +1 | 新增 `chat.noteIncomingTransfer(peerId)`；`transfers.apply()` 判定「新任务 + receive 方向」时计数（同一任务后续状态推进不重复计；正在该会话时不计） |

- **门禁**：`npm run test` **152 passed**（+6：移动端右键不触发菜单、新增置底、传输状态推进不改变滚动、传输未读 3 条）；`vue-tsc`、`prettier` 通过。
- **产物**：APK 11.05 MB `0C134693…6989A61B`；exe 12.94 MB `1507074F…1D98B01`（均内嵌 `index-BdXF0_WN.js`）。


### I2.7 · 向上无限滚动 + 新内容分割线（已完成，2026-11-12）

| 项 | 实现 |
|---|---|
| 分页接口 | 新增命令 `get_history_page(peer_id?, before_ts?, limit?)`：把消息与传输记录合并按时间升序，取「比游标更早」的最新一页；默认 **100 条/页**，上限 500；返回 `{ items, hasMore }`。标签字段用 `itemType`（避开传输记录自带的 `kind`） |
| 前端装配 | `Chat.vue` 首屏拉第一页 → `chat.ingestMessage` / `transfers.ingestHistory`（去重、不计未读）；滚到距顶 32px 内自动加载更早一页 |
| 视口保持 | 加载前记录 `scrollHeight/scrollTop`，插入后 `scrollTop = 新高度 - 旧高度 + 旧 top`，避免内容跳动；期间 `suppressAutoScroll` 禁止自动置底 |
| 顶部提示 | 加载中显示「正在加载更早的消息…」，无更早记录显示「没有更多了」 |
| 滚动策略（最终） | 停在底部：新增内容（收/发的文字、图片、文件）置底一次；**上滑翻历史**：新内容不打断滚动，改为插入「以下为新消息」分割线（**收到的文件除外**——仍需置底以展示接受/取消）；回到底部清除分割线并恢复自动置底；有分割线时继续上滑则清掉旧分割线，重新计算新内容起点 |
| 清空后不复现 | 清空本会话时后端已把该对端记录从 `history.json` 真正删除；前端同时复位分页状态（`hasMoreHistory=false`、清空 `knownKeys`、清除分割线、重置首屏标记），因此不会再去请求更早数据，也不会把已清空的内容翻回来 |

- **门禁**：`cargo test` lib **84**（新增 5 条 `page_history` 单测）+ 集成 22；`npm run test` **156**（新增首屏 100 条分页、滚顶加载与视口保持、分割线三态等）；`fmt/clippy/vue-tsc/prettier` 全绿。
- **未出包**：按用户要求先不打包，等后续改动一起出。


### I3 · 通用浮层剩余两件（已完成，2026-11-13）

| 组件 | 能力 |
|---|---|
| `stores/confirm.ts` + `ConfirmHost.vue` | 函数式 `confirmDialog(options): Promise<boolean>`；支持**多个弹窗排队**（一次展示一个）；props：`title/message/confirmText/cancelText/danger/maxWidth`；events：`onConfirm`（可异步，期间按钮 loading 防重复）/`onCancel`；遮罩 + 正中，宽度夹在 `min(maxWidth, 92vw)`（280–560） |
| `stores/message.ts` + `MessageHost.vue` | `message.success/error/info(text, duration=1500)`；**同屏最多 3 条**，超出排队、到点后依次补位；PC 顶部居中、移动端右上贴边；滑入淡出动画 |

### I4 · 文本复制改造（已完成，2026-11-13）

- 移除气泡内「复制」按钮与「已复制」内联提示；
- PC 右键 / 移动端长按 → 菜单只有「复制」（复用 I2 的菜单组件，移动端居中、PC 跟随鼠标）；
- 复制成功 → `message.success("已复制")`，失败 → `message.error("复制失败")`；长按期间禁用系统文本选择。

### I5 · 设置页打开目录 + 清空历史（已完成，2026-11-13）

| 项 | 实现 |
|---|---|
| 打开下载目录 | 新命令 `open_download_dir()`（目录不存在先创建，桌面端 `open_command` 打开；Android 返回明确错误、前端隐藏按钮）；设置页下载目录右侧新增「打开文件夹」按钮 |
| 二次确认 | 「清空全部历史」始终弹 `confirmDialog`，文案随「同时删除文件」勾选变化（保留/删除下载目录文件），`danger` 红色确认 |
| 清空下载目录 | `clear_history(None, delete_files=true)` 额外调用 `purge_download_dir`：删除下载目录内**所有单层普通文件**（不递归、跳过子目录/符号链接），修复「勾选后仍有残留文件」 |
| 单测 | Rust：`purge_download_dir`（只删文件、保留子目录；目录不存在返回 0）；前端：确认/取消/文案切换/Android 隐藏按钮 |

- **门禁**：`npm run test` **174**（22 文件）；`cargo test` lib **86** + 集成 22；`fmt/clippy/vue-tsc/prettier` 全绿。


### I6 · 回归与出包（已完成，2026-11-13）

- **门禁**：`npm run test` **174 passed（22 文件）**、`vue-tsc` 0、`prettier` 通过；
  `cargo test` lib **86** + 集成 22（discovery 5 / text 5 / reconnect 2 / transfer 10）、`clippy -D warnings` 0、`fmt` 0。
- **产物**：
  - APK：11.12 MB，SHA-256 `EEAA3B25F8425E402ADE6B484DF62BE9BE8A464CDB43C65CFAB4DBF0A17DE8F1`；
    `.so` 9.36 MB，内嵌 `index-FIvlqZkW.js`，并已确认包含新命令（`get_history_page`、`open_download_dir`）。
  - exe：13.09 MB，SHA-256 `9147AB734BA3ABA7F2E6330134B5F7E1DB14ED6E7795204C0E33AE24B160BAB3`；
    PE 子系统 GUI（无控制台），内嵌同一份前端资源。
- **真机验收清单（2026-11-14 Windows + Android 真机复测通过）**：
  - [x] 图标：PC 资源管理器/任务栏、Android 启动器均为新 logo
  - [x] 时间线：文本/图片/文件按时间混排、左右对齐；120px 方形、图片 contain、文件名中间省略保留后缀
  - [x] 气泡：完成后无进度/无「接收·已完成」文案；发送失败左侧重试；接收待确认右侧接受/取消
  - [x] 菜单：PC 右键跟随鼠标（贴边翻转）、移动端长按居中；图片菜单仅「另存为」，文件菜单「打开文件+另存为」
  - [x] 预览：点击图片 80vw/80vh contain 居中；移动端双指缩放 1–4 倍、关闭复位；全局禁止双指缩放
  - [x] 滚动：新增内容置底；上滑时不打断并显示「以下为新消息」（收到的文件除外）；回到底部恢复；滚顶加载更早 100 条且视口不跳
  - [x] 复制：气泡无复制按钮，右键/长按菜单复制并弹 1.5s「已复制」
  - [x] 设置：下载目录「打开文件夹」（仅桌面）；清空全部历史二次确认，勾选后下载目录文件被清空
  - [x] 安全区/键盘：顶部状态栏与底部手势条留白正常，键盘弹出时输入行被顶起

> 迭代中新增的回归项（I6.6 文件打开语义、I6.7 预览滚轮/拖拽与失败态、I6.8 清空历史即时生效、I6.9 移动端定时清理）随同轮验收确认。


### I6.1 · 真机反馈三项修复（2026-11-13）

| # | 问题 | 根因 | 修复 |
|---|---|---|---|
| 1 | 重新进入会话后，已完成的图片预览失败并提示 `image not received yet` | 历史记录里**发送方向**的传输 `path` 恒为 `null`（只有接收方向写落盘路径），预览接口回退历史时判定「没有可用路径」→ 报未接收完成 | `StoredTransfer` 新增 `source_path`（`serde(default)` 兼容旧数据），落历史时对发送任务写入源文件路径；`preview_source` 对历史记录改为「落盘路径 → 源文件路径」回退。旧数据（无该字段）仍会提示，但新发送的图片重进会话可正常预览 |
| 2 | 发送成功后的「已送达」没有右对齐 | `.meta` 默认为左对齐 | `.bubble.out .meta { justify-content: flex-end }` |
| 3 | 移动端文本长按还要再点菜单里的「复制」 | 长按统一弹菜单 | `MessageBubble` 长按时：移动端（`touchOnly`）**直接复制**并弹「已复制」提示；PC 仍走右键菜单 |

- **门禁**：`npm run test` **177**（新增 `message-bubble.spec`：状态展示、右键菜单、移动端长按直复）；`cargo test` lib **88**（新增「历史 source_path 预览」「无任何路径仍报未完成」）；`fmt/clippy/vue-tsc/prettier` 全绿。
- **产物**：APK 11.12 MB `E6F021B3…3C26CC1`；exe 13.11 MB `8B19F70D…C881689`（均内嵌 `index-D9gruElw.js`）。
- **已知限制**：本次修复前已存在的历史发送图片没有 `source_path`，重进会话仍会提示预览不可用（记录里确实没有源路径）；重新发送一次即可。


### I6.2 · 设备列表与设置页改造（2026-11-13）

| # | 需求 | 实现 |
|---|---|---|
| 1 | 下载目录改为「只读 + 点击更换」 | 新命令 `pick_download_dir()`（`plugin-dialog` 的 `pick_folder`，Android 返回错误）；设置页输入框 `readonly`、点击弹目录选择框，选中后**即时保存**；取消不改动原值 |
| 2 | 移动端设备列表改卡片 | `usePlatform().isAndroid` → `.list.is-card`：`grid-template-columns: repeat(auto-fill, minmax(150px, 1fr))` 自动换行；**整张卡片可点**进入聊天（`role=button`，支持回车）；桌面端保持单列行 |
| 3 | 设备名称自定义 + 缓存 + 编辑弹窗 + 顶部布局 | 新增 `stores/peerAliases.ts`（deviceId→别名，localStorage `chatters.peerAliases` 缓存，空名恢复默认）；新增 `stores/prompt.ts` + `PromptHost.vue`（函数式 `promptDialog`，队列、Enter 确认/Esc 取消）；列表项名称旁「✎」按钮 → 弹窗修改；聊天页标题也使用别名。布局：**添加设备行置顶** → 标题「局域网设备（num）」→ 本机昵称 → 列表（名称/地址/指纹）；**本机指纹绝对定位右上角**，只显示编码（title 里给全称） |
| 4 | 已发送文件重进后点开报错 | 发送方向的文件点开不再调用后端（历史里只有源路径、可能已移动），改为 `message.info("该文件已发送，如需重新获取请让对方重新发送", 2600)`；**图片**通过历史 `source_path` 持久化预览（I6.1 已实现） |

- **门禁**：`npm run test` **188**（新增 device-list 别名/卡片/编辑弹窗、prompt store+host、settings 目录选择、transfer-bubble 已发送文件提示）；`cargo test` lib **88** + 集成 22；`fmt/clippy/vue-tsc/prettier` 全绿。
- **产物**：APK 11.13 MB `D5D9EA7B…C49EC08CB`；exe 13.13 MB `E7F4BE2A…E0FCE29`（均内嵌 `index-DPub4BO6.js`，`.so` 含 `pick_download_dir`）。


### I6.3 · 指纹布局修正 + 本机名称可改（2026-11-13）

| # | 需求 | 实现 |
|---|---|---|
| 1 | 本机指纹与「添加设备」行重叠 | 去掉指纹的绝对定位，改为**与「局域网设备（num）」标题同一行、右对齐**（`.head` + `margin-left: auto`）；层级顺序：添加设备行 → 标题/指纹行 → 本机名称 → 列表 |
| 2 | 本机设备名称需可修改 | 本机名称行加「✎」按钮 → `promptDialog` 输入 → `settings.renameLocal()`（未加载时先 `getSettings`，再 `saveSettings`）→ `devices.setLocalName()` 立即刷新本机显示 + 顶部 `message.success` 提示；空名拒绝保存并提示 |

- **口径**：本机改名对**自己实时可见**；对其他设备**不做实时回推**（发现广播在启动时读取昵称），对方在下次连接/本应用重启后看到新名称。修改「其他设备」的名称仍只存本机别名，不通知对方。
- **后端**：`save_settings` 保持「昵称/端口变更 → `restartRequired`」，仅调整提示文案为「昵称本机立即生效（其他设备下次连接可见），端口需重启应用后生效」；`discovery.rs` 接口不变（仍为启动期读取的 `String` 昵称）。
- **门禁**：`npm run test` **194**（新增 device-list 指纹同行/改名成功/空名拒绝 3 例、settings store `renameLocal` 3 例）；`cargo test` lib **88** + 集成 22；`fmt/clippy/vue-tsc/prettier` 全绿。


### I6.4 · 打开文件去黑框 + 移动端大文件提速（2026-11-13）

| # | 问题 | 根因 | 修复 |
|---|---|---|---|
| 1 | PC 右键「打开文件」先闪一个命令行黑框，且整体偏慢 | `platform/shell.rs` 在 Windows 用 `cmd /C start "" <path>`：GUI 进程拉起 `cmd` 必然创建控制台窗口，还得为每个文件多起一个 cmd 进程 | 改为直接调用 `ShellExecuteW`（新增 `windows-sys`，仅 `cfg(windows)` 依赖，features：`Win32_UI_Shell` + `Win32_UI_WindowsAndMessaging`）；新 API `open_path()` 统一入口，macOS 仍 `open`、Linux 仍 `xdg-open`；`open_download_dir` / `open_transfer_file` 的调用放进 `spawn_blocking`，不占异步运行时 |
| 2 | 移动端上传文件「文件越大越慢」，期间界面像卡住 | Kotlin `importFile` 用默认 8KB 缓冲在命令线程把 SAF `content://` 整体复制到缓存，大文件耗时且调用方长时间拿不到结果 | `ChattersAndroidPlugin.kt`：新增 `COPY_BUFFER = 1MB`；新增 `copyInBackground()` 在后台线程复制、完成后 `runOnUiThread` 回传 `resolve/reject`；`importFile`、`saveFileResult`、`fileUri` 兜底复制全部改用它 |
| 3 | 用户不知道「选完文件后还要等复制」 | 复制期间没有任何界面反馈 | `transfers.pickAndSend` 在 Android 上先弹一条常驻提示「正在准备文件…（文件越大耗时越久）」，`finally` 里按 id 移除；桌面端不提示（选完即为真实路径） |

- **词条**：`chat.preparingFile`；`settings.restart` 文案改为「修改已保存：昵称本机立即生效（其他设备下次连接可见），端口需重启应用后生效」。
- **门禁**：`npm run test` **196 passed（24 文件）**（新增 transfer-store 准备提示 2 例、device-list 指纹同行/改名/空名 3 例、settings `renameLocal` 3 例）；`cargo test` lib **89**（新增 `open_missing_path_returns_error`）+ 集成 22（discovery 5 / text 5 / reconnect 2 其中 2 忽略 / transfer 10 其中 0 忽略）；`clippy -D warnings` 0、`fmt` 0、`vue-tsc` 0、`prettier` 通过。
- **产物**：
  - APK：11.13 MB，SHA-256 `49EEBD35DF97252E8375C9F80A53841B85D7DC1196AE268062A2F118FA5C0F43`；`.so` 9.38 MB，内嵌 `index-BOFct123.js`，已确认含 `pick_download_dir`。
  - exe：13.15 MB，SHA-256 `4A9200EB4CECCA39BAE9B2391AE1670A4403C615FDB32B822635CE6B3D69C380`（`release` 构建 7m29s，内嵌同一份 `index-BOFct123.js`）。
- **待真机确认**：右键打开文件不再出现黑框、打开响应更快；移动端选择大文件时出现「正在准备文件」提示，复制速度明显提升、界面不卡。


### I6.5 · 轻提示统一走 Message（2026-11-13）

| # | 需求 | 实现 |
|---|---|---|
| 1 | 移动端「无法修改下载目录」的提示改为 Message 组件 | `Settings.vue` 的 `pickDownloadDir()` 先判断 `isAndroid`：直接 `message.info(t("settings.downloadDirLocked"))` 并返回，**不再调用后端、不写页面级 `error`**；输入框在 Android 上加 `.locked`（`cursor: default`）并把 title 换成「移动端下载目录固定在应用私有目录，无法修改」 |
| 2 | 新规则：轻量级提示统一用 message，1.5s 后消失 | `rules.md` 新增「## 8. 交互与提示规范（v3）」：R8.1 轻提示统一 `message.success/error/info`（`DEFAULT_MESSAGE_DURATION = 1500`，同屏 3 条超出排队）、R8.2 禁止 `window.alert/confirm`（确认用 `confirmDialog`、输入用 `promptDialog`）、R8.3 平台能力缺失用 `message.info`、R8.4 页面级常驻信息仍就地展示、R8.5 文案走 i18n；`stores/message.ts` 头部同步注明该规范 |

- **词条**：`settings.downloadDirLocked`（「移动端下载目录固定在应用私有目录，无法修改」）。
- **门禁**：`npm run test` **198 passed（24 文件）**（新增 settings-view 2 例：Android 轻提示且不调后端 / 桌面端仍弹目录框且不出提示）；`cargo test` lib 89 + 集成 22；`clippy -D warnings` 0、`fmt` 0、`vue-tsc` 0、`prettier` 通过（`prettier --check .` 仍会报 `README.md` 与 `tauri*.conf.json`，属历史未格式化文件，本次未改动）。
- **产物**（内嵌 `index-MRJhmv9z.js` / `index-DutiSACO.css`）：
  - APK：11.13 MB，SHA-256 `34FF7CE2DEE8BB97A4B4E9E77ACCBFC305408620435A1A1CB8B9E68B15941590`。
  - exe：13.15 MB，SHA-256 `668266954B7E6671F1D040FEBDFD0450E7C0912A47F3AFA6CF7EA49C4194630D`（release 4m51s）。



### I6.6 · 文件打开语义修正 + 预览缩放拖拽 + 改名提示（2026-11-13）

| # | 问题 | 实现 |
|---|---|---|
| 1.1 / 1.2 | 自己发出的文件点开只提示「请对方重发」，无法找回文件 | 后端新增 `pick_local_path()` + `transfer_local_path()`：**接收方向**用落盘路径，**发送方向**先用内存任务源路径、再退回历史 `source_path`；任务不在内存（应用重启后从历史加载）时按历史方向取值。`open_transfer_file` / `reveal_transfer_file` / `save_transfer_as` 统一走它。气泡点击（发送/接收的 done 文件）改为 emit `open` → `transfers.openFile()`；右键/长按菜单对两个方向都可用（文件：打开文件 + 另存为；图片：另存为）。文件不在时按方向提示：**发送** →「源文件已被移动或删除」，**接收** →「文件已被清理，如需重新获取请让对方重新发送」 |
| 2.1 / 2.2 | 预览不支持鼠标滚轮缩放、不能拖拽 | `ImagePreviewHost.vue` 重写交互：PC **滚轮以光标为锚点缩放**（1–4 倍）、按住拖拽平移；移动端双指缩放 + **单指拖拽**；位移按图片尺寸夹取、缩回 1 倍归零；拖拽后松手不误触遮罩关闭；放大时显示 grab/grabbing 光标 |
| 3 | 设置页出现长串重启文案 | `save_settings` 的 `restartRequired` 改为**只由端口变更触发**（昵称本机立即生效，其他设备下次连接自然可见）；`settings.restart` 文案改为「端口已修改，重启应用后生效」；设备列表改「其他设备」名称后 `message.success("设备名称已更新")`（1.5s，R8.1） |

- **词条**：新增 `transfer.sourceMissing` / `transfer.receivedMissing` / `transfer.openFailed` / `transfer.saveFailed` / `device.aliasSaved`；移除 `transfer.sentFileHint`。
- **门禁**：`npm run test` **206 passed（24 文件）**（bubble 打开 3 例、预览滚轮/拖拽 3 例、store `openFile` 提示 3 例、device 别名提示）；`cargo test` lib **93**（新增 `pick_local_path` 4 例）+ 集成 22（discovery 5 / text 5 / reconnect 2（2 ignored）/ transfer 10）；`clippy -D warnings` 0、`fmt` 0、`vue-tsc` 0、`prettier` 通过。
- **产物**（内嵌 `index-Ijln4N4T.js` / `index-DhEf0nzg.css`）：
  - APK：11.16 MB，SHA-256 `C72802E470266EF7A55F194E7C7A87344917F12274173BABDFFE4210801A9DF3`（`.so` 9.41 MB）。
  - exe：13.18 MB，SHA-256 `3ED6F24F09C9C7E6FF7CAA128B04580011F538F2AB8A3925CDE987B90B7DEEA4`。



### I6.7 · 失败态交互 + 错误日志 + 通用提示条（2026-11-14）

| # | 需求 | 实现 |
|---|---|---|
| 1 | 已拒绝/失败的文件、图片不可点击（发送方可点，移动端重启后点击无反应） | 根因：`onBoxClick` 只对 `state === "done"` 生效，失败项点击**完全没有反馈**。新增 `isClickable`：done → 双向可点；failed/rejected/cancelled → **只有发送方可点**（打开源文件，找不到时提示「源文件已被移动或删除」），接收方加 `.static` 去掉 hover 且不响应；进行中的图片仅在已有预览数据时可点 |
| 2 | 失败任务重启后隐藏重试 | transfer store 新增 `sessionIds`（`upsert` 标记，历史 `ingestHistory` 不标记）+ getter `isSessionTask`；`TransferBubble` 新增 `hideRetry`（用反向语义规避 Vue 把缺省布尔 prop 判为 false 的坑），Chat 传 `!isSessionTask(id)` |
| 3 | 清空会话记录二次确认 + message 提示 | `clearConversation()` 先 `confirmDialog`（危险样式），确认后清空并 `message.success("本会话记录已清空")`；失败走统一兜底并写入日志 |
| 4 | 未知报错统一兜底 + 错误日志 + 设置页入口 | 新增 `src-tauri/src/errorlog.rs`：`<app_data>/logs/YYYY-MM-DD/errors.log`，启动时清理超过 7 天的日期目录（只处理可解析为日期的目录），`has_logs()` 判断目录内是否有非空日志；新增命令 `report_error` / `get_error_log` / `open_error_log_dir`（桌面打开文件夹，Android 返回明确错误、前端改为展示路径）；前端新增 `utils/errors.ts`（`friendlyError`：中文后端文案透传，其余统一「操作失败，请稍后重试」；`handleError` 上报原始信息并返回友好文案）与 `stores/errorLog.ts`；设置页在 `hasLogs` 时显示「查看错误日志」（桌面按钮打开文件夹 / Android 只读文本框显示目录），重启后按目录内容决定显隐 |
| 5 | 对话区提示效果抽成通用组件，清空会话不再使用 | 新增 `components/NoticeBar.vue`（`text`/`testid`/`closable`，内置下滑动画与关闭按钮；`withDefaults` 保证 `closable` 默认 true），Chat 改用它渲染错误/传输/粘贴/文件夹提示；清空会话改用 confirm + message，不再占用该浮层 |

- **词条**：新增 `error.unknown` / `error.logHint` / `error.logOpen` / `error.logOpenFailed`、`chat.clearHistoryConfirm`。
- **门禁**：`npm run test` **221 passed（26 文件）**（新增 errors util 5 例、NoticeBar 4 例、失败态点击/hideRetry 2 例、`isSessionTask` 1 例、错误日志入口 3 例）；`cargo test` lib **97**（新增 errorlog 4 例：日期互逆、非法目录名、写日志、清理过期）+ 集成 22；`clippy -D warnings` 0、`fmt` 0、`vue-tsc` 0、`prettier` 通过。
- **产物**（内嵌 `index-zwgRPt9I.js` / `index-j46guuss.css`）：
  - APK：11.20 MB，SHA-256 `958A15970C906BD65CFE7D55BE36A7EDC74481E6FFF28FD590A8DB339A76BF92`。
  - exe：13.28 MB，SHA-256 `D8556343C313811385780915A663896DB0C3DC3E77117BF98E327AF8BC128847`。



### I6.8 · 清空历史即时生效 + Android 下载目录固定（2026-11-14）

| # | 问题 | 实现 |
|---|---|---|
| 1 | 设置页「历史已清空」改为 message | `settings.clearAllHistory()` 成功后 `message.success(...)`（有删除文件时带数量），移除设置页 `settings-history-cleared` 常驻段落与 `notice = "historyCleared"` |
| 2 | 移动端「同时删除已接收文件」不生效；下载目录统一固定 | 根因：Android 的配置下载目录是**相对路径** `downloads`，`delete_files_under()` 里的 `canonicalize(base)` 失败 → 一律不删。新增 `AppState.receive_dir`（Android = `<app_data>/download/chatter`，桌面 = 配置值），`clear_history` 以它为基准；`get_settings` 在 Android 回报实际目录；`save_settings` 在 Android 不再把接收目录改成客户端传值；`platform::paths::android_download_dir()` 抽出并加单测 |
| 3 | 清空全部历史后回对话页仍有记录，需重启 | 根因：后端已清空，前端 `chat`/`transfers` store 仍缓存旧消息与任务。新增 `chat.resetAll()` / `transfers.resetAll()`（**保留事件订阅**，与测试用 `clear()` 区分），清空成功后同步调用 |

- **门禁**：`npm run test` **221 passed（26 文件）**（改写 settings-store 清空历史用例：断言 message 文案 + 本地缓存清空；settings-view 两处改为断言 message）；`cargo test` lib **98**（新增 `android_download_dir_is_fixed`）+ 集成 22；`clippy -D warnings` 0（`AppState::new` 参数超限处加注释豁免）、`fmt` 0、`vue-tsc` 0、`prettier` 通过。
- **产物**（内嵌 `index-CPHiYFp1.js` / `index-Cj0iekW1.css`）：
  - APK：11.20 MB，SHA-256 `4F863FBFB4E10C7ED9FD5003A15F318229775869014049F9A273076151D347C5`。
  - exe：13.28 MB，SHA-256 `A8CE8113DF3E523CADFE24483305374D745534312B4218668591CCD5F4F2097A`。



### I6.9 · 移动端定时清理（2026-11-14）

| # | 需求 | 实现 |
|---|---|---|
| 1 | 设置页新增「定时清理」：勾选框 + 天数输入（默认 7、纯数字、>0） | `AppConfig` 新增 `auto_clean_enabled`（**默认关闭**，旧配置由 serde 默认补齐）与 `auto_clean_days`（默认 7）；`SettingsDto`/前端 `Settings` 同步；设置页**仅 Android** 渲染该区块，未勾选时天数输入禁用；输入即时净化（小数向下取整、0/空回退 7、上限 3650），保存时前端 `normalizeCleanDays` + 后端 `validate_clean_days`（0 → 报错）双保险 |
| 2 | 第二行提示语 | `settings.autoCleanHint`：「超过保留天数的接收文件会在每次打开应用时自动删除。请及时用「另存为」备份重要文件，否则这些文件会持续占用手机存储空间。」 |
| 3 | 勾选后每次启动清理超期数据 | 新增 `src-tauri/src/autoclean.rs`：按文件 mtime 清理接收目录第一层文件 + `.chatters-partial/` 过期残留（目录本身保留）；`lib.rs` 启动时开关开启则异步清理，并用删除路径集合调用 `HistoryStore::remove_transfers()` 同步移除指向已删文件的接收记录，避免留下"点开必失败"的条目 |

- **门禁**：`npm run test` **226 passed（26 文件）**（新增 settings-view 3 例：桌面隐藏、移动端渲染与保存、天数规范化；settings-store 2 例：保存规范化 + `normalizeCleanDays`）；`cargo test` lib **107**（autoclean 4、storage `remove_transfers` 2、config 旧配置默认值 1、`validate_clean_days` 2）+ 集成 22；`clippy -D warnings`/`fmt`/`vue-tsc`/`prettier` 全绿；`cargo check --target aarch64-linux-android` 通过。
- **产物**（内嵌 `index-FZIPQD0X.js` / `index-iQM8bW87.css`）：
  - APK：11.27 MB，SHA-256 `50AE935C05047FE73D28029D556E3B13B3D5E9E8FA549778ADB648782477ED60`。
  - exe：13.31 MB，SHA-256 `38F83791817F422A7D84DC2BC4668BFE74DF36E581B949B4267117B8BB9B8CA8`。



### I6.10 · 验收归档与工程门禁补齐（2026-11-14）

| # | 事项 | 结果 |
|---|---|---|
| 1 | 真机验收归档 | Windows 10/11 + Android arm64 通过：勾选 I6 的 9 项验收清单（本文 §I6）、`15` §8.4（4/6，剩余两项列入 `17`）、`15` §9.4（2/3）、`11` B 节 24 项；`11` A 节门禁数字更新为 lib 107 + 集成 22 / 前端 26 文件 226 用例 |
| 2 | ESLint 补齐（R3.1 缺口） | 新增 `eslint.config.js`（ESLint 9 扁平配置：`typescript-eslint` + `eslint-plugin-vue` essential，忽略构建产物）；`npm run lint` **0 error**；CI `frontend` job 增加 `npm run lint`；修复 `tests/utils/long-press.spec.ts` 的 `any`（改用 `LongPressHandlers` 与 `TouchEvent` 断言） |
| 3 | 格式统一 | Prettier 统一 `README.md` 与 3 个 `tauri*.conf.json`，`npx prettier --check .` 全绿 |
| 4 | 文档盘点 | 新增 `17-v3-followups.md`（**唯一待办入口**：发布阻塞、功能、优化、已决不做、待决策的不一致点）；更新 `01` §3.5、`02` §8、`03` 索引、`README`（数据位置/门禁/Android 出包）、`11`、`15` |
| 5 | 设备上下线提示改 `message`（R8.1） | `devices.applyUpdate` 直接 `message.success/info`；删除 `notice` 状态、`clearNotice`、设备页内联提示与 3 秒计时器（文案仍走 i18n，保留「已上线/已下线」） |

- **门禁**：`npm run test` 226、`npm run lint` 0、`vue-tsc` 0、`prettier --check .` 全绿；`cargo test` lib **107** + 集成 **22**、`clippy -D warnings` 0、`fmt` 0。
- **提交**：`4cc80be`（后端）、`dcc859b`（前端）、`a1579fc`（lint）、`f5d53fe`（格式）、`553ae60`（文档）、`e4faad6`（I6.10 记录）、`d255ba0`（设备提示改 message），已推送 `origin/main`。
- **产物（最终包，内嵌 `index-Dh8BPHY7.js` / `index-Cq4M2HbF.css`）**：
  - APK：11.27 MB，SHA-256 `C72C07A6E8D3085B9894CCA93AFDD44169025356ABB36520E1824A61A6E88E05`。
  - exe：13.31 MB，SHA-256 `4B550AB2E8AD15B1E298C72838A14F6FFDE8836DA1EAE4F0C40CC9ACE8D4BA0A`。


### I6.11 · 文字消息重发幂等（2026-11-14）

| # | 项 | 实现 |
|---|---|---|
| 1 | 重发产生重复历史/重复气泡 | 根因：`chat.retry()` 复用 `send_text` → 后端生成**新 msg_id** → `append_message` 新增一条，旧 `failed` 记录残留；重启后同一句话出现两条，且首次已送达但 ACK 丢失时对端会真的收到两条 |
| 2 | 后端 | `SessionHub::send_text_with_id(peer_id, msg_id, text)`（`send_text` 改为生成 id 后委托它）；新命令 `resend_text(msg_id)`：查历史 → 校验 `direction == "out"` 且文本非空 → 复用原 msg_id 发送 → 同 msg_id 覆盖历史（`ts` 沿用原值）；`HistoryStore::find_message()` |
| 3 | 前端 | `chat.retry()` 改调 `resendText(id)`，不再替换界面 id / 时间戳；`chat.receive()` 按 msg_id 去重（对端重发时只更新状态） |

- **门禁**：`npm run test` **228 passed（26 文件）**（新增 retry 幂等 / 重复消息去重 2 例，改写原 retry 用例）；`cargo test` lib **110**（新增 `find_message`、`resend_target` 正常/异常 3 例）+ 集成 22；`clippy`/`fmt`/`lint`/`vue-tsc`/`prettier` 全绿。
- **兼容性**：TEXT 帧本就携带 `msg_id`，协议 `ver` 不变；旧对端收到同 msg_id 的重复 TEXT 时按既有 `append_message` upsert 处理（不会重复落库）。


### I6.12 · M11 S1：块级续传地基（2026-11-14）

| # | 项 | 实现 |
|---|---|---|
| 1 | 协议帧 | `protocol.rs` 新增 `FILE_BLOCK_MAP(0x17)` / `FILE_HAVE(0x18)` / `FILE_RESUME_STATE(0x19)` 与 `FileBlockMap` / `FileHave` / `FileResumeState` payload（snake_case，与既有帧一致）；`FileBlockMap::block_count/hash_at` |
| 2 | 块表与位图 | 新增 `transfer/chunkmap.rs`：`ChunkMap`（流式 `from_reader`、`to_frame/from_frame`、块大小与哈希长度校验）、`BlockBitmap`（置位/统计/缺失/hex 互转/尾部填充位清零）、`crc32`（编译期查表，无新依赖） |
| 3 | 位图落盘 | `.bitmap` 格式 `magic "CBM1" + 块数 + 长度 + 位图 + crc32`，`save_bitmap` 走「临时文件 + 改名」，`load_bitmap` 校验 magic/块数/长度/CRC（失败返回分类错误，供 S3 回退重建） |
| 4 | 兼容性 | **不声明 `chunkmap`**：现有传输仍走 v1 偏移续传；`transfer::handle_frame` 对三个新帧先忽略，避免未启用时影响旧流程 |

- **门禁**：`cargo test` lib **119**（+9：帧类型往返、块表 hash_at/JSON、CRC32 参考值、位图置位/缺失/hex/填充位、位图落盘-加载/损坏分类、块表流式构建与帧互转、非法输入）+ 集成 22；`clippy -D warnings` 0、`fmt` 0。
- **下一步**：S2 发送端（BLOCK_MAP → HAVE → 只发缺失块）→ S3 接收端（随机写 + 位图 + 完整性判定）→ S4 协商与真机验收（见 `17-v3-followups.md` §2.2.1）。


### I6.13 · 系统分享入口（Android ACTION_SEND，2026-11-14）

| # | 项 | 实现 |
|---|---|---|
| 1 | 接收分享 | `AndroidManifest.xml` 为 `MainActivity` 增加 `ACTION_SEND` + `*/*` filter；`MainActivity` 在 `onCreate`/`onNewIntent`（`singleTask`）解析 `EXTRA_TEXT` 或 `EXTRA_STREAM`，写入新增的 `ShareInbox` 单例（只保留最近一次） |
| 2 | 取用与导入 | 插件命令 `takeShare`（无分享返回空对象）→ Rust `take_share_payload()`（消费型，取走即清空、校验 kind/内容）；`import_shared_file(uri)` 复用 Kotlin `importFile` 把 `content://` 复制到 `cacheDir/imports/` |
| 3 | 前端交互 | 新增 `stores/shareInbox`（`pending` / `summary` / `importFile`）；App 在启动与回到前台时 `refresh()`；设备页显示「已收到分享：{summary}，点选设备即可发送」+「取消」；点设备后 **文本走 `chat.send`**、**文件先 import 再 `transfers.sendPath`**，结果用 `message` 提示并进入该会话 |
| 4 | 平台范围 | 仅 Android；桌面端 `take_share_payload` 恒为 `null`，界面无变化 |

- **限制**：只支持单个 `ACTION_SEND`（`SEND_MULTIPLE` 暂不支持）；文件在「选中设备」后才复制到缓存（大文件期间会等待，与现有选择器一致）。
- **门禁**：`npm run test` **234 passed（27 文件）**（新增 share store 5 例、设备页分享提示 1 例）；`cargo test` lib **121**（新增 `share::validate` 2 例）+ 集成 22；`clippy`/`fmt`/`lint`/`vue-tsc`/`prettier` 全绿；`cargo check --target aarch64-linux-android` 通过。
- **产物**（内嵌 `index-PaQBxUHd.js` / `index-B68X56oA.css`）：
  - APK：11.34 MB，SHA-256 `38667C1B1DC40BD4BE45477D917FC13D965CA378624AAADF21AAB4AE17237D7E`。
  - exe：13.40 MB，SHA-256 `DF2318394BC296119A6DA50106275CDEBA8C98CF969B0FDD8DA997FD7E2F240F`。



### I6.14 · M11 S2–S4：块级续传落地（2026-11-14）

| # | 项 | 实现 |
|---|---|---|
| 1 | 发送端（S2） | `send_file` 在双方声明 `chunkmap` 时用 `hash_file_with_blocks` 一次读取同时算整文件 sha256 与块表；`sender::run` 收到 ACCEPT 后发 `FILE_BLOCK_MAP`、等 `FILE_HAVE`（10s 超时按全缺失），按位图缺失索引 `seek` 读取发送（seq = 块索引）；`FileResult.error` 分类透传到失败原因 |
| 2 | 接收端（S3） | `accept` 回 ACCEPT（`resume_offset=0`）；`on_block_map` 校验块大小/块数 → 预分配 `.part` → 加载/新建 `.bitmap` → 回 `FILE_HAVE`；`on_chunk_map` 校验块短哈希 + 随机写（`std::fs::File` + `Mutex`）+ 位图每 32 块原子落盘；`on_done_map` 校验位图全满 + 重新计算整文件 sha256 → rename / `missing_blocks` / `file_hash_mismatch` |
| 3 | 降级兼容（S4） | `LOCAL_FEATURES` 加 `chunkmap`（双方声明才启用）；未声明 → 原顺序写 + v1 偏移续传；声明但块表未到（异常/旧端）→ `on_chunk` 懒创建顺序句柄复用旧逻辑 |
| 4 | 清理规则 | 成功：删 `.bitmap` + 续传元数据；取消/拒绝：删 `.part` + `.bitmap` + 元数据；校验失败/断线：保留（重试时按位图只补缺失块） |
| 5 | 测试 | 新增集成：`chunkmap_transfers_multi_block_file`（2.5MB 多块）、`legacy_peer_falls_back_without_chunkmap`（空 features 模拟旧端）；协议 `FileResult.error` 断言更新 |

- **门禁**：`cargo test` lib **121** + 集成 **24**（file_transfer 12 / discovery 5 / text 5 / reconnect 2）；`clippy -D warnings` 0、`fmt` 0；前端未改（`npm run test` 234 基线不变）。
- **行为说明**：
  - 接收端回 `missing_blocks:N` 时发送端标记失败并展示分类；用户「重试」会重新 OFFER，接收端加载位图后只补缺失块（自动有限次补发暂未实现）。
  - 位图损坏/缺失时从零开始（尚未实现按 `.part` 重建）；`.part` 已预分配为 size，重试不受文件长度影响。
- **待办**：1GB 真机断网续传验收、断线重连后的位图续传验证、最终出包（用户要求最后统一打包）。

