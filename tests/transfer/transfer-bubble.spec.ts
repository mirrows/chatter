// TransferBubble 组件测试（v3）：120px 方形、按钮矩阵、进度仅传输中、图片点击预览。
import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import TransferBubble from "../../src/components/TransferBubble.vue";
import type { TransferInfo } from "../../src/api/types";

const transfer = (overrides: Partial<TransferInfo> = {}): TransferInfo => ({
  transferId: "t-1",
  peerId: "peer-1",
  direction: "receive",
  kind: "image",
  filename: "photo.png",
  size: 200,
  transferred: 50,
  state: "transferring",
  path: null,
  error: null,
  ts: 1_700_000_000_000,
  ...overrides,
});

describe("TransferBubble", () => {
  it("图片气泡不显示文件名，有预览时渲染缩略图", () => {
    const wrapper = mount(TransferBubble, {
      props: { transfer: transfer(), previewUrl: "data:image/png;base64,AAA" },
    });
    expect(wrapper.get('[data-testid="transfer-image"]').attributes("src")).toBe(
      "data:image/png;base64,AAA",
    );
    expect(wrapper.find('[data-testid="transfer-name"]').exists()).toBe(false);
  });

  it("点击图片抛出 preview 事件（供全局预览弹窗）", async () => {
    const wrapper = mount(TransferBubble, {
      props: { transfer: transfer(), previewUrl: "data:image/png;base64,AAA" },
    });
    await wrapper.get('[data-testid="transfer-box"]').trigger("click");
    expect(wrapper.emitted("preview")).toEqual([["data:image/png;base64,AAA"]]);
  });

  it("进度条与百分比只在传输中显示，完成后隐藏（需求 5）", async () => {
    const wrapper = mount(TransferBubble, { props: { transfer: transfer() } });
    expect(wrapper.get('[data-testid="transfer-percent"]').text()).toBe("25%");

    await wrapper.setProps({ transfer: transfer({ state: "done", transferred: 200 }) });
    expect(wrapper.find('[data-testid="transfer-progress"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="transfer-percent"]').exists()).toBe(false);
    // 完成后不展示任何状态文案
    expect(wrapper.text()).not.toContain("已完成");
    expect(wrapper.text()).not.toContain("接收");
  });

  it("接收方 pending 显示接收/取消，点击抛事件", async () => {
    const wrapper = mount(TransferBubble, {
      props: { transfer: transfer({ state: "pending", transferred: 0 }) },
    });
    await wrapper.get('[data-testid="transfer-accept"]').trigger("click");
    await wrapper.get('[data-testid="transfer-reject"]').trigger("click");
    expect(wrapper.emitted("accept")).toEqual([["t-1"]]);
    expect(wrapper.emitted("reject")).toEqual([["t-1"]]);
  });

  it("接收完成不再有打开/另存为/打开文件夹常驻按钮", () => {
    const wrapper = mount(TransferBubble, {
      props: {
        transfer: transfer({ state: "done", kind: "file", transferred: 200, path: "C:/dl/a.zip" }),
      },
    });
    expect(wrapper.find('[data-testid="transfer-open"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="transfer-save-as"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="transfer-reveal"]').exists()).toBe(false);
  });

  it("发送方等待接收显示标签，失败时左侧显示重试", async () => {
    const waiting = mount(TransferBubble, {
      props: { transfer: transfer({ direction: "send", state: "offered", transferred: 0 }) },
    });
    expect(waiting.get('[data-testid="transfer-waiting"]').text()).toBe("待接收");

    const failed = mount(TransferBubble, {
      props: { transfer: transfer({ direction: "send", state: "failed", transferred: 0 }) },
    });
    await failed.get('[data-testid="transfer-retry"]').trigger("click");
    expect(failed.emitted("retry")).toEqual([["t-1"]]);
  });

  it("文件气泡显示中间省略且保留后缀的文件名", () => {
    const wrapper = mount(TransferBubble, {
      props: {
        transfer: transfer({
          kind: "file",
          filename: "2026 年度总结报告最终版.pdf",
          state: "done",
          transferred: 200,
        }),
      },
    });
    const name = wrapper.get('[data-testid="transfer-name"]').text();
    expect(name).toContain("…");
    expect(name.endsWith(".pdf")).toBe(true);
    expect(wrapper.get('[data-testid="transfer-box"]').attributes("title")).toContain(
      "2026 年度总结报告最终版.pdf",
    );
  });

  it("右键抛出鼠标位置菜单事件", async () => {
    const wrapper = mount(TransferBubble, { props: { transfer: transfer({ state: "done" }) } });
    await wrapper.get('[data-testid="transfer-box"]').trigger("contextmenu", {
      clientX: 120,
      clientY: 80,
    });
    expect(wrapper.emitted("menu")).toEqual([[{ x: 120, y: 80, centered: false }]]);
  });

  it("移动端长按 500ms 抛出居中菜单事件", async () => {
    vi.useFakeTimers();
    try {
      const wrapper = mount(TransferBubble, { props: { transfer: transfer({ state: "done" }) } });
      const box = wrapper.get('[data-testid="transfer-box"]');
      await box.trigger("touchstart", { touches: [{ clientX: 10, clientY: 10 }] });
      vi.advanceTimersByTime(520);
      expect(wrapper.emitted("menu")).toEqual([[{ x: 10, y: 10, centered: true }]]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("已发送的文件点开时请求用本地源文件打开（v3）", async () => {
    const wrapper = mount(TransferBubble, {
      props: {
        transfer: transfer({ direction: "send", kind: "file", state: "done", transferred: 200 }),
      },
    });
    await wrapper.get('[data-testid="transfer-box"]').trigger("click");
    expect(wrapper.emitted("open")).toEqual([[]]);
    expect(wrapper.emitted("unavailable")).toBeUndefined();
  });

  it("已接收的文件点开时同样请求打开（v3）", async () => {
    const wrapper = mount(TransferBubble, {
      props: {
        transfer: transfer({ direction: "receive", kind: "file", state: "done", transferred: 200 }),
      },
    });
    await wrapper.get('[data-testid="transfer-box"]').trigger("click");
    expect(wrapper.emitted("open")).toEqual([[]]);
  });

  it("未完成的文件点击不触发打开（v3 边界）", async () => {
    const wrapper = mount(TransferBubble, {
      props: {
        transfer: transfer({ direction: "send", kind: "file", state: "transferring" }),
      },
    });
    await wrapper.get('[data-testid="transfer-box"]').trigger("click");
    expect(wrapper.emitted("open")).toBeUndefined();
  });

  it("失败/拒绝：发送方可点开源文件，接收方不可点（v3）", async () => {
    const senderFailed = mount(TransferBubble, {
      props: { transfer: transfer({ direction: "send", kind: "file", state: "failed" }) },
    });
    await senderFailed.get('[data-testid="transfer-box"]').trigger("click");
    expect(senderFailed.emitted("open")).toEqual([[]]);

    const senderRejected = mount(TransferBubble, {
      props: { transfer: transfer({ direction: "send", kind: "file", state: "rejected" }) },
    });
    await senderRejected.get('[data-testid="transfer-box"]').trigger("click");
    expect(senderRejected.emitted("open")).toEqual([[]]);

    const receiverFailed = mount(TransferBubble, {
      props: { transfer: transfer({ direction: "receive", kind: "file", state: "failed" }) },
    });
    await receiverFailed.get('[data-testid="transfer-box"]').trigger("click");
    expect(receiverFailed.emitted("open")).toBeUndefined();
    // 不可点的气泡加 static 类，去掉 hover 反馈
    expect(receiverFailed.get('[data-testid="transfer-box"]').classes()).toContain("static");
  });

  it("hideRetry 时失败任务不再显示重试（重启后，v3）", () => {
    const wrapper = mount(TransferBubble, {
      props: {
        transfer: transfer({ direction: "send", state: "failed" }),
        hideRetry: true,
      },
    });
    expect(wrapper.find('[data-testid="transfer-retry"]').exists()).toBe(false);
  });
});
it("touchOnly（移动端）时右键不触发菜单，只走长按（v3）", async () => {
  const wrapper = mount(TransferBubble, {
    props: { transfer: transfer({ state: "done" }), touchOnly: true },
  });
  await wrapper
    .get('[data-testid="transfer-box"]')
    .trigger("contextmenu", { clientX: 5, clientY: 5 });
  expect(wrapper.emitted("menu")).toBeUndefined();
});
