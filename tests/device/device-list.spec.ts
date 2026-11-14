// DeviceList 组件测试：验证设备列表渲染、本机信息、改名、别名与移动端卡片布局。
import { describe, it, expect, beforeEach, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import type { Settings } from "../../src/api/types";

const api = vi.hoisted(() => ({
  getSettings: vi.fn(),
  saveSettings: vi.fn(),
  isTauri: vi.fn(() => false),
}));

vi.mock("../../src/api/tauri", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../src/api/tauri")>();
  return {
    ...actual,
    getSettings: api.getSettings,
    saveSettings: api.saveSettings,
    isTauri: api.isTauri,
  };
});

// 必须在 mock 之后导入被测模块。
import DeviceList from "../../src/views/DeviceList.vue";
import { useDeviceStore } from "../../src/stores/devices";
import { useMessageStore } from "../../src/stores/message";
import { usePeerAliasStore } from "../../src/stores/peerAliases";
import { usePromptStore } from "../../src/stores/prompt";
import { useSettingsStore } from "../../src/stores/settings";

/** 本机默认设置（改名用例使用）。 */
const localSettings: Settings = {
  deviceId: "self-1",
  nickname: "PC-self",
  discoveryPort: 28100,
  tcpPort: 28101,
  downloadDir: "C:/Downloads/Chatters",
  theme: "light",
  autoAcceptImages: true,
  autoAcceptFiles: false,
  autoCleanEnabled: false,
  autoCleanDays: 7,
};

