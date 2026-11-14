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
  openDownloadDir: vi.fn(),
  pickDownloadDir: vi.fn(),
  isTauri: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  getSettings: api.getSettings,
  saveSettings: api.saveSettings,
  getAutostart: api.getAutostart,
  setAutostart: api.setAutostart,
  clearHistory: api.clearHistory,
  openDownloadDir: api.openDownloadDir,
  pickDownloadDir: api.pickDownloadDir,
  isTauri: api.isTauri,
}));

// 必须在 mock 之后导入被测模块。
import Settings from "../../src/views/Settings.vue";
import { useConfirmStore } from "../../src/stores/confirm";
import { useDeviceStore } from "../../src/stores/devices";
import { useMessageStore } from "../../src/stores/message";

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

  it("清空历史需二次确认，确认后调用命令并显示提示（v3 需求 7）", async () => {
    api.clearHistory.mockResolvedValue(0);
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-clear-history"]').trigger("click");
    await flushPromises();

    const confirm = useConfirmStore();
    // 未勾选：文案提示「文件保留」
    expect(confirm.active?.message).toContain("下载目录内的文件保留");
    // 未确认前不调用后端
    expect(api.clearHistory).not.toHaveBeenCalled();

    await confirm.confirm();
    await flushPromises();

    expect(api.clearHistory).toHaveBeenCalledWith(undefined, false);
    expect(wrapper.find('[data-testid="settings-history-cleared"]').exists()).toBe(true);
  });

  it("勾选删除文件时弹窗文案变化，确认后带标记并显示数量（F41）", async () => {
    api.clearHistory.mockResolvedValue(2);
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-delete-files"]').setValue(true);
    await wrapper.get('[data-testid="settings-clear-history"]').trigger("click");
    await flushPromises();

    const confirm = useConfirmStore();
    expect(confirm.active?.message).toContain("全部文件");

    await confirm.confirm();
    await flushPromises();

    expect(api.clearHistory).toHaveBeenCalledWith(undefined, true);
    expect(wrapper.get('[data-testid="settings-history-cleared"]').text()).toContain("2");
  });

  it("取消弹窗不会清空历史（v3 需求 7）", async () => {
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-clear-history"]').trigger("click");
    await flushPromises();

    useConfirmStore().cancel();
    await flushPromises();

    expect(api.clearHistory).not.toHaveBeenCalled();
  });

  it("下载目录只读，点击弹目录选择框并即时保存（v3 需求 1）", async () => {
    api.pickDownloadDir.mockResolvedValue("D:/ChattersDownloads");
    api.saveSettings.mockResolvedValue({ restartRequired: false });
    const wrapper = mount(Settings);
    await flushPromises();

    const input = wrapper.get('[data-testid="settings-download-dir"]');
    expect((input.element as HTMLInputElement).readOnly).toBe(true);

    await input.trigger("click");
    await flushPromises();

    expect(api.pickDownloadDir).toHaveBeenCalledTimes(1);
    expect((input.element as HTMLInputElement).value).toBe("D:/ChattersDownloads");
    expect(api.saveSettings).toHaveBeenCalledTimes(1);
    expect((api.saveSettings.mock.calls[0][0] as { downloadDir: string }).downloadDir).toBe(
      "D:/ChattersDownloads",
    );
  });

  it("取消目录选择时不改动当前值（v3 需求 1）", async () => {
    api.pickDownloadDir.mockResolvedValue(null);
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-download-dir"]').trigger("click");
    await flushPromises();

    expect(
      (wrapper.get('[data-testid="settings-download-dir"]').element as HTMLInputElement).value,
    ).toBe("C:/Downloads/Chatters");
    expect(api.saveSettings).not.toHaveBeenCalled();
  });

  it("打开下载目录按钮调用命令；Android 隐藏（v3 需求 6）", async () => {
    api.openDownloadDir.mockResolvedValue(undefined);
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-open-download-dir"]').trigger("click");
    expect(api.openDownloadDir).toHaveBeenCalledTimes(1);

    // Android：没有文件管理器入口，按钮隐藏
    const devices = useDeviceStore();
    devices.local = {
      deviceId: "self",
      name: "phone",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "android",
    };
    const androidWrapper = mount(Settings);
    await flushPromises();
    expect(androidWrapper.find('[data-testid="settings-open-download-dir"]').exists()).toBe(false);
  });

  it("Android 点击下载目录给出 message 轻提示，不调用后端（R8.1/R8.3）", async () => {
    const devices = useDeviceStore();
    devices.local = {
      deviceId: "self",
      name: "phone",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "android",
    };
    const messages = useMessageStore();
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-download-dir"]').trigger("click");
    await flushPromises();

    expect(api.pickDownloadDir).not.toHaveBeenCalled();
    expect(messages.visible.map((item) => item.text)).toEqual([
      "移动端下载目录固定在应用私有目录，无法修改",
    ]);
    // 轻提示默认 1.5s 自动消失（R8.1）
    expect(messages.visible[0].duration).toBe(1500);
    // 不写页面级 error
    expect(wrapper.find('[data-testid="settings-error"]').exists()).toBe(false);
  });

  it("桌面端点击下载目录仍弹目录选择框且不出轻提示（R8.1）", async () => {
    api.pickDownloadDir.mockResolvedValue(null);
    const messages = useMessageStore();
    const wrapper = mount(Settings);
    await flushPromises();

    await wrapper.get('[data-testid="settings-download-dir"]').trigger("click");
    await flushPromises();

    expect(api.pickDownloadDir).toHaveBeenCalledTimes(1);
    expect(messages.visible).toHaveLength(0);
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
