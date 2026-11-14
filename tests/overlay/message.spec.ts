// 顶部提示条（v3）：同屏最多 3 条、超出排队、1.5s 自动消失。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import MessageHost from "../../src/components/MessageHost.vue";
import { MAX_VISIBLE_MESSAGES, useMessageStore } from "../../src/stores/message";
import { useDeviceStore } from "../../src/stores/devices";
import type { LocalDevice } from "../../src/api/types";

const mountHost = () => mount(MessageHost, { global: { stubs: { teleport: true } } });

describe("message store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.useFakeTimers();
  });

  afterEach(() => {
    useMessageStore().clear();
    vi.useRealTimers();
  });

  it("默认 1.5s 后自动消失", () => {
    const store = useMessageStore();
    store.push({ text: "已复制", type: "success" });

    expect(store.visible).toHaveLength(1);
    vi.advanceTimersByTime(1400);
    expect(store.visible).toHaveLength(1);

    vi.advanceTimersByTime(200);
    expect(store.visible).toHaveLength(0);
  });

  it(`同屏最多 ${MAX_VISIBLE_MESSAGES} 条，超出排队并依次补位`, () => {
    const store = useMessageStore();
    for (let index = 1; index <= MAX_VISIBLE_MESSAGES + 1; index += 1) {
      store.push({ text: `t${index}` });
    }

    expect(store.visible).toHaveLength(MAX_VISIBLE_MESSAGES);
    expect(store.queue).toHaveLength(1);

    // 同批展示的 3 条同时到点消失，队列中的第 4 条立刻补位
    vi.advanceTimersByTime(1500);
    expect(store.queue).toHaveLength(0);
    expect(store.visible).toHaveLength(1);
    expect(store.visible[0].text).toBe(`t${MAX_VISIBLE_MESSAGES + 1}`);
  });

  it("支持自定义时长与手动移除", () => {
    const store = useMessageStore();
    const id = store.push({ text: "自定义", duration: 3000 });

    vi.advanceTimersByTime(1500);
    expect(store.visible).toHaveLength(1);

    store.remove(id);
    expect(store.visible).toHaveLength(0);
  });

  it("clear 清空全部", () => {
    const store = useMessageStore();
    for (let index = 0; index < 5; index += 1) store.push({ text: `t${index}` });

    store.clear();
    expect(store.visible).toHaveLength(0);
    expect(store.queue).toHaveLength(0);
  });
});

describe("MessageHost", () => {
  const local = (platform: LocalDevice["platform"]): LocalDevice => ({
    deviceId: "self",
    name: "self",
    discoveryPort: 28100,
    tcpPort: 28101,
    fingerprint: "1111-2222-3333-4444",
    platform,
  });

  beforeEach(() => {
    setActivePinia(createPinia());
  });

  afterEach(() => {
    useMessageStore().clear();
  });

  it("渲染提示文案与类型样式", async () => {
    const store = useMessageStore();
    const wrapper = mountHost();
    store.push({ text: "已复制", type: "success", duration: 60000 });
    await flushPromises();

    const toast = wrapper.get('[data-testid="message-success"]');
    expect(toast.text()).toBe("已复制");
    expect(toast.classes()).toContain("success");
  });

  it("移动端右上贴边（is-mobile），桌面端居中", async () => {
    const devices = useDeviceStore();
    devices.local = local("android");
    const wrapper = mountHost();
    expect(wrapper.get('[data-testid="message-root"]').classes()).toContain("is-mobile");

    devices.local = local("windows");
    const desktop = mountHost();
    expect(desktop.get('[data-testid="message-root"]').classes()).not.toContain("is-mobile");
  });
});
