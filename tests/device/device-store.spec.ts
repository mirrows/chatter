// 设备 store 单测：事件应用、去重与状态重置（对应 devices.ts）。
import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useDeviceStore } from "../../src/stores/devices";
import type { DeviceEntry } from "../../src/api/types";

const entry = (id: string, name: string): DeviceEntry => ({
  deviceId: id,
  name,
  addr: "192.168.1.10:28101",
  tcpPort: 28101,
  fingerprint: "AAAA-BBBB-CCCC-DDDD",
});

describe("device store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("applyUpdate(up) 新增设备并计算在线数", () => {
    const store = useDeviceStore();
    store.applyUpdate({ kind: "up", device: entry("id-1", "PC-a") });
    expect(store.count).toBe(1);
    expect(store.devices[0].name).toBe("PC-a");
  });

  it("applyUpdate(up) 同一 deviceId 只保留最新一条", () => {
    const store = useDeviceStore();
    store.applyUpdate({ kind: "up", device: entry("id-1", "PC-old") });
    store.applyUpdate({
      kind: "up",
      device: { ...entry("id-1", "PC-new"), addr: "192.168.1.11:28101" },
    });
    expect(store.count).toBe(1);
    expect(store.devices[0].name).toBe("PC-new");
    expect(store.devices[0].addr).toBe("192.168.1.11:28101");
  });

  it("applyUpdate(down) 移除对应设备", () => {
    const store = useDeviceStore();
    store.applyUpdate({ kind: "up", device: entry("id-1", "PC-a") });
    store.applyUpdate({ kind: "down", deviceId: "id-1" });
    expect(store.count).toBe(0);
  });

  it("applyUpdate(down) 未知设备不抛错", () => {
    const store = useDeviceStore();
    expect(() => store.applyUpdate({ kind: "down", deviceId: "nope" })).not.toThrow();
    expect(store.count).toBe(0);
  });

  it("clear 重置设备、本机信息与初始化标记", () => {
    const store = useDeviceStore();
    store.applyUpdate({ kind: "up", device: entry("id-1", "PC-a") });
    store.local = {
      deviceId: "self",
      name: "PC-self",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
      platform: "windows",
    };
    store.started = true;
    store.error = "x";
    store.clear();
    expect(store.count).toBe(0);
    expect(store.local).toBeNull();
    expect(store.localName).toBe("");
    expect(store.started).toBe(false);
    expect(store.error).toBe("");
  });

  it("依据平台暴露 isAndroid / isWindows（v2/M9）", () => {
    const store = useDeviceStore();
    const base = {
      deviceId: "self",
      name: "PC-self",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
    };

    store.local = { ...base, platform: "android" };
    expect(store.isAndroid).toBe(true);
    expect(store.isWindows).toBe(false);

    store.local = { ...base, platform: "windows" };
    expect(store.isAndroid).toBe(false);
    expect(store.isWindows).toBe(true);

    // 未取到后端信息时两个判断都为 false，避免误隐藏按钮。
    store.local = null;
    expect(store.isAndroid).toBe(false);
    expect(store.isWindows).toBe(false);
  });
});
