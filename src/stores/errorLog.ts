// 错误日志入口状态（v3）：日志目录里有内容时，设置页才展示入口。
import { defineStore } from "pinia";
import { getErrorLog, isTauri, openErrorLogDir, reportError } from "../api/tauri";
import type { ErrorLogInfo } from "../api/types";

/** 空占位（未加载时）。 */
const EMPTY_INFO: ErrorLogInfo = { dir: "", file: "", hasLogs: false };

/** 错误日志 store。 */
export const useErrorLogStore = defineStore("errorLog", {
  state: () => ({
    /** 日志位置与「目录里是否有日志」。 */
    info: { ...EMPTY_INFO } as ErrorLogInfo,
    /** 是否已向后端确认过状态。 */
    loaded: false,
    /** 最近一次操作失败原因（空串表示无）。 */
    error: "",
  }),
  actions: {
    /** 读取日志位置（进入设置页时调用；非 Tauri 环境跳过）。 */
    async refresh() {
      if (!isTauri()) return;
      try {
        const info = await getErrorLog();
        // 后端异常返回时保持上一次状态，避免模板取到 undefined。
        if (info) this.info = info;
        this.loaded = true;
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 上报一条未知错误；成功后入口立即出现。 */
    async report(scope: string, message: string) {
      if (!isTauri()) return;
      try {
        const info = await reportError(scope, message);
        if (info) this.info = info;
        this.loaded = true;
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 打开日志目录（桌面端）；返回是否成功。 */
    async openDir(): Promise<boolean> {
      try {
        await openErrorLogDir();
        this.error = "";
        return true;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
        return false;
      }
    },
  },
});
