// usePlatform 单测（v3）：桌面/移动分支判断收敛到一处。
import { describe, it, expect, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { useDeviceStore } from "../../src/stores/devices";
import { usePlatform } from "../../src/composables/usePlatform";
import type { LocalDevice } from "../../src/api/types";

const local = (platform: LocalDevice["platform"]): LocalDevice => ({
  deviceId: "self",
  name: "PC-self",
  discoveryPort: 28100,
  tcpPort: 28101,
  fingerprint: "1111-2222-3333-4444",
  platform,
});

describe("usePlatform", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("Android 判定为移动端", () => {
    const devices = useDeviceStore();
    devices.local = local("android");

    const { platform, isAndroid, isWindows, isDesktop } = usePlatform();
    expect(platform.value).toBe("android");
    expect(isAndroid.value).toBe(true);
    expect(isWindows.value).toBe(false);
    expect(isDesktop.value).toBe(false);
  });

  it("Windows/macOS/Linux 判定为桌面端", () => {
    const devices = useDeviceStore();
    const { isAndroid, isWindows, isDesktop, platform } = usePlatform();

    for (const name of ["windows", "macos", "linux"] as const) {
      devices.local = local(name);
      expect(platform.value).toBe(name);
      expect(isDesktop.value).toBe(true);
      expect(isAndroid.value).toBe(false);
    }

    devices.local = local("windows");
    expect(isWindows.value).toBe(true);
  });

  it("未取到本机信息时既不判为桌面也不判为移动", () => {
    const { isAndroid, isWindows, isDesktop, platform } = usePlatform();
    expect(platform.value).toBe("");
    expect(isAndroid.value).toBe(false);
    expect(isWindows.value).toBe(false);
    expect(isDesktop.value).toBe(false);
  });
});
