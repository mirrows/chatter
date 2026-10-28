// DeviceList 组件测试：验证设备列表渲染、本机信息与空状态。
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
      fingerprint: "AAAA-BBBB-CCCC-DDDD",
    });
    const wrapper = mount(DeviceList);
    expect(wrapper.text()).toContain("PC-8f3a");
    expect(wrapper.text()).toContain("192.168.1.10:51000");
    expect(wrapper.text()).toContain("在线 1 台");
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
    };
    const wrapper = mount(DeviceList);
    expect(wrapper.get('[data-testid="local-device"]').text()).toBe("本机：PC-self");
    expect(wrapper.get('[data-testid="local-fingerprint"]').text()).toBe(
      "指纹 1111-2222-3333-4444",
    );
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
