// 自动更新 API（v3）：桌面端使用 Tauri updater（GitHub Releases 的 latest.json），
// 移动端不支持该插件，只提示到 Releases 页面手动下载。
//
// 更新包使用独立的 minisign 密钥签名（与 Windows 代码签名证书无关）：
// 私钥只保存在维护者机器与 CI Secrets（TAURI_SIGNING_PRIVATE_KEY）。
import { invoke } from "@tauri-apps/api/core";
import type { Update } from "@tauri-apps/plugin-updater";
import type { AvailableUpdate, UpdateStatus } from "./types";
import { isTauri } from "./tauri";

/** Releases 页面（Android 手动下载 / 桌面检查失败时的兜底入口）。 */
export const RELEASES_URL = "https://github.com/mirrows/chatter/releases";

/** 待安装的更新句柄（含插件原始对象）。 */
export interface UpdateHandle {
  /** 界面展示用信息。 */
  info: AvailableUpdate;
  /** 插件返回的更新对象（仅桌面端有效）。 */
  raw: Update;
}

/** 下载进度回调（`percent` 0-100，未知总量时为 -1）。 */
export type UpdateProgressHandler = (percent: number) => void;

/** 读取更新状态（当前版本、备份、pending、上次回滚）。 */
export function getUpdateStatus(): Promise<UpdateStatus> {
  return invoke<UpdateStatus>("get_update_status");
}

/**
 * 检查新版本；没有更新或非桌面环境返回 null。
 *
 * 动态 import 保证 Android 打包产物不会在启动时加载原生插件。
 */
export async function checkForUpdate(): Promise<UpdateHandle | null> {
  if (!isTauri()) return null;
  const { check } = await import("@tauri-apps/plugin-updater");
  const update = await check();
  if (!update) return null;
  return {
    info: {
      version: update.version,
      currentVersion: update.currentVersion,
      notes: update.body ?? null,
      date: update.date ?? null,
    },
    raw: update,
  };
}

/**
 * 下载并安装更新（v3）。
 *
 * 调用前必须由 store 调用 `prepareUpdate` 备份当前版本并写 pending；
 * 安装成功后需要 `relaunchApp()` 重启才能运行新版本。
 */
export async function downloadAndInstall(
  handle: UpdateHandle,
  onProgress?: UpdateProgressHandler,
): Promise<void> {
  let downloaded = 0;
  let total = 0;
  await handle.raw.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        total = event.data.contentLength ?? 0;
        onProgress?.(0);
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        onProgress?.(total > 0 ? Math.min(100, (downloaded / total) * 100) : -1);
        break;
      case "Finished":
        onProgress?.(100);
        break;
    }
  });
}

/** 重启应用（安装更新后调用）。 */
export async function relaunchApp(): Promise<void> {
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}

/** 更新安装前：备份当前 exe 并写入 pending（后端）。 */
export function prepareUpdate(from: string, to: string): Promise<void> {
  return invoke<void>("prepare_update", { from, to });
}

/** 下载/安装失败：撤销 pending（保留备份）。 */
export function cancelUpdate(): Promise<void> {
  return invoke<void>("cancel_update");
}

/** 新版本启动后确认健康，避免下次启动被自动回滚。 */
export function confirmUpdateHealthy(): Promise<void> {
  return invoke<void>("confirm_update_healthy");
}

/** 手动回滚到上一版本：后端安排替换脚本，调用方随后退出应用。 */
export function rollbackUpdate(): Promise<void> {
  return invoke<void>("rollback_update");
}

/** 回滚提示已展示：删除回滚记录。 */
export function ackRollback(): Promise<void> {
  return invoke<void>("ack_rollback");
}
