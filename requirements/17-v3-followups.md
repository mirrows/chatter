# Chatters 后续迭代与优化项（v3 之后）

> 生成：2026-11-14（随 v3 I6.9 验收一并盘点）
> 口径：已完成功能不再重复登记；本文件只记录**未做/待定/已决定不做**的项。
> 入口：`03-doc-index.md` §3；范围基准：`16-v3-plan.md`（v3 已完成）、`14-milestone-m6-m10-report.md` §7。

## 1. 发布与工程阻塞（建议优先）

> 交付顺序建议：**M11 块级续传（§2.2）→ 系统分享入口（§3.3）→ 发布工程（本节）**；
> 发布项需要真实 keystore / CI runner / 联网构建，可并行准备。

| # | 项 | 现状 | 影响 | 备注 |
|---|---|---|---|---|
| 1.1 | Android 正式签名 + 双 ABI | 只有 arm64 debug keystore 包 | **不能上架/分发** | 需真实 keystore；同时产出 `armv7 + arm64` 或 AAB |
| 1.2 | Windows 安装包（NSIS）与代码签名 | 仅绿色 exe；未签名 | 用户会看到 SmartScreen | `npm run tauri build`（联网）产 NSIS；证书到位后签名 |
| 1.3 | macOS / Linux 打包 | 未产出 dmg / deb | 跨平台交付缺 2 个平台 | 需对应系统或 CI runner；macOS 需先生成 `.icns` |
| 1.4 | CI Android 构建 job | `.github/workflows/ci.yml` 只有 frontend + rust（三平台测试） | 移动端问题只能本地暴露 | 需 runner 具备 JDK17 + SDK/NDK + Gradle |
| 1.5 | E2E 测试 | 未做 | 关键链路（双端收发）靠人工回归 | 可先用 Playwright/WebDriver 覆盖前端 + mock IPC |

## 2. 功能与稳定性（P1）

| # | 项 | 说明 | 预估 |
|---|---|---|---|
| 2.1 | ~~设备上下线提示改用 `message` 组件~~ | ✅ **已完成（2026-11-14）**：`devices.applyUpdate` 直接 `message.success/info`，删除 `notice` 状态与设备页内联提示 + 3s 计时器 | — |
| 2.2 | M11 完整块级续传（**已排期**：2026-11-14 用户确认） | 按 `02-technical-design.md` §4.7 实现：`FILE_BLOCK_MAP`/`FILE_HAVE`、`.bitmap` 位图随机写、`chunkmap` 协商与降级；当前只有「记录偏移」的简版续传。拆分见 §2.2.1 | 8–12 人日 |
| 2.3 | Android MulticastLock 前后台管理 | 现在锁随进程存活；切后台不释放，可能影响省电与发现稳定性 | 1 人日 |
| 2.4 | 断线后自动续传 | M10 只做自动重连，断线任务保持 `failed` 需手动重试 | 2–3 人日 |
| 2.5 | `resend(msg_id)` 命令（**建议做，待确认**） | 现状：前端重试复用 `send_text` → 生成**新 msg_id** → 历史新增一条、旧 `failed` 记录残留（重启后出现重复气泡），对端也可能收到两条。改为按原 msg_id 重发可保证历史/界面/对端幂等；改动与收益见 §2.5.1 | 0.5–1 人日 |

### 2.2.1 M11 实施拆分（按 `02` §4.7 既有设计，不需要重新定义协议）

| 阶段 | 内容 | 交付物 | 预估 |
|---|---|---|---|
| S1 协议与位图地基 | `FILE_BLOCK_MAP` / `FILE_HAVE` / 块表编解码（每块 SHA-256 前 8 字节）；`transfer/chunkmap.rs`：位图读写（原子落盘 + CRC32）、`missing_blocks()`、由 `.part` + 块表重建 | 协议编解码单测（含长度上限/非法输入）、位图单测；**暂不声明 `chunkmap`** | 2–3 人日 |
| S2 发送端 | 双方声明 `chunkmap` 时：OFFER 后发 BLOCK_MAP（流式计算块哈希）→ 收 HAVE → 只发缺失块（`seek`）→ DONE 后按结果有限次补发 | `sender.rs` 新分支 + 与 v1 偏移续传并存 | 2–3 人日 |
| S3 接收端 | ACCEPT 后回 HAVE（读位图）；`on_chunk` 改随机写 + 块哈希校验 + 位图更新（每 32 块/5s 落盘）；DONE 校验位图全满 + 整文件 SHA-256；失败分类 `missing_blocks` / `block_hash_mismatch` / `file_hash_mismatch`；清理规则（成功删位图、取消/拒绝/失败删 `.part`+`.bitmap`、断线保留） | `receiver.rs` + 位图集成测试（乱序/重复/损坏块） | 2–3 人日 |
| S4 协商与验收 | `LOCAL_FEATURES` 加入 `chunkmap`；兼容矩阵回归（v2↔v2 / v2↔v1 四种组合）；断线 + 重启续传集成测试；1GB 真机断网续传验收；出包 | 测试 + APK/exe + `16`/`17` 记录 | 2–3 人日 |

