// TransferCard 组件测试：进度、状态、确认/取消动作与图片预览（M3-a）。
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import TransferCard from "../../src/components/TransferCard.vue";
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
  ...overrides,
});

describe("TransferCard", () => {
  it("显示文件名、方向、状态与百分比", () => {
    const wrapper = mount(TransferCard, { props: { transfer: transfer() } });
    expect(wrapper.get('[data-testid="transfer-name"]').text()).toBe("photo.png");
    expect(wrapper.text()).toContain("接收");
    expect(wrapper.text()).toContain("传输中");
    expect(wrapper.get('[data-testid="transfer-percent"]').text()).toBe("25%");
  });

  it("待确认任务显示接受与拒绝按钮，并携带任务 ID 抛事件", async () => {
    const wrapper = mount(TransferCard, { props: { transfer: transfer({ state: "pending" }) } });
    await wrapper.get('[data-testid="transfer-accept"]').trigger("click");
    await wrapper.get('[data-testid="transfer-reject"]').trigger("click");

    expect(wrapper.emitted("accept")).toEqual([["t-1"]]);
    expect(wrapper.emitted("reject")).toEqual([["t-1"]]);
  });

  it("进行中的任务显示取消按钮，完成后不再显示", async () => {
    const wrapper = mount(TransferCard, { props: { transfer: transfer() } });
    await wrapper.get('[data-testid="transfer-cancel"]').trigger("click");
    expect(wrapper.emitted("cancel")).toEqual([["t-1"]]);

    await wrapper.setProps({ transfer: transfer({ state: "done", transferred: 200 }) });
    expect(wrapper.find('[data-testid="transfer-cancel"]').exists()).toBe(false);
  });

  it("已完成的图片渲染预览，缺失预览时给出提示", async () => {
    const done = transfer({ state: "done", transferred: 200, path: "C:/dl/photo.png" });
    const wrapper = mount(TransferCard, {
      props: { transfer: done, previewUrl: "data:image/png;base64,AAA" },
    });
    const img = wrapper.get('[data-testid="transfer-image"]');
    expect(img.attributes("src")).toBe("data:image/png;base64,AAA");
    expect(img.attributes("alt")).toBe("photo.png");

    await wrapper.setProps({ previewUrl: undefined });
    expect(wrapper.find('[data-testid="transfer-image"]').exists()).toBe(false);
    expect(wrapper.get('[data-testid="transfer-preview-unavailable"]').text()).toBe(
      "图片预览不可用",
    );
  });

  it("失败任务展示原因，且不显示取消按钮", () => {
    const wrapper = mount(TransferCard, {
      props: { transfer: transfer({ state: "failed", error: "哈希校验失败" }) },
    });
    expect(wrapper.get('[data-testid="transfer-error"]').text()).toBe("哈希校验失败");
    expect(wrapper.find('[data-testid="transfer-cancel"]').exists()).toBe(false);
  });

  it("失败的发送任务显示重试按钮并抛出 retry 事件（M3-b）", async () => {
    const wrapper = mount(TransferCard, {
      props: {
        transfer: transfer({ direction: "send", state: "failed", error: "连接已断开，可重试" }),
      },
    });
    await wrapper.get('[data-testid="transfer-retry"]').trigger("click");
    expect(wrapper.emitted("retry")).toEqual([["t-1"]]);

    // 接收方向的失败任务不能重试（源文件在对方）。
    await wrapper.setProps({ transfer: transfer({ direction: "receive", state: "failed" }) });
    expect(wrapper.find('[data-testid="transfer-retry"]').exists()).toBe(false);
  });

  it("已完成的接收任务显示打开/定位/另存为按钮（F23）", async () => {
    const wrapper = mount(TransferCard, {
      props: {
        transfer: transfer({
          direction: "receive",
          kind: "file",
          state: "done",
          transferred: 200,
          path: "C:/dl/a.zip",
        }),
      },
    });

    await wrapper.get('[data-testid="transfer-open"]').trigger("click");
    await wrapper.get('[data-testid="transfer-reveal"]').trigger("click");
    await wrapper.get('[data-testid="transfer-save-as"]').trigger("click");

    expect(wrapper.emitted("open")).toEqual([["t-1"]]);
    expect(wrapper.emitted("reveal")).toEqual([["t-1"]]);
    expect(wrapper.emitted("saveAs")).toEqual([["t-1"]]);
  });

  it("发送方向完成的任务不显示打开/另存为", () => {
    const wrapper = mount(TransferCard, {
      props: { transfer: transfer({ direction: "send", state: "done", transferred: 200 }) },
    });
    expect(wrapper.find('[data-testid="transfer-open"]').exists()).toBe(false);
  });

  it("零字节任务完成时按 100% 显示，不出现除零", () => {
    const wrapper = mount(TransferCard, {
      props: { transfer: transfer({ size: 0, transferred: 0, state: "done" }) },
    });
    expect(wrapper.get('[data-testid="transfer-percent"]').text()).toBe("100%");
  });
});
