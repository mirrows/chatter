// Tauri IPC 封装：所有与后端的交互集中在此，便于 mock 测试。
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DeviceEntry,
  DeviceUpdate,
  History,
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

/** 获取本机设备信息（昵称、ID、端口）。 */
export function getLocalDevice(): Promise<LocalDevice> {
  return invoke<LocalDevice>("get_local_device");
}

/** 发送文字消息（F10）；后端会在未连接时自动建连。 */
export function sendText(peerId: string, text: string): Promise<TextSendResult> {
  return invoke<TextSendResult>("send_text", { peerId, text });
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

/** 清空历史（F40/F41）；`peerId` 为空时清空全部。 */
export function clearHistory(peerId?: string): Promise<void> {
  return invoke<void>("clear_history", { peerId: peerId ?? null });
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
