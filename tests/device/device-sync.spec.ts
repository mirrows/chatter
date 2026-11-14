// 设备初始化与实时事件订阅（mock 掉 Tauri IPC 层）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import type { DeviceEntry, DeviceUpdate, LocalDevice } from "../../src/api/types";

const api = vi.hoisted(() => ({
  listDevices: vi.fn(),
  getLocalDevice: vi.fn(),
  onDeviceUpdate: vi.fn(),
  refreshDevices: vi.fn(),
  isTauri: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  DEVICE_UPDATE_EVENT: "device://update",
  listDevices: api.listDevices,
  getLocalDevice: api.getLocalDevice,
  onDeviceUpdate: api.onDeviceUpdate,
  refreshDevices: api.refreshDevices,
  isTauri: api.isTauri,
}));

// 必须在 mock 之后导入被测模块。
import { useDeviceStore } from "../../src/stores/devices";

const peer: DeviceEntry = {
  deviceId: "peer-1",
  name: "PC-peer",
  addr: "192.168.1.20:28101",
  tcpPort: 28101,
  fingerprint: "AAAA-BBBB-CCCC-DDDD",
};

const local: LocalDevice = {
  deviceId: "self-1",
  name: "PC-self",
  discoveryPort: 28100,
  tcpPort: 28101,
  fingerprint: "1111-2222-3333-4444",
  platform: "windows",
};

describe("device store init", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("拉取快照、保存本机信息并订阅事件", async () => {
    api.isTauri.mockReturnValue(true);
    api.listDevices.mockResolvedValue([peer]);
    api.getLocalDevice.mockResolvedValue(local);
    api.onDeviceUpdate.mockResolvedValue(() => {});

    const store = useDeviceStore();
    await store.init();

    expect(store.started).toBe(true);
    expect(store.count).toBe(1);
    expect(store.localName).toBe("PC-self");

    const handler = api.onDeviceUpdate.mock.calls[0][0] as (u: DeviceUpdate) => void;
    handler({ kind: "down", deviceId: "peer-1" });
    expect(store.count).toBe(0);
  });

  it("非 Tauri 环境跳过初始化（浏览器预览）", async () => {
    api.isTauri.mockReturnValue(false);

    const store = useDeviceStore();
    await store.init();

    expect(api.listDevices).not.toHaveBeenCalled();
    expect(api.onDeviceUpdate).not.toHaveBeenCalled();
    expect(store.started).toBe(false);
  });

  it("重复调用 init 只订阅一次", async () => {
    api.isTauri.mockReturnValue(true);
    api.listDevices.mockResolvedValue([]);
    api.getLocalDevice.mockResolvedValue(local);
    api.onDeviceUpdate.mockResolvedValue(() => {});

    const store = useDeviceStore();
    await store.init();
    await store.init();

    expect(api.listDevices).toHaveBeenCalledTimes(1);
    expect(api.onDeviceUpdate).toHaveBeenCalledTimes(1);
  });

  it("初始化失败记录错误并允许重试", async () => {
    api.isTauri.mockReturnValue(true);
    api.listDevices.mockRejectedValue(new Error("ipc unavailable"));

    const store = useDeviceStore();
    await store.init();

    expect(store.started).toBe(false);
    expect(store.error).toBe("操作失败，请稍后重试");
    expect(store.count).toBe(0);
  });

  it("refresh 触发重新发现并重新拉取快照（v2/M9 真机问题）", async () => {
    api.isTauri.mockReturnValue(true);
    api.refreshDevices.mockResolvedValue(undefined);
    api.listDevices.mockResolvedValue([peer]);

    const store = useDeviceStore();
    await store.refresh();

    expect(api.refreshDevices).toHaveBeenCalledTimes(1);
    expect(api.listDevices).toHaveBeenCalledTimes(1);
    expect(store.count).toBe(1);
    expect(store.error).toBe("");
  });

  it("非 Tauri 环境下 refresh 不做任何 IPC 调用", async () => {
    api.isTauri.mockReturnValue(false);

    const store = useDeviceStore();
    await store.refresh();

    expect(api.refreshDevices).not.toHaveBeenCalled();
  });
});
