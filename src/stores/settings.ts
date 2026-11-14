// 设置状态管理（F52）：加载、保存、自启与清空历史（M4）。
import { defineStore } from "pinia";
import {
  clearHistory,
  getAutostart,
  getSettings,
  isTauri,
  openDownloadDir as openDownloadDirApi,
  pickDownloadDir as pickDownloadDirApi,
  saveSettings,
  setAutostart,
} from "../api/tauri";
import type { Settings } from "../api/types";

/** 默认设置（未加载前的占位值）。 */
const EMPTY_SETTINGS: Settings = {
  deviceId: "",
  nickname: "",
  discoveryPort: 28100,
  tcpPort: 28101,
  downloadDir: "",
  theme: "light",
  autoAcceptImages: true,
  autoAcceptFiles: false,
};

/** 设置 store。 */
export const useSettingsStore = defineStore("settings", {
  state: () => ({
    settings: { ...EMPTY_SETTINGS } as Settings,
    /** 是否已开启开机自启。 */
    autostart: false,
    /** 是否已加载过。 */
    loaded: false,
    saving: false,
    /** 清空历史时是否同时删除已接收文件（F41）。 */
    deleteFiles: false,
    /** 最近一次清空历史删除的文件数。 */
    lastRemovedFiles: 0,
    /** 昵称/端口改动需重启生效。 */
    restartRequired: false,
    /** 操作成功提示（空串表示无）。 */
    notice: "",
    error: "",
  }),
  actions: {
    /** 读取设置与自启状态。 */
    async load() {
      if (this.loaded || !isTauri()) return;
      try {
        const [settings, autostart] = await Promise.all([getSettings(), getAutostart()]);
        this.settings = settings;
        this.autostart = autostart;
        this.loaded = true;
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 保存设置；返回是否需要重启。 */
    async save(): Promise<boolean> {
      this.saving = true;
      this.notice = "";
      try {
        const result = await saveSettings(this.settings);
        this.restartRequired = result.restartRequired;
        this.notice = "saved";
        this.error = "";
        return result.restartRequired;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
        return false;
      } finally {
        this.saving = false;
      }
    },
    /**
     * 修改本机名称（v3）：写入配置并保存。
     *
     * 本机界面立即显示新名称；其他设备在下次连接/应用重启后的广播中看到新名称。
     * 成功返回 true（调用方负责提示），失败写入 error 并返回 false。
     */
    async renameLocal(name: string): Promise<boolean> {
      const nickname = name.trim();
      if (!nickname) return false;
      try {
        if (!this.loaded) {
          this.settings = await getSettings();
          this.loaded = true;
        }
        this.settings = { ...this.settings, nickname };
        const result = await saveSettings(this.settings);
        this.restartRequired = result.restartRequired;
        this.error = "";
        return true;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
        return false;
      }
    },
    /** 切换开机自启（F50）。 */
    async toggleAutostart(enabled: boolean) {
      try {
        await setAutostart(enabled);
        this.autostart = enabled;
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 打开下载目录（v3）：失败写入 error 由设置页展示。 */
    async openDownloadDir() {
      try {
        await openDownloadDirApi();
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /**
     * 选择下载目录（v3）：弹系统目录选择框，选中后即时保存设置。
     *
     * 取消选择时不改动当前值；保存失败写入 error。
     */
    async pickDownloadDir() {
      try {
        const picked = await pickDownloadDirApi();
        if (!picked) return;
        this.settings.downloadDir = picked;
        await this.save();
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 清空全部历史（F40/F41），可选择同时删除已接收文件。 */
    async clearAllHistory() {
      try {
        const removed = await clearHistory(undefined, this.deleteFiles);
        this.lastRemovedFiles = removed;
        this.notice = "historyCleared";
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 应用主题到根元素（light/dark 由 CSS 变量切换）。 */
    applyTheme() {
      document.documentElement.dataset.theme = this.settings.theme;
    },
    /** 重置（测试用）。 */
    clear() {
      this.settings = { ...EMPTY_SETTINGS };
      this.autostart = false;
      this.loaded = false;
      this.saving = false;
      this.restartRequired = false;
      this.notice = "";
      this.error = "";
      this.deleteFiles = false;
      this.lastRemovedFiles = 0;
    },
  },
});
