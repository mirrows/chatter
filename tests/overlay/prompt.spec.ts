// 输入弹窗（v3）：store 队列语义 + 宿主渲染/交互。
import { describe, it, expect, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import PromptHost from "../../src/components/PromptHost.vue";
import { promptDialog, usePromptStore } from "../../src/stores/prompt";

const mountHost = () => mount(PromptHost, { global: { stubs: { teleport: true } } });

describe("prompt store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("确认返回输入内容（去首尾空格）", async () => {
    const result = promptDialog({ title: "改名", value: "旧名" });
    const store = usePromptStore();
    expect(store.draft).toBe("旧名");

    store.draft = "  新名  ";
    store.confirm();

    await expect(result).resolves.toBe("新名");
    expect(store.active).toBeNull();
  });

  it("取消返回 null", async () => {
    const result = promptDialog({ title: "改名" });
    usePromptStore().cancel();
    await expect(result).resolves.toBeNull();
  });

  it("支持多个弹窗排队", async () => {
    const first = promptDialog({ title: "一", value: "a" });
    const second = promptDialog({ title: "二", value: "b" });
    const store = usePromptStore();

    expect(store.active?.title).toBe("一");
    store.confirm();
    await expect(first).resolves.toBe("a");
    expect(store.active?.title).toBe("二");
    expect(store.draft).toBe("b");

    store.cancel();
    await expect(second).resolves.toBeNull();
  });
});

describe("PromptHost", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("渲染标题/说明/初始值，点击确定生效", async () => {
    const wrapper = mountHost();
    const result = promptDialog({ title: "修改设备名称", message: "留空恢复默认", value: "旧名" });
    await flushPromises();

    expect(wrapper.get('[data-testid="prompt-title"]').text()).toBe("修改设备名称");
    expect(wrapper.get('[data-testid="prompt-message"]').text()).toBe("留空恢复默认");
    const input = wrapper.get('[data-testid="prompt-input"]');
    expect((input.element as HTMLInputElement).value).toBe("旧名");

    await input.setValue("新名");
    await wrapper.get('[data-testid="prompt-ok"]').trigger("click");
    await expect(result).resolves.toBe("新名");
  });

  it("点击取消返回 null，遮罩点击同样取消", async () => {
    const wrapper = mountHost();
    const first = promptDialog({ title: "x" });
    await flushPromises();
    await wrapper.get('[data-testid="prompt-cancel"]').trigger("click");
    await expect(first).resolves.toBeNull();

    const second = promptDialog({ title: "y" });
    await flushPromises();
    await wrapper.get('[data-testid="prompt-root"]').trigger("click");
    await expect(second).resolves.toBeNull();
  });
});
