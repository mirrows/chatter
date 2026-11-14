// MessageBubble（v3）：状态右对齐、右键菜单、移动端长按直接复制。
import { describe, it, expect, vi, afterEach } from "vitest";
import { mount } from "@vue/test-utils";
import MessageBubble from "../../src/components/MessageBubble.vue";
import type { ChatMessage } from "../../src/stores/chat";

const message = (overrides: Partial<ChatMessage> = {}): ChatMessage => ({
  id: "m-1",
  peerId: "peer-1",
  direction: "out",
  text: "hello",
  ts: 1,
  status: "delivered",
  ...overrides,
});

describe("MessageBubble", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("发送方向展示状态文案，接收方向不展示", () => {
    const outgoing = mount(MessageBubble, { props: { message: message() } });
    expect(outgoing.get('[data-testid="message-out"]').text()).toContain("已送达");

    const incoming = mount(MessageBubble, {
      props: { message: message({ direction: "in" }) },
    });
    expect(incoming.get('[data-testid="message-in"]').text()).not.toContain("已送达");
  });

  it("PC 右键抛出菜单位置事件（不直接复制）", async () => {
    const wrapper = mount(MessageBubble, { props: { message: message() } });
    await wrapper
      .get('[data-testid="message-out"]')
      .trigger("contextmenu", { clientX: 30, clientY: 40 });

    expect(wrapper.emitted("menu")).toEqual([[{ x: 30, y: 40, centered: false }]]);
    expect(wrapper.emitted("copy")).toBeUndefined();
  });

  it("移动端长按直接复制，不弹菜单（v3）", async () => {
    vi.useFakeTimers();
    const wrapper = mount(MessageBubble, {
      props: { message: message({ direction: "in" }), touchOnly: true },
    });

    await wrapper
      .get('[data-testid="message-in"]')
      .trigger("touchstart", { touches: [{ clientX: 10, clientY: 10 }] });
    vi.advanceTimersByTime(520);

    expect(wrapper.emitted("copy")).toEqual([["m-1"]]);
    expect(wrapper.emitted("menu")).toBeUndefined();
  });
});
