// Settings 组件测试：字段渲染、保存、自启与清空历史（F52/F50/F40）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

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
import Settings from "../../src/views/Settings.vue";

const settings = {
  deviceId: "self-1",
  nickname: "PC-test",
  discoveryPort: 28100,
  tcpPort: 28101,
  downloadDir: "C:/Downloads/Chatters",
  theme: "light",
  autoAcceptImages: true,
  autoAcceptFiles: false,
};

describe("Settings view", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    api.isTauri.mockReturnValue(true);
    api.getSettings.mockResolvedValue({ ...settings });
    api.getAutostart.mockResolvedValue(false);
  });

  it("加载后渲染设置字段（设备 ID 只读）", async () => {
    const wrapper = mount(Settings);
    await flushPromises();

    expect(
      (wrapper.get('[data-testid="settings-nickname"]').element as HTMLInputElement).value,
    ).toBe("PC-test");
    expect(
      (wrapper.get('[data-testid="settings-device-id"]').element as HTMLInputElement).disabled,
    ).toBe(true);
    expect(
      (wrapper.get('[data-testid="settings-discovery-port"]').element as HTMLInputElement).value,
    ).toBe("28100");
    expect(
      (wrapper.get('[data-testid="settings-auto-images"]').element as HTMLInputElement).checked,
    ).toBe(true);
  });

  it("保存时把编辑后的值提交给后端", async () => {
    api.saveSettings.mockResolvedValue({ restartRequired: false });
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-nickname"]').setValue("PC-new");
    await wrapper.get('[data-testid="settings-save"]').trigger("click");
    await flushPromises();

    expect(api.saveSettings).toHaveBeenCalledTimes(1);
    const submitted = api.saveSettings.mock.calls[0][0] as { nickname: string };
    expect(submitted.nickname).toBe("PC-new");
    expect(wrapper.find('[data-testid="settings-saved"]').exists()).toBe(true);
  });

  it("昵称或端口变更后提示需要重启", async () => {
    api.saveSettings.mockResolvedValue({ restartRequired: true });
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-save"]').trigger("click");
    await flushPromises();

    expect(wrapper.find('[data-testid="settings-restart"]').exists()).toBe(true);
  });

  it("勾选自启调用 autostart 命令", async () => {
    api.setAutostart.mockResolvedValue(undefined);
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-autostart"]').setValue(true);
    expect(api.setAutostart).toHaveBeenCalledWith(true);
  });

  it("清空历史按钮调用命令并显示提示", async () => {
    api.clearHistory.mockResolvedValue(0);
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-clear-history"]').trigger("click");
    await flushPromises();

    expect(api.clearHistory).toHaveBeenCalledWith(undefined, false);
    expect(wrapper.find('[data-testid="settings-history-cleared"]').exists()).toBe(true);
  });

  it("勾选同时删除文件后清空历史会带上标记并显示删除数量（F41）", async () => {
    api.clearHistory.mockResolvedValue(2);
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-delete-files"]').setValue(true);
    await wrapper.get('[data-testid="settings-clear-history"]').trigger("click");
    await flushPromises();

    expect(api.clearHistory).toHaveBeenCalledWith(undefined, true);
    expect(wrapper.get('[data-testid="settings-history-cleared"]').text()).toContain("2");
  });

  it("后端报错时显示错误信息", async () => {
    api.saveSettings.mockRejectedValue(new Error("昵称不能为空"));
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-save"]').trigger("click");
    await flushPromises();

    expect(wrapper.get('[data-testid="settings-error"]').text()).toBe("昵称不能为空");
  });
});
