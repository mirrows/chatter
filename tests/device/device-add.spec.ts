// 手动添加设备与上下线提示（F3/F4）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import type { DeviceEntry } from "../../src/api/types";

const api = vi.hoisted(() => ({
  addDeviceByIp: vi.fn(),
  listDevices: vi.fn(),
  getLocalDevice: vi.fn(),
  onDeviceUpdate: vi.fn(),
  refreshDevices: vi.fn(),
  isTauri: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  DEVICE_UPDATE_EVENT: "device://update",
  addDeviceByIp: api.addDeviceByIp,
  listDevices: api.listDevices,
  getLocalDevice: api.getLocalDevice,
  onDeviceUpdate: api.onDeviceUpdate,
  refreshDevices: api.refreshDevices,
  isTauri: api.isTauri,
}));

// 必须在 mock 之后导入被测模块。
import DeviceList from "../../src/views/DeviceList.vue";
import { useChatStore } from "../../src/stores/chat";
import { useDeviceStore } from "../../src/stores/devices";
import { useMessageStore } from "../../src/stores/message";
import { useShareInboxStore } from "../../src/stores/shareInbox";

const peer: DeviceEntry = {
  deviceId: "id-1",
  name: "PC-8f3a",
  addr: "192.168.1.10:28101",
  tcpPort: 28101,
  fingerprint: "AAAA-BBBB-CCCC-DDDD",
};

describe("device add & notice", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    api.isTauri.mockReturnValue(true);
  });
  it("addByIp 成功后加入设备列表并清空错误", async () => {
    api.addDeviceByIp.mockResolvedValue(peer);
    const store = useDeviceStore();

    await expect(store.addByIp(" 192.168.1.10 ")).resolves.toBe(true);
    expect(api.addDeviceByIp).toHaveBeenCalledWith("192.168.1.10");
    expect(store.count).toBe(1);
    expect(store.error).toBe("");
  });

  it("addByIp 空输入不请求后端", async () => {
    const store = useDeviceStore();
    await expect(store.addByIp("   ")).resolves.toBe(false);
    expect(api.addDeviceByIp).not.toHaveBeenCalled();
    expect(store.error).toBe("请输入设备 IP 地址");
  });

  it("addByIp 失败时记录后端错误", async () => {
    api.addDeviceByIp.mockRejectedValue(new Error("连接失败：timeout"));
    const store = useDeviceStore();
    await expect(store.addByIp("10.0.0.2")).resolves.toBe(false);
    expect(store.error).toBe("连接失败：timeout");
  });

  it("新设备上线用 message 提示一次，心跳刷新不再提示（F4 / R8.1）", () => {
    const store = useDeviceStore();
    const messages = useMessageStore();

    store.applyUpdate({ kind: "up", device: peer });
    expect(messages.visible.map((item) => item.text)).toEqual(["PC-8f3a 已上线"]);

    // 心跳重复上报同一设备：不再提示
    messages.clear();
    store.applyUpdate({ kind: "up", device: { ...peer, name: "PC-8f3a" } });
    expect(messages.visible).toHaveLength(0);
  });

  it("设备下线用 message 提示并包含昵称（F4）", () => {
    const store = useDeviceStore();
    const messages = useMessageStore();
    store.upsert(peer);
    store.applyUpdate({ kind: "down", deviceId: "id-1" });
    expect(messages.visible.map((item) => item.text)).toEqual(["PC-8f3a 已下线"]);
    expect(store.count).toBe(0);
  });

  it("设备页提交表单后调用 addByIp 并清空输入", async () => {
    api.addDeviceByIp.mockResolvedValue(peer);
    const wrapper = mount(DeviceList);
    const input = wrapper.get('[data-testid="device-add-input"]');
    await input.setValue("192.168.1.10");

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(api.addDeviceByIp).toHaveBeenCalledWith("192.168.1.10");
    expect((input.element as HTMLInputElement).value).toBe("");
    expect(wrapper.text()).toContain("PC-8f3a");
  });

  it("上下线提示走 message 组件，不再内联渲染在设备页（v3）", async () => {
    const store = useDeviceStore();
    const messages = useMessageStore();
    store.applyUpdate({ kind: "up", device: peer });
    const wrapper = mount(DeviceList);
    // 设备页不再有内联提示元素
    expect(wrapper.find('[data-testid="device-notice"]').exists()).toBe(false);
    // 提示出现在全局 message 队列里
    expect(messages.visible.map((item) => item.text)).toEqual(["PC-8f3a 已上线"]);
  });

  it("系统分享待发送提示显示在设备页，可取消（v3）", async () => {
    const share = useShareInboxStore();
    share.pending = { kind: "text", text: "共享的一段文字" };

    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="device-share-hint"]').text()).toContain("共享的一段文字");

    await wrapper.get('[data-testid="device-share-dismiss"]').trigger("click");
    expect(share.pending).toBeNull();
    expect(wrapper.find('[data-testid="device-share-hint"]').exists()).toBe(false);
  });

  it("未读消息在设备列表显示徽标（F42）", async () => {
    const devices = useDeviceStore();
    devices.upsert(peer);
    const chat = useChatStore();
    chat.unread[peer.deviceId] = 3;

    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="device-unread"]').text()).toBe("3");
  });
});
