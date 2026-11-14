// 设备列表状态管理（Pinia store）。
import { defineStore } from "pinia";
import {
  addDeviceByIp,
  getLocalDevice,
  isTauri,
  listDevices,
  onDeviceUpdate,
  refreshDevices,
} from "../api/tauri";
import type { DeviceEntry, DeviceUpdate, LocalDevice } from "../api/types";
import { handleError } from "../utils/errors";

/** 主动重新发现后，等待对端单播应答的时长（毫秒）。 */
const REFRESH_SETTLE_MS = 200;

/** 设备上下线提示（F4）。 */
export interface DeviceNotice {
  kind: "up" | "down";
  name: string;
}

/** 设备状态 store。 */
export const useDeviceStore = defineStore("devices", {
  state: () => ({
    devices: [] as DeviceEntry[],
    local: null as LocalDevice | null,
    /** 是否已完成初始化（快照 + 事件订阅）。 */
    started: false,
    /** 初始化失败信息（空串表示无错误）。 */
    error: "",
    /** 最近一次上下线提示（F4，由界面定时清除）。 */
    notice: null as DeviceNotice | null,
  }),
  getters: {
    /** 在线设备数。 */
    count: (state) => state.devices.length,
    /** 本机昵称；未取到后端信息时为空串。 */
    localName: (state) => state.local?.name ?? "",
    /** 本机指纹（v2/M10）；未取到后端信息时为空串。 */
    localFingerprint: (state) => state.local?.fingerprint ?? "",
    /** 运行平台；未取到后端信息时为空串。 */
    platform: (state) => state.local?.platform ?? "",
    /** 是否 Android（隐藏「打开所在文件夹」等桌面专属操作，v2/M9）。 */
    isAndroid: (state) => state.local?.platform === "android",
    /** 是否 Windows（用于禁用 WebView 右键菜单，v2/M9）。 */
    isWindows: (state) => state.local?.platform === "windows",
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
      this.local = null;
      this.started = false;
      this.error = "";
      this.notice = null;
    },
    /** 应用一次后端推送的设备事件（F4）。 */
    applyUpdate(update: DeviceUpdate) {
      if (update.kind === "up") {
        const isNew = !this.devices.some((d) => d.deviceId === update.device.deviceId);
        this.upsert(update.device);
        // 心跳会周期性重复上报，只有新设备才提示"已上线"。
        if (isNew) this.notice = { kind: "up", name: update.device.name };
      } else {
        const name =
          this.devices.find((d) => d.deviceId === update.deviceId)?.name ?? update.deviceId;
        this.remove(update.deviceId);
        this.notice = { kind: "down", name };
      }
    },
    /** 立即更新本机显示名（v3 改名：本机实时可见，无需重启）。 */
    setLocalName(name: string) {
      if (this.local) this.local = { ...this.local, name };
    },
    /** 清除上下线提示（界面 3 秒后调用）。 */
    clearNotice() {
      this.notice = null;
    },
    /** 手动按 IP 添加设备（F3）；成功返回 true。 */
    async addByIp(addr: string): Promise<boolean> {
      const trimmed = addr.trim();
      if (!trimmed) {
        this.error = "请输入设备 IP 地址";
        return false;
      }
      try {
        const device = await addDeviceByIp(trimmed);
        this.upsert(device);
        this.error = "";
        return true;
      } catch (err) {
        this.error = handleError("devices.addByIp", err);
        return false;
      }
    },
    /**
     * 主动重新发现并刷新设备列表。
     *
     * 场景（v2/M9 真机问题）：Android 的 Wi-Fi 省电策略会过滤入站广播，
     * 对端可能被判定离线而从设备表消失；切回前台或发送失败时调用本方法，
     * 让后端立即补发 query 并重新拉取快照。
     */
    async refresh() {
      if (!isTauri()) return;
      try {
        await refreshDevices();
        await new Promise((resolve) => setTimeout(resolve, REFRESH_SETTLE_MS));
        this.devices = await listDevices();
        this.error = "";
      } catch (err) {
        this.error = handleError("devices.refresh", err);
      }
    },
    /**
     * 拉取设备快照并订阅实时上下线事件。
     *
     * 浏览器预览（非 Tauri 环境）直接跳过；重复调用无副作用。
     */
    async init() {
      if (this.started || !isTauri()) return;
      this.started = true;
      try {
        const [devices, local] = await Promise.all([listDevices(), getLocalDevice()]);
        this.devices = devices;
        this.local = local;
        await onDeviceUpdate((update) => this.applyUpdate(update));
        this.error = "";
      } catch (err) {
        this.started = false;
        this.error = handleError("devices.init", err);
      }
    },
  },
});
