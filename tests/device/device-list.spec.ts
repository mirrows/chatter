// DeviceList 组件测试：验证设备列表渲染与空状态。
import { describe, it, expect, beforeEach } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import DeviceList from "../../src/views/DeviceList.vue";
import { useDeviceStore } from "../../src/stores/devices";

describe("DeviceList", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
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
    });
    const wrapper = mount(DeviceList);
    expect(wrapper.text()).toContain("PC-8f3a");
    expect(wrapper.text()).toContain("192.168.1.10:51000");
    expect(wrapper.text()).toContain("在线 1 台");
  });

  it("设备下线后从列表移除", () => {
    const store = useDeviceStore();
    store.upsert({ deviceId: "id-1", name: "A", addr: "1.1.1.1:1", tcpPort: 1 });
    store.remove("id-1");
    const wrapper = mount(DeviceList);
    expect(wrapper.text()).not.toContain("A");
  });
});
