// Tauri IPC 封装：所有与后端的交互集中在此，便于 mock 测试。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DeviceEntry,
  DeviceUpdate,
  ErrorLogInfo,
  History,
  HistoryPage,
  IncomingMessage,
  LocalDevice,
  PathSendResult,
  SaveSettingsResult,
  SessionUpdate,
  Settings,
  TextSendResult,
  TransferInfo,
} from "./types";

/** 设备表变更事件名（与 Rust commands.rs 的 DEVICE_UPDATE_EVENT 一致）。 */
export const DEVICE_UPDATE_EVENT = "device://update";
/** 新消息事件名（与 Rust commands.rs 的 MESSAGE_NEW_EVENT 一致）。 */
export const MESSAGE_NEW_EVENT = "message://new";
/** 会话连接状态事件名（与 Rust commands.rs 的 SESSION_UPDATE_EVENT 一致）。 */
export const SESSION_UPDATE_EVENT = "session://update";
/** 传输任务状态事件名（与 Rust commands.rs 的 TRANSFER_UPDATE_EVENT 一致）。 */
export const TRANSFER_UPDATE_EVENT = "transfer://update";

/** 获取当前在线设备列表（后端 DeviceTable 快照）。 */
export function listDevices(): Promise<DeviceEntry[]> {
  return invoke<DeviceEntry[]>("list_devices");
}

/** 主动触发一次设备重新发现（Android 广播会被过滤，切前台/发送失败时调用）。 */
export function refreshDevices(): Promise<void> {
  return invoke<void>("refresh_devices");
}

/** 获取本机设备信息（昵称、ID、端口）。 */
export function getLocalDevice(): Promise<LocalDevice> {
  return invoke<LocalDevice>("get_local_device");
}

/** 发送文字消息（F10）；后端会在未连接时自动建连。 */
export function sendText(peerId: string, text: string): Promise<TextSendResult> {
  return invoke<TextSendResult>("send_text", { peerId, text });
}

/** 重发一条失败消息（v3）：复用原 msg_id，历史与界面都只保留一条。 */
export function resendText(msgId: string): Promise<TextSendResult> {
  return invoke<TextSendResult>("resend_text", { msgId });
}

/** 订阅设备上下线事件（F4），返回取消订阅函数。 */
export function onDeviceUpdate(handler: (update: DeviceUpdate) => void): Promise<UnlistenFn> {
  return listen<DeviceUpdate>(DEVICE_UPDATE_EVENT, (event) => handler(event.payload));
}

/** 订阅新消息事件，返回取消订阅函数。 */
export function onMessage(handler: (message: IncomingMessage) => void): Promise<UnlistenFn> {
  return listen<IncomingMessage>(MESSAGE_NEW_EVENT, (event) => handler(event.payload));
}

/** 订阅会话连接状态事件，返回取消订阅函数。 */
export function onSessionUpdate(handler: (update: SessionUpdate) => void): Promise<UnlistenFn> {
  return listen<SessionUpdate>(SESSION_UPDATE_EVENT, (event) => handler(event.payload));
}

/** 发送图片或文件（F20/F30）：后端按扩展名分流，图片自动接收。 */
export function sendFile(peerId: string, path: string): Promise<TransferInfo> {
  return invoke<TransferInfo>("send_file", { peerId, path });
}

/** 接受接收中的文件（F34）。 */
export function acceptFile(transferId: string): Promise<void> {
  return invoke<void>("accept_file", { transferId });
}

/** 拒绝接收文件（F34）。 */
export function rejectFile(transferId: string): Promise<void> {
  return invoke<void>("reject_file", { transferId });
}

/** 取消传输（F33）。 */
export function cancelTransfer(transferId: string): Promise<void> {
  return invoke<void>("cancel_transfer", { transferId });
}

/** 重试失败的发送任务（M3-b）。 */
export function retryTransfer(transferId: string): Promise<TransferInfo> {
  return invoke<TransferInfo>("retry_transfer", { transferId });
}

/** 列出全部传输任务（界面重建时使用）。 */
export function listTransfers(): Promise<TransferInfo[]> {
  return invoke<TransferInfo[]>("list_transfers");
}

/** 读取已接收图片的 data URL 预览（F23）。 */
export function getImagePreview(transferId: string): Promise<string> {
  return invoke<string>("get_image_preview", { transferId });
}