describe("DeviceList", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    localStorage.clear();
    vi.clearAllMocks();
  });

  it("无设备时显示扫描提示", () => {
    const wrapper = mount(DeviceList);
    expect(wrapper.text()).toContain("未发现其他设备");
  });

  it("发现一台设备后正确渲染", () => {
    const store = useDeviceStore();
    store.upsert({
      deviceId: "id-1",
      name: "PC-8f3a",
      addr: "192.168.1.10:51000",
      tcpPort: 51000,
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    });
    const wrapper = mount(DeviceList);
    expect(wrapper.text()).toContain("PC-8f3a");
    expect(wrapper.text()).toContain("192.168.1.10:51000");
    // v3：在线数改为标题后的「（num）」
    expect(wrapper.get('[data-testid="device-count"]').text()).toBe("（1）");
  });

  it("显示设备指纹便于人工核对（v2/M10）", () => {
    const store = useDeviceStore();
    store.upsert({
      deviceId: "id-1",
      name: "PC-8f3a",
      addr: "192.168.1.10:51000",
      tcpPort: 51000,
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    });
    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="device-fingerprint"]').text()).toBe(
      "指纹 AAAA-BBBB-CCCC-DDDD",
    );
  });

  it("设备下线后从列表移除", () => {
    const store = useDeviceStore();
    store.upsert({
      deviceId: "id-1",
      name: "A",
      addr: "1.1.1.1:1",
      tcpPort: 1,
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    });
    store.remove("id-1");
    const wrapper = mount(DeviceList);
    expect(wrapper.text()).not.toContain("A");
  });

  it("显示本机昵称（F2）", () => {
    const store = useDeviceStore();
    store.local = {
      deviceId: "self-1",
      name: "PC-self",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "windows",
    };
    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="local-device"]').text()).toContain("本机：PC-self");
    // v3：指纹只展示编码，位于「局域网设备」标题行右侧
    expect(wrapper.get('[data-testid="local-fingerprint"]').text()).toBe("1111-2222-3333-4444");
  });

  it("本机指纹与「局域网设备」标题同一行且右对齐（v3）", () => {
    const store = useDeviceStore();
    store.local = {
      deviceId: "self-1",
      name: "PC-self",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "windows",
    };
    const wrapper = mount(DeviceList);

    const head = wrapper.get(".head");
    expect(head.get('[data-testid="local-fingerprint"]').text()).toBe("1111-2222-3333-4444");
    // 添加设备在指纹上方（指纹不再与添加设备行重叠）
    const html = wrapper.html();
    expect(html.indexOf('data-testid="device-add-button"')).toBeLessThan(
      html.indexOf('data-testid="local-fingerprint"'),
    );
  });

  it("修改本机名称：保存设置并立即更新显示（v3 需求 2）", async () => {
    const store = useDeviceStore();
    const prompt = usePromptStore();
    const settings = useSettingsStore();
    store.local = {
      deviceId: "self-1",
      name: "PC-self",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "windows",
    };
    api.getSettings.mockResolvedValue({ ...localSettings });
    api.saveSettings.mockResolvedValue({ restartRequired: true });

    const wrapper = mount(DeviceList);
    await wrapper.get('[data-testid="local-edit-name"]').trigger("click");

    expect(prompt.active?.title).toBe("修改本机名称");
    expect(prompt.draft).toBe("PC-self");

    prompt.draft = "我的电脑";
    prompt.confirm();
    await flushPromises();

    expect(api.saveSettings).toHaveBeenCalledWith(
      expect.objectContaining({ nickname: "我的电脑" }),
    );
    // 本机立即显示新名称（其他设备下次连接生效）
    expect(wrapper.get('[data-testid="local-device"]').text()).toContain("本机：我的电脑");
    expect(settings.restartRequired).toBe(true);
  });

  it("本机名称留空时不保存并保持原名（v3 需求 2）", async () => {
    const store = useDeviceStore();
    const prompt = usePromptStore();
    store.local = {
      deviceId: "self-1",
      name: "PC-self",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "windows",
    };

    const wrapper = mount(DeviceList);
    await wrapper.get('[data-testid="local-edit-name"]').trigger("click");
    prompt.draft = "   ";
    prompt.confirm();
    await flushPromises();

    expect(api.saveSettings).not.toHaveBeenCalled();
    expect(wrapper.get('[data-testid="local-device"]').text()).toContain("本机：PC-self");
  });

  it("自定义设备名称：列表展示别名并缓存，留空恢复默认（v3 需求 3）", async () => {
    const store = useDeviceStore();
    const aliases = usePeerAliasStore();
    store.upsert({
      deviceId: "id-1",
      name: "PC-8f3a",
      addr: "192.168.1.10:51000",
      tcpPort: 51000,
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    });

    aliases.setAlias("id-1", "客厅电脑");
    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="device-name"]').text()).toBe("客厅电脑");
    expect(localStorage.getItem("chatters.peerAliases") ?? "").toContain("客厅电脑");

    // 留空 → 删除别名，恢复对端昵称
    aliases.setAlias("id-1", "");
    await flushPromises();
    expect(wrapper.get('[data-testid="device-name"]').text()).toBe("PC-8f3a");
  });

  it("点击编辑图标弹出输入弹窗并保存新名称（v3 需求 3）", async () => {
    const store = useDeviceStore();
    const prompt = usePromptStore();
    store.upsert({
      deviceId: "id-1",
      name: "PC-8f3a",
      addr: "192.168.1.10:51000",
      tcpPort: 51000,
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    });

    const wrapper = mount(DeviceList);
    await wrapper.get('[data-testid="device-edit-name"]').trigger("click");

    expect(prompt.active?.title).toBe("修改设备名称");
    prompt.draft = "书房电脑";
    prompt.confirm();
    await flushPromises();

    expect(usePeerAliasStore().aliases["id-1"]).toBe("书房电脑");
    expect(wrapper.get('[data-testid="device-name"]').text()).toBe("书房电脑");
    // 修改其他设备名称只影响本机显示，用轻提示确认（R8.1）
    expect(useMessageStore().visible[useMessageStore().visible.length - 1]?.text).toBe(
      "设备名称已更新",
    );
  });

  it("移动端使用卡片网格布局（v3 需求 2）", () => {
    const store = useDeviceStore();
    store.local = {
      deviceId: "self",
      name: "phone",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "android",
    };
    store.upsert({
      deviceId: "id-1",
      name: "PC-8f3a",
      addr: "192.168.1.10:51000",
      tcpPort: 51000,
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    });

    const wrapper = mount(DeviceList);
    expect(wrapper.get(".list").classes()).toContain("is-card");
  });

  it("初始化失败时展示错误信息", () => {
    const store = useDeviceStore();
    store.error = "ipc unavailable";
    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="device-error"]').text()).toBe("ipc unavailable");
  });

  it("点击开始聊天向父组件发出 select 事件（F10）", async () => {
    const store = useDeviceStore();
    const device = {
      deviceId: "id-1",
      name: "PC-8f3a",
      addr: "192.168.1.10:51000",
      tcpPort: 51000,
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    };
    store.upsert(device);

    const wrapper = mount(DeviceList);
    await wrapper.get('[data-testid="device-open"]').trigger("click");

    expect(wrapper.emitted("select")).toEqual([[device]]);
  });
});
