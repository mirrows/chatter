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
  isTauri: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  DEVICE_UPDATE_EVENT: "device://update",
  addDeviceByIp: api.addDeviceByIp,
  listDevices: api.listDevices,
  getLocalDevice: api.getLocalDevice,
  onDeviceUpdate: api.onDeviceUpdate,
  isTauri: api.isTauri,
}));

// 必须在 mock 之后导入被测模块。
import DeviceList from "../../src/views/DeviceList.vue";
import { useChatStore } from "../../src/stores/chat";
import { useDeviceStore } from "../../src/stores/devices";

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

  it("新设备上线提示一次，心跳刷新不再提示", () => {
    const store = useDeviceStore();
    store.applyUpdate({ kind: "up", device: peer });
    expect(store.notice).toEqual({ kind: "up", name: "PC-8f3a" });

    store.clearNotice();
    store.applyUpdate({ kind: "up", device: { ...peer, name: "PC-8f3a" } });
    expect(store.notice).toBeNull();
  });

  it("设备下线提示包含昵称", () => {
    const store = useDeviceStore();
    store.upsert(peer);
    store.applyUpdate({ kind: "down", deviceId: "id-1" });
    expect(store.notice).toEqual({ kind: "down", name: "PC-8f3a" });
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

  it("上下线提示渲染在设备页", async () => {
    const store = useDeviceStore();
    store.applyUpdate({ kind: "up", device: peer });
    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="device-notice"]').text()).toBe("PC-8f3a 已上线");
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