/** 打开系统文件选择框；`imagesOnly` 为 true 时只列出图片。 */
export function pickFile(imagesOnly: boolean): Promise<string | null> {
  return invoke<string | null>("pick_file", { imagesOnly });
}

/** 订阅传输任务状态事件，返回取消订阅函数。 */
export function onTransferUpdate(handler: (info: TransferInfo) => void): Promise<UnlistenFn> {
  return listen<TransferInfo>(TRANSFER_UPDATE_EVENT, (event) => handler(event.payload));
}

/** 读取设置（F52）。 */
export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

/** 保存设置（F52）。 */
export function saveSettings(settings: Settings): Promise<SaveSettingsResult> {
  return invoke<SaveSettingsResult>("save_settings", { settings });
}

/** 读取历史（F40）；`peerId` 为空时返回全部。 */
export function getHistory(peerId?: string): Promise<History> {
  return invoke<History>("get_history", { peerId: peerId ?? null });
}

/**
 * 分页读取历史（v3 向上无限滚动）。
 *
 * `beforeTs` 为游标：传当前时间线最老条目的 `ts` 可加载更早的一页；
 * 默认每页 100 条（后端上限 500）。
 */
export function getHistoryPage(
  peerId: string | undefined,
  beforeTs?: number,
  limit = 100,
): Promise<HistoryPage> {
  return invoke<HistoryPage>("get_history_page", {
    peerId: peerId ?? null,
    beforeTs: beforeTs ?? null,
    limit,
  });
}

/** 清空历史（F40/F41）；`peerId` 为空时清空全部，`deleteFiles` 为 true 时删除下载目录内的历史文件。 */
export function clearHistory(peerId?: string, deleteFiles = false): Promise<number> {
  return invoke<number>("clear_history", { peerId: peerId ?? null, deleteFiles });
}

/** 用系统默认程序打开已接收文件（F23）。 */
export function openTransferFile(transferId: string): Promise<void> {
  return invoke<void>("open_transfer_file", { transferId });
}

/** 在文件管理器中定位已接收文件（F23）。 */
export function revealTransferFile(transferId: string): Promise<void> {
  return invoke<void>("reveal_transfer_file", { transferId });
}

/** 另存为（F23）；取消时返回 null。 */
export function saveTransferAs(transferId: string): Promise<string | null> {
  return invoke<string | null>("save_transfer_as", { transferId });
}

/** 选择下载目录（v3，桌面端；Android 返回错误）。取消时返回 null。 */
export function pickDownloadDir(): Promise<string | null> {
  return invoke<string | null>("pick_download_dir");
}

/** 打开下载目录（v3，桌面端；Android 会返回错误）。 */
export function openDownloadDir(): Promise<void> {
  return invoke<void>("open_download_dir");
}

/** 上报一条错误到本地日志（v3）；返回日志位置。 */
export function reportError(scope: string, message: string): Promise<ErrorLogInfo> {
  return invoke<ErrorLogInfo>("report_error", { scope, message });
}

/** 读取错误日志位置与是否已有日志（v3）。 */
export function getErrorLog(): Promise<ErrorLogInfo> {
  return invoke<ErrorLogInfo>("get_error_log");
}

/** 打开错误日志目录（v3，桌面端；Android 会返回错误）。 */
export function openErrorLogDir(): Promise<void> {
  return invoke<void>("open_error_log_dir");
}

/** 是否开启开机自启（F50）。 */
export function getAutostart(): Promise<boolean> {
  return invoke<boolean>("get_autostart");
}

/** 设置开机自启（F50）。 */
export function setAutostart(enabled: boolean): Promise<void> {
  return invoke<void>("set_autostart", { enabled });
}

/** 手动按 IP 添加设备（F3）；后端会立即握手建连。 */
export function addDeviceByIp(addr: string): Promise<DeviceEntry> {
  return invoke<DeviceEntry>("add_device_by_ip", { addr });
}

/** 把剪贴板图片写入临时文件，返回路径（F21）。 */
export function saveClipboardImage(dataBase64: string, extension: string): Promise<string> {
  return invoke<string>("save_clipboard_image", { dataBase64, extension });
}

/** 批量发送本地路径（拖拽，F22/F31）。 */
export function sendPaths(peerId: string, paths: string[]): Promise<PathSendResult[]> {
  return invoke<PathSendResult[]>("send_paths", { peerId, paths });
}

/** 是否运行在 Tauri 环境（浏览器开发时为 false，便于 mock）。 */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
