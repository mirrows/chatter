// 通用提示条（v3）：对话区浮层抽成的可复用组件。
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import NoticeBar from "../../src/components/NoticeBar.vue";

/** 挂载提示条（transition 在测试里保留真实行为）。 */
const mountBar = (props: { text: string; testid?: string; closable?: boolean }) =>
  mount(NoticeBar, { props });

describe("NoticeBar", () => {
  it("有文案时渲染提示与关闭按钮", () => {
    const wrapper = mountBar({ text: "传输失败", testid: "demo-notice" });
    expect(wrapper.get('[data-testid="demo-notice"]').text()).toContain("传输失败");
    expect(wrapper.find('[data-testid="notice-close"]').exists()).toBe(true);
  });

  it("空文案时不渲染任何节点", () => {
    const wrapper = mountBar({ text: "" });
    expect(wrapper.find(".notice-layer").exists()).toBe(false);
  });

  it("点击关闭按钮抛出 close 事件", async () => {
    const wrapper = mountBar({ text: "提示" });
    await wrapper.get('[data-testid="notice-close"]').trigger("click");
    expect(wrapper.emitted("close")).toEqual([[]]);
  });

  it("closable=false 时不显示关闭按钮", () => {
    const wrapper = mountBar({ text: "提示", closable: false });
    expect(wrapper.find('[data-testid="notice-close"]').exists()).toBe(false);
  });
});
