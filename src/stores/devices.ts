// 设备列表状态管理（Pinia store）。
import { defineStore } from "pinia";
import type { DeviceEntry } from "../api/types";

/** 设备状态 store。 */
export const useDeviceStore = defineStore("devices", {
  state: () => ({
    devices: [] as DeviceEntry[],
  }),
  getters: {
    /** 在线设备数。 */
    count: (state) => state.devices.length,
  },
  actions: {
    /** 上线/刷新一台设备。 */
    upsert(device: DeviceEntry) {
      const i = this.devices.findIndex((d) => d.deviceId === device.deviceId);
      if (i >= 0) this.devices[i] = device;
      else this.devices.push(device);
    },
    /** 移除一台设备。 */
    remove(deviceId: string) {
      this.devices = this.devices.filter((d) => d.deviceId !== deviceId);
    },
    /** 清空（测试用）。 */
    clear() {
      this.devices = [];
    },
  },
});
