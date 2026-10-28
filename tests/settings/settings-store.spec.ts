// 设置 store 单测：加载、保存、自启、清空历史与主题（F52/F50/F40）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import type { Settings } from "../../src/api/types";

const api = vi.hoisted(() => ({
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
  getAutostart: vi.fn(),
  setAutostart: vi.fn(),
  clearHistory: vi.fn(),
  isTauri: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  getSettings: api.getSettings,
  saveSettings: api.saveSettings,
  getAutostart: api.getAutostart,
  setAutostart: api.setAutostart,
  clearHistory: api.clearHistory,
  isTauri: api.isTauri,
}));

// 必须在 mock 之后导入被测模块。
import { useSettingsStore } from "../../src/stores/settings";

const settings: Settings = {
  deviceId: "self-1",
  nickname: "PC-test",
  discoveryPort: 28100,
  tcpPort: 28101,
  downloadDir: "C:/Downloads/Chatters",
  theme: "dark",
  autoAcceptImages: true,
  autoAcceptFiles: false,
};

describe("settings store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("load 拉取设置与自启状态", async () => {
    api.isTauri.mockReturnValue(true);
    api.getSettings.mockResolvedValue(settings);
    api.getAutostart.mockResolvedValue(true);

    const store = useSettingsStore();
    await store.load();

    expect(store.loaded).toBe(true);
    expect(store.settings.nickname).toBe("PC-test");
    expect(store.settings.theme).toBe("dark");
    expect(store.autostart).toBe(true);
  });

  it("非 Tauri 环境不调用后端", async () => {
    api.isTauri.mockReturnValue(false);
    const store = useSettingsStore();
    await store.load();
    expect(api.getSettings).not.toHaveBeenCalled();
    expect(store.loaded).toBe(false);
  });

  it("save 返回 restartRequired 并记录成功提示", async () => {
    api.saveSettings.mockResolvedValue({ restartRequired: true });
    const store = useSettingsStore();
    store.settings = { ...settings, nickname: "PC-new" };

    await expect(store.save()).resolves.toBe(true);
    expect(api.saveSettings).toHaveBeenCalledWith(store.settings);
    expect(store.restartRequired).toBe(true);
    expect(store.notice).toBe("saved");
    expect(store.saving).toBe(false);
  });

  it("save 失败时记录错误且返回 false", async () => {
    api.saveSettings.mockRejectedValue(new Error("昵称不能为空"));
    const store = useSettingsStore();
    await expect(store.save()).resolves.toBe(false);
    expect(store.error).toBe("昵称不能为空");
  });

  it("toggleAutostart 调用命令并更新状态", async () => {
    api.setAutostart.mockResolvedValue(undefined);
    const store = useSettingsStore();
    await store.toggleAutostart(true);
    expect(api.setAutostart).toHaveBeenCalledWith(true);
    expect(store.autostart).toBe(true);

    api.setAutostart.mockRejectedValue(new Error("denied"));
    await store.toggleAutostart(false);
    expect(store.error).toBe("denied");
  });

  it("clearAllHistory 调用清空命令并给出提示", async () => {
    api.clearHistory.mockResolvedValue(undefined);
    const store = useSettingsStore();
    await store.clearAllHistory();
    expect(api.clearHistory).toHaveBeenCalled();
    expect(store.notice).toBe("historyCleared");
  });

  it("applyTheme 写入根元素 data-theme", () => {
    const store = useSettingsStore();
    store.settings = { ...settings, theme: "dark" };
    store.applyTheme();
    expect(document.documentElement.dataset.theme).toBe("dark");

    store.settings.theme = "light";
    store.applyTheme();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("clear 重置为默认占位值", () => {
    const store = useSettingsStore();
    store.settings = { ...settings };
    store.autostart = true;
    store.clear();
    expect(store.settings.deviceId).toBe("");
    expect(store.autostart).toBe(false);
    expect(store.loaded).toBe(false);
  });
});
