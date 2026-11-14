// 图片预览（v3）：store 单例 + 宿主渲染/关闭。
import { describe, it, expect, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import ImagePreviewHost from "../../src/components/ImagePreviewHost.vue";
import { useImagePreviewStore } from "../../src/stores/imagePreview";

const SRC = "data:image/png;base64,AAA";

/** 宿主用了 Teleport，测试里 stub 掉以便在 wrapper 内查询。 */
const mountHost = () => mount(ImagePreviewHost, { global: { stubs: { teleport: true } } });

describe("image preview store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("打开与关闭", () => {
    const store = useImagePreviewStore();
    store.open(SRC, "photo.png");
    expect(store.visible).toBe(true);
    expect(store.src).toBe(SRC);
    expect(store.alt).toBe("photo.png");

    store.close();
    expect(store.visible).toBe(false);
    expect(store.src).toBe("");
  });

  it("空地址不打开（避免闪一个空弹窗）", () => {
    const store = useImagePreviewStore();
    store.open("");
    expect(store.visible).toBe(false);
  });
});

describe("ImagePreviewHost", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("未打开时不渲染任何节点", () => {
    const wrapper = mountHost();
    expect(wrapper.find('[data-testid="image-preview"]').exists()).toBe(false);
  });

  it("打开后全屏容器渲染图片", async () => {
    const store = useImagePreviewStore();
    const wrapper = mountHost();
    store.open(SRC, "photo.png");
    await flushPromises();

    expect(wrapper.find('[data-testid="image-preview"]').exists()).toBe(true);
    const img = wrapper.get('[data-testid="image-preview-img"]');
    expect(img.attributes("src")).toBe(SRC);
    expect(img.attributes("alt")).toBe("photo.png");
  });

  it("关闭按钮与 Esc 都能关闭", async () => {
    const store = useImagePreviewStore();
    const wrapper = mountHost();

    store.open(SRC);
    await flushPromises();
    await wrapper.get('[data-testid="image-preview-close"]').trigger("click");
    expect(store.visible).toBe(false);

    store.open(SRC);
    await flushPromises();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(store.visible).toBe(false);
  });
  it("PC 滚轮可缩放，关闭后复位（v3）", async () => {
    const store = useImagePreviewStore();
    const wrapper = mountHost();
    store.open(SRC);
    await flushPromises();

    const root = wrapper.get('[data-testid="image-preview"]');
    // 向上滚 → 放大
    root.element.dispatchEvent(
      new WheelEvent("wheel", { deltaY: -100, clientX: 300, clientY: 200, bubbles: true }),
    );
    await flushPromises();
    let style = wrapper.get('[data-testid="image-preview-img"]').attributes("style") ?? "";
    expect(style).toContain("scale(1.2");

    // 向下滚 → 缩回原大小
    root.element.dispatchEvent(
      new WheelEvent("wheel", { deltaY: 100, clientX: 300, clientY: 200, bubbles: true }),
    );
    await flushPromises();
    style = wrapper.get('[data-testid="image-preview-img"]').attributes("style") ?? "";
    expect(style).toContain("scale(1)");
    expect(style).toContain("translate(0px, 0px)");
  });

  it("PC 放大后可拖拽平移，未放大时拖动不偏移（v3）", async () => {
    const store = useImagePreviewStore();
    const wrapper = mountHost();
    store.open(SRC);
    await flushPromises();
    const img = wrapper.get('[data-testid="image-preview-img"]');
    const imgEl = img.element as HTMLElement;
    const rootEl = wrapper.get('[data-testid="image-preview"]').element;
    const mouse = (type: string, x: number, y: number) =>
      new MouseEvent(type, { button: 0, clientX: x, clientY: y, bubbles: true });

    // 未放大：拖动不改变位置
    imgEl.dispatchEvent(mouse("mousedown", 100, 100));
    window.dispatchEvent(mouse("mousemove", 160, 140));
    window.dispatchEvent(mouse("mouseup", 160, 140));
    await flushPromises();
    let style = img.attributes("style") ?? "";
    expect(style).toContain("translate(0px, 0px)");

    // 先放大，再拖拽：位移生效
    rootEl.dispatchEvent(
      new WheelEvent("wheel", {
        deltaY: -100,
        clientX: 300,
        clientY: 200,
        bubbles: true,
        cancelable: true,
      }),
    );
    await flushPromises();
    imgEl.dispatchEvent(mouse("mousedown", 100, 100));
    window.dispatchEvent(mouse("mousemove", 140, 120));
    await flushPromises();
    style = wrapper.get('[data-testid="image-preview-img"]').attributes("style") ?? "";
    expect(style).not.toContain("translate(0px, 0px)");

    window.dispatchEvent(mouse("mouseup", 140, 120));
    await flushPromises();
  });

  it("移动端单指拖拽可平移（放大后）（v3）", async () => {
    const store = useImagePreviewStore();
    const wrapper = mountHost();
    store.open(SRC);
    await flushPromises();

    const touchEvent = (type: string, points: Array<{ clientX: number; clientY: number }>) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", { value: points });
      return event;
    };
    const root = wrapper.get('[data-testid="image-preview"]');

    // 双指放大到约 2 倍
    root.element.dispatchEvent(
      touchEvent("touchstart", [
        { clientX: 0, clientY: 0 },
        { clientX: 100, clientY: 0 },
      ]),
    );
    root.element.dispatchEvent(
      touchEvent("touchmove", [
        { clientX: 0, clientY: 0 },
        { clientX: 200, clientY: 0 },
      ]),
    );
    root.element.dispatchEvent(touchEvent("touchend", []));
    await flushPromises();

    // 单指拖动 40×30
    root.element.dispatchEvent(touchEvent("touchstart", [{ clientX: 100, clientY: 100 }]));
    root.element.dispatchEvent(touchEvent("touchmove", [{ clientX: 140, clientY: 130 }]));
    await flushPromises();

    const style = wrapper.get('[data-testid="image-preview-img"]').attributes("style") ?? "";
    expect(style).not.toContain("translate(0px, 0px)");
  });

  it("双指可放大图片，关闭后下次打开恢复原大小（v3 需求 6）", async () => {
    const store = useImagePreviewStore();
    const wrapper = mountHost();
    store.open(SRC);
    await flushPromises();

    /** jsdom 没有 TouchEvent，手工挂载 touches 字段。 */
    const touchEvent = (type: string, points: Array<{ clientX: number; clientY: number }>) => {
      const event = new Event(type, { bubbles: true, cancelable: true });
      Object.defineProperty(event, "touches", { value: points });
      return event;
    };
    const root = wrapper.get('[data-testid="image-preview"]');

    // 起始间距 100px → 拉伸到 200px，应放大约 2 倍
    root.element.dispatchEvent(
      touchEvent("touchstart", [
        { clientX: 0, clientY: 0 },
        { clientX: 100, clientY: 0 },
      ]),
    );
    root.element.dispatchEvent(
      touchEvent("touchmove", [
        { clientX: 0, clientY: 0 },
        { clientX: 200, clientY: 0 },
      ]),
    );
    await flushPromises();
    expect(wrapper.get('[data-testid="image-preview-img"]').attributes("style")).toContain(
      "scale(2",
    );

    // 关闭再打开：恢复到 1 倍
    store.close();
    await flushPromises();
    store.open(SRC);
    await flushPromises();
    expect(wrapper.get('[data-testid="image-preview-img"]').attributes("style")).toContain(
      "scale(1)",
    );
  });
});
