// 自动更新状态（v3）：检查新版本、下载安装、启动健康确认与回滚。
//
// 流程：
// 1. 启动时 `init` 读取状态：有 `lastRollback` 就提示一次，有 `pending` 就延迟确认健康；
// 2. 桌面端在窗口稳定后静默检查更新，发现新版本提示一次；
// 3. 用户在设置页点「下载并安装」→ `prepareUpdate`（备份 + pending）→ 下载安装 → 重启；
// 4. 安装失败调用 `cancelUpdate` 撤销 pending；成功但新版本启动不健康时后端自动回滚。
import { defineStore } from "pinia";
import type { AvailableUpdate, UpdateStatus } from "../api/types";
import {
  ackRollback,
  cancelUpdate,
  checkForUpdate,
  confirmUpdateHealthy,
  downloadAndInstall,
  getUpdateStatus,
  prepareUpdate,
  relaunchApp,
  rollbackUpdate,
  type UpdateHandle,
} from "../api/update";
import { isTauri } from "../api/tauri";
import { t } from "../i18n";
import { handleError } from "../utils/errors";
import { message } from "./message";

/** 更新流程阶段。 */
export type UpdatePhase =
  | "idle"
  | "checking"
  | "latest"
  | "available"
  | "downloading"
  | "installing"
  | "error"
  | "unsupported";

/** 启动后多久确认新版本健康（毫秒）。 */
export const HEALTHY_CONFIRM_DELAY = 8000;
/** 启动后多久静默检查更新（毫秒）。 */
export const AUTO_CHECK_DELAY = 5000;

/**
 * 当前待安装的更新句柄。
 *
 * 放在模块级而不是 store state：`Update` 是插件实例，放进响应式 state 会被代理，
 * 其方法内部的 `this` 绑定可能失效。
 */
let pendingHandle: UpdateHandle | null = null;

/** 更新 store。 */
export const useUpdaterStore = defineStore("updater", {
  state: () => ({
    /** 后端更新状态（版本、备份、pending、回滚记录）。 */
    status: null as UpdateStatus | null,
    /** 当前阶段。 */
    phase: "idle" as UpdatePhase,
    /** 检查到的新版本信息。 */
    available: null as AvailableUpdate | null,
    /** 下载进度（0-100；-1 表示未知总量）。 */
    progress: 0,
    /** 最近一次错误（界面展示）。 */
    error: "",
    /** 是否已执行过启动初始化。 */
    initialized: false,
  }),
  getters: {
    /** 当前版本（后端未就绪时为空串）。 */
    currentVersion(state): string {
      return state.status?.currentVersion ?? "";
    },
    /** 是否桌面端（仅桌面支持自动更新）。 */
    isDesktop(state): boolean {
      return state.status !== null && state.status.platform !== "android";
    },
    /** 是否有可回滚的旧版本备份。 */
    canRollback(state): boolean {
      return Boolean(state.status?.backupAvailable);
    },
    /** 是否正在执行更新流程（禁用重复点击）。 */
    busy(state): boolean {
      return (
        state.phase === "checking" || state.phase === "downloading" || state.phase === "installing"
      );
    },
  },
  actions: {
    /**
     * 启动初始化：读取状态、提示回滚、确认健康、延迟静默检查。
     *
     * 只在 Tauri 环境执行（浏览器开发时无后端）。
     */
    async init() {
      if (!isTauri() || this.initialized) return;
      this.initialized = true;
      try {
        this.status = await getUpdateStatus();
        // 上次自动回滚过：提示一次并清除记录。
        const rollback = this.status.lastRollback;
        if (rollback) {
          message.info(t("update.rolledBack", { from: rollback.from, to: rollback.to }), 4000);
          await ackRollback();
          this.status.lastRollback = null;
        }
        if (this.status.platform === "android") {
          this.phase = "unsupported";
          return;
        }
        // 更新后首次启动：窗口稳定后确认健康，避免下次启动被自动回滚。
        if (this.status.pending) {
          setTimeout(() => void this.confirmHealthy(), HEALTHY_CONFIRM_DELAY);
        }
        setTimeout(() => void this.autoCheck(), AUTO_CHECK_DELAY);
      } catch (err) {
        this.error = handleError("update.init", err);
      }
    },
    /** 确认新版本健康（清除 pending）。 */
    async confirmHealthy() {
      try {
        await confirmUpdateHealthy();
        if (this.status) this.status.pending = null;
      } catch (err) {
        this.error = handleError("update.confirmHealthy", err);
      }
    },
    /** 静默检查：发现新版本时给一条轻提示。 */
    async autoCheck() {
      if (this.phase !== "idle" || this.status?.platform === "android") return;
      const handle = await this.check(true);
      if (handle) {
        message.info(t("update.availableToast", { version: handle.info.version }), 4000);
      }
    },
    /**
     * 检查更新。
     *
     * `silent` 为 true 时不弹「已是最新」提示（启动自动检查用）。
     * 返回句柄便于调用方决定是否立即安装；没有更新返回 null。
     */
    async check(silent = false): Promise<UpdateHandle | null> {
      if (this.status?.platform === "android") {
        this.phase = "unsupported";
        return null;
      }
      if (this.busy) return null;
      this.phase = "checking";
      this.error = "";
      try {
        const handle = await checkForUpdate();
        if (!handle) {
          pendingHandle = null;
          this.available = null;
          this.phase = "latest";
          if (!silent) message.success(t("update.latest"));
          return null;
        }
        pendingHandle = handle;
        this.available = handle.info;
        this.phase = "available";
        if (!silent) {
          message.info(t("update.availableToast", { version: handle.info.version }), 4000);
        }
        return handle;
      } catch (err) {
        this.phase = "error";
        this.error = handleError("update.check", err);
        return null;
      }
    },
    /**
     * 下载并安装更新。
     *
     * 先备份当前版本并写 pending；安装成功后重启应用。
     * 任一步失败都会撤销 pending，保留备份供手动回滚。
     */
    async install() {
      if (!pendingHandle || !this.available || this.busy) return;
      const from = this.available.currentVersion;
      const to = this.available.version;
      this.error = "";
      this.progress = 0;
      this.phase = "downloading";
      try {
        await prepareUpdate(from, to);
      } catch (err) {
        this.phase = "error";
        this.error = handleError("update.prepare", err);
        return;
      }
      try {
        await downloadAndInstall(pendingHandle, (percent) => {
          this.progress = percent < 0 ? -1 : Math.max(0, Math.min(100, Math.round(percent)));
        });
        this.phase = "installing";
        this.progress = 100;
        await relaunchApp();
      } catch (err) {
        this.phase = "error";
        this.error = handleError("update.install", err);
        await cancelUpdate();
      }
    },
    /** 手动回滚到上一版本（后端安排替换脚本后重启）。 */
    async rollback() {
      this.error = "";
      try {
        await rollbackUpdate();
        message.info(t("update.rollingBack"), 4000);
        await relaunchApp();
      } catch (err) {
        this.error = handleError("update.rollback", err);
      }
    },
    /** 重置（测试用）。 */
    reset() {
      this.status = null;
      this.phase = "idle";
      this.available = null;
      this.progress = 0;
      this.error = "";
      this.initialized = false;
      pendingHandle = null;
    },
  },
});
