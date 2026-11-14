// 确认弹窗（v3）：store 队列语义 + 宿主渲染/交互。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import ConfirmHost from "../../src/components/ConfirmHost.vue";
import { confirmDialog, useConfirmStore } from "../../src/stores/confirm";

/** 宿主用了 Teleport，测试里 stub 掉以便在 wrapper 内查询。 */
const mountHost = () => mount(ConfirmHost, { global: { stubs: { teleport: true } } });

describe("confirm store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("确认后 resolve(true) 并执行 onConfirm", async () => {
    const onConfirm = vi.fn();
    const result = confirmDialog({ title: "清空", onConfirm });
    const store = useConfirmStore();

    expect(store.active?.title).toBe("清空");
    await store.confirm();

    await expect(result).resolves.toBe(true);
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(store.active).toBeNull();
  });

  it("取消后 resolve(false) 并执行 onCancel", async () => {
    const onCancel = vi.fn();
    const result = confirmDialog({ title: "清空", onCancel });
    const store = useConfirmStore();

    store.cancel();

    await expect(result).resolves.toBe(false);
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  it("支持多个弹窗排队：一次只展示一个，完成后自动下一个", async () => {
    const first = confirmDialog({ title: "第一个" });
    const second = confirmDialog({ title: "第二个" });
    const store = useConfirmStore();

    expect(store.dialogs).toHaveLength(2);
    expect(store.active?.title).toBe("第一个");

    await store.confirm();
    await expect(first).resolves.toBe(true);
    expect(store.active?.title).toBe("第二个");

    await store.confirm();
    await expect(second).resolves.toBe(true);
    expect(store.active).toBeNull();
  });

  it("异步 onConfirm 期间防重复点击", async () => {
    const control: { release?: () => void } = {};
    const onConfirm = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          control.release = resolve;
        }),
    );
    confirmDialog({ title: "异步", onConfirm });
    const store = useConfirmStore();

    const first = store.confirm();
    void store.confirm(); // 第二次点击应被忽略
    expect(onConfirm).toHaveBeenCalledTimes(1);

    control.release?.();
    await first;
    expect(store.active).toBeNull();
  });
});

describe("ConfirmHost", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("渲染标题与正文，点击确定/取消生效", async () => {
    const wrapper = mountHost();
    const result = confirmDialog({ title: "清空全部历史", message: "不可恢复" });
    await flushPromises();

    expect(wrapper.get('[data-testid="confirm-title"]').text()).toBe("清空全部历史");
    expect(wrapper.get('[data-testid="confirm-message"]').text()).toBe("不可恢复");

    await wrapper.get('[data-testid="confirm-ok"]').trigger("click");
    await flushPromises();
    await expect(result).resolves.toBe(true);
    expect(wrapper.find('[data-testid="confirm-dialog"]').exists()).toBe(false);
  });

  it("点击遮罩取消", async () => {
    const wrapper = mountHost();
    const result = confirmDialog({ title: "删除" });
    await flushPromises();

    await wrapper.get('[data-testid="confirm-root"]').trigger("click");
    await expect(result).resolves.toBe(false);
  });

  it("Esc 取消", async () => {
    mountHost();
    const result = confirmDialog({ title: "删除" });
    await flushPromises();

    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await expect(result).resolves.toBe(false);
  });

  it("自定义按钮文案与危险样式", async () => {
    const wrapper = mountHost();
    confirmDialog({ title: "x", confirmText: "清空", cancelText: "再想想", danger: true });
    await flushPromises();

    expect(wrapper.get('[data-testid="confirm-ok"]').text()).toBe("清空");
    expect(wrapper.get('[data-testid="confirm-cancel"]').text()).toBe("再想想");
    expect(wrapper.get('[data-testid="confirm-ok"]').classes()).toContain("danger");
  });
});
