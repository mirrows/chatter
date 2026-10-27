// Tauri IPC 封装：所有与后端的交互集中在此，便于 mock 测试。
import { invoke } from "@tauri-apps/api/core";
import type { DeviceEntry } from "./types";

/** 获取当前在线设备列表（后端 DeviceTable 快照）。 */
export function listDevices(): Promise<DeviceEntry[]> {
  return invoke<DeviceEntry[]>("list_devices");
}

/** 是否运行在 Tauri 环境（浏览器开发时为 false，便于 mock）。 */
export function isTauri(): boolean {
  return "__TAURI_INTERNALS__" in window;
}