> 风险与约束：块大小不一致时丢弃旧块状态；位图损坏时按 `.part` 重建；对端未声明 `chunkmap` 时**必须**回到 v1 偏移续传（现有行为不变）。

### 2.5.1 `resend(msg_id)` 的收益与改动点

**现状（复用 `send_text`）**：前端 `chat.retry()` 调用 `send_text(peer_id, text)`，后端 `SessionHub::send_text` 内部**新生成 msg_id**，`append_message` 按 msg_id upsert → 历史里新增一条；旧 `failed` 记录保留 → 重启后同一句话出现两条（一条失败、一条已送达），且对端可能真的收到两条（首次已送达但 ACK 丢失时）。

**改为 `resend(msg_id)` 后**：
1. 后端按 msg_id 取原记录的 `peer_id`/`text`，**复用原 msg_id** 发送（`TextMessage.msg_id` 不变）→ `append_message` 覆盖同一条记录，状态 `failed → delivered`；
2. 历史只有一条，界面 id/时间戳稳定（不再替换 id、不改 ts），重启前后完全一致；
3. 对端可**按 msg_id 幂等**（接收侧历史已按 msg_id upsert；前端 `receive()` 需补去重），ACK 丢失后的重发不会产生重复消息；
4. 为后续「自动重试/发送队列」提供幂等键。

**改动点**：`session.rs` 增加 `send_text_with_id(peer_id, msg_id, text)`（复用现有 ACK 逻辑）；`commands.rs` 新增 `resend_text(msg_id)`（校验 direction=out、记录存在、更新状态）；前端 `retry` 改为传 msg_id；`chat.receive()` 按 id 去重；单测覆盖「成功/失败/未知 id/非 out 方向/重复重发」。



| # | 项 | 说明 |
|---|---|---|
| 3.1 | 大图预览改 asset 协议 | 目前 base64 IPC，>20MB 无内联预览；改 asset 可省内存与 IPC 开销 |
| 3.2 | 接收文件自动进公共目录/相册 | **已决定不做**（见 §4-1）；保留「另存为」手动导出 |
| 3.3 | 系统分享入口（分享 → Chatters）（**已排期**：2026-11-14 用户确认要做） | 方案：Android 注册 `ACTION_SEND`（`text/plain` / `image/*` / 任意文件）→ 进入 Chatters 选择设备并发送；共享文本直接填充会话输入框。桌面端保持"应用内选择/拖拽" | +2 人日 |
| 3.4 | 定时清理后保留图片/文件气泡 | **已决定不做**（见 §4-2） |
| 3.5 | `.chatters-partial` 残留按龄清理 | 已在 I6.9 定时清理中覆盖；无需重复 |

## 4. 已决定不做 / 保持现状（记录口径，避免反复讨论）

1. **接收图片/文件不自动写入公共相册或 `Download/chatter`**：Android 分区存储下需 MediaStore 直写并重建预览/打开/续传链路，收益有限；保持"私有目录落盘 + 另存为导出"。
2. **定时清理只删文件，不保留图片/文件气泡**：定位是局域网传输工具而非聊天工具，完整性要求一般（2026-11-14 用户确认）。
3. **移动端下载目录固定在 `<app_data>/download/chatter`**：不可修改，设置页只读显示（v3/I6.8）。
4. **弹窗与轻提示分工**：确认用 `confirmDialog`、输入用 `promptDialog`、轻提示统一 `message`（R8.1–R8.5）。

## 5. 待决策：与现有流程/文档不一致的点

| # | 不一致点 | 现状 | 选项 |
|---|---|---|---|
| ① | `11-acceptance-checklist.md` D 节原写「Android 不做」 | Android 已交付但清单未更新 | **已按实修订**（D 节改为"已交付/仍未做"），无需决策 |
| ② | `01` §3.5 写「Q8 块级续传已纳入 v2 M11」 | M11 未开工，v2 已收口 | **已确认要做**：已改为"顺延至后续迭代（§2.2）"，实施拆分见 §2.2.1 |
| ③ | `14` §7 写「M9 Android 未做」 | 已完成并出包 | **已过时**，以 `15`/`16` 为准 |
| ④ | `02` §命令清单含 `resend(msg_id)` | 前端复用 `send_text` | 建议补实现（收益见 §2.5.1）；**待你决定** |
| ⑤ | `13` §279 分享入口"可选，未做则列后续" | 未做 | **已确认要做**（§3.3，+2 人日） |
| ⑥ | 定时清理"删文件 + 删记录"与"对话完整"的预期 | 已按"删记录"实现 | 已在 §4-2 定案，无需再议 |

## 6. 验证类待办（人工）

| # | 项 | 说明 |
|---|---|---|
| 6.1 | 10GB 级长稳与千兆吞吐 | 现仅实测 ≤1GB；F35/3.1 指标未复核 |
| 6.2 | 拒绝通知权限时的传输行为 | `15` §8.4 未单独复测 |
| 6.3 | Android 11 / 13 / 14 多版本矩阵 | 目前只在单台真机验证 |
| 6.4 | macOS / Linux 含冒号文件名回归 | `14` §8 记录的平台差异项 |
