// 右键/长按菜单（v3）：store 单例语义 + 宿主定位/边缘翻转/关闭。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import ContextMenuHost from "../../src/components/ContextMenuHost.vue";
import { useContextMenuStore, type ContextMenuAction } from "../../src/stores/contextMenu";

/** 宿主用了 Teleport，测试里 stub 掉以便在 wrapper 内查询。 */
const mountHost = () => mount(ContextMenuHost, { global: { stubs: { teleport: true } } });

/** 构造菜单项。 */
function items(onSelect = vi.fn()): ContextMenuAction[] {
  return [{ key: "copy", label: "复制", onSelect }];
}

describe("context menu store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("同一时刻只保留一个菜单（后开覆盖先开）", () => {
    const store = useContextMenuStore();
    store.openAtPointer({ x: 1, y: 2 }, items());
    store.openCentered(items());

    expect(store.visible).toBe(true);
    expect(store.centered).toBe(true);
    expect(store.items).toHaveLength(1);
  });

  it("空菜单不打开", () => {
    const store = useContextMenuStore();
    store.openAtPointer({ x: 1, y: 2 }, []);
    expect(store.visible).toBe(false);
  });

  it("select 先关闭菜单再执行回调", async () => {
    const onSelect = vi.fn();
    const store = useContextMenuStore();
    store.openCentered(items(onSelect));

    await store.select(store.items[0]);

    expect(store.visible).toBe(false);
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("禁用项不执行回调", async () => {
    const onSelect = vi.fn();
    const store = useContextMenuStore();
    store.openCentered([{ key: "x", label: "x", disabled: true, onSelect }]);

    await store.select(store.items[0]);
    expect(onSelect).not.toHaveBeenCalled();
  });
});

describe("ContextMenuHost", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("PC 在鼠标位置展示，并渲染全部菜单项", async () => {
    const store = useContextMenuStore();
    const wrapper = mountHost();
    store.openAtPointer({ x: 100, y: 100 }, items());
    await flushPromises();

    const menu = wrapper.get('[data-testid="context-menu"]');
    expect(menu.attributes("style")).toContain("left: 100px");
    expect(menu.attributes("style")).toContain("top: 100px");
    expect(wrapper.find('[data-testid="context-menu-copy"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="context-menu-mask"]').exists()).toBe(false);
  });

  it("靠近右下角时翻转到鼠标左侧/上方（边缘检测）", async () => {
    const store = useContextMenuStore();
    const wrapper = mountHost();
    store.openAtPointer({ x: window.innerWidth - 10, y: window.innerHeight - 10 }, items());
    await flushPromises();

    const style = wrapper.get('[data-testid="context-menu"]').attributes("style") ?? "";
    // 兜底宽度 160 / 高度 120（jsdom 无布局）
    expect(style).toContain(`left: ${window.innerWidth - 10 - 160}px`);
    expect(style).toContain(`top: ${window.innerHeight - 10 - 120}px`);
  });

  it("移动端居中展示并带遮罩", async () => {
    const store = useContextMenuStore();
    const wrapper = mountHost();
    store.openCentered(items());
    await flushPromises();

    expect(wrapper.get('[data-testid="context-menu-root"]').classes()).toContain("is-centered");
    expect(wrapper.find('[data-testid="context-menu-mask"]').exists()).toBe(true);
  });

  it("点击菜单项执行回调并关闭", async () => {
    const onSelect = vi.fn();
    const store = useContextMenuStore();
    const wrapper = mountHost();
    store.openCentered(items(onSelect));
    await flushPromises();

    await wrapper.get('[data-testid="context-menu-copy"]').trigger("click");
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(store.visible).toBe(false);
  });

  it("Esc 关闭菜单", async () => {
    const store = useContextMenuStore();
    mount(ContextMenuHost);
    store.openCentered(items());
    await flushPromises();

    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(store.visible).toBe(false);
  });
});
