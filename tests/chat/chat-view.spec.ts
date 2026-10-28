// Chat 组件测试：消息渲染、发送状态、重发与返回（M2-b）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount, flushPromises } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";

const api = vi.hoisted(() => ({
  sendText: vi.fn(),
  onMessage: vi.fn(),
  onSessionUpdate: vi.fn(),
  isTauri: vi.fn(),
  clearHistory: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  MESSAGE_NEW_EVENT: "message://new",
  SESSION_UPDATE_EVENT: "session://update",
  TRANSFER_UPDATE_EVENT: "transfer://update",
  sendText: api.sendText,
  onMessage: api.onMessage,
  onSessionUpdate: api.onSessionUpdate,
  isTauri: api.isTauri,
  getHistory: vi.fn().mockResolvedValue({ messages: [], transfers: [] }),
  listTransfers: vi.fn(),
  onTransferUpdate: vi.fn(),
  pickFile: vi.fn(),
  sendFile: vi.fn(),
  acceptFile: vi.fn(),
  rejectFile: vi.fn(),
  cancelTransfer: vi.fn(),
  getImagePreview: vi.fn(),
  addDeviceByIp: vi.fn(),
  saveClipboardImage: vi.fn(),
  sendPaths: vi.fn(),
  clearHistory: api.clearHistory,
  openTransferFile: vi.fn(),
  revealTransferFile: vi.fn(),
  saveTransferAs: vi.fn(),
}));

// 必须在 mock 之后导入被测模块。
import Chat from "../../src/views/Chat.vue";
import { useChatStore } from "../../src/stores/chat";

const props = { peerId: "peer-1", peerName: "PC-peer" };

describe("Chat", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("显示会话标题与空状态", () => {
    const wrapper = mount(Chat, { props });
    expect(wrapper.get('[data-testid="chat-title"]').text()).toBe("与 PC-peer 的会话");
    expect(wrapper.text()).toContain("还没有消息");
    expect(wrapper.get('[data-testid="chat-connection"]').text()).toBe("未连接（发送时自动连接）");
  });

  it("只渲染当前对端的消息，并显示发送状态", () => {
    const store = useChatStore();
    store.receive({ peerId: "peer-1", msgId: "m-1", text: "来自对端", ts: 1000 });
    store.messages.push({
      id: "m-2",
      peerId: "peer-1",
      direction: "out",
      text: "本机回复",
      ts: 2000,
      status: "delivered",
    });
    store.receive({ peerId: "peer-2", msgId: "m-3", text: "别的会话", ts: 3000 });
    store.applySession({ kind: "connected", peerId: "peer-1", name: "PC-peer" });

    const wrapper = mount(Chat, { props });
    expect(wrapper.findAll('[data-testid="message-in"]')).toHaveLength(1);
    expect(wrapper.findAll('[data-testid="message-out"]')).toHaveLength(1);
    expect(wrapper.text()).toContain("来自对端");
    expect(wrapper.text()).toContain("本机回复");
    expect(wrapper.text()).not.toContain("别的会话");
    expect(wrapper.text()).toContain("已送达");
    expect(wrapper.get('[data-testid="chat-connection"]').text()).toBe("已连接");
  });

  it("发送失败的消息显示重发按钮并可重试", async () => {
    const store = useChatStore();
    store.messages.push({
      id: "m-1",
      peerId: "peer-1",
      direction: "out",
      text: "hi",
      ts: 1000,
      status: "failed",
    });
    const retry = vi.spyOn(store, "retry").mockResolvedValue(true);

    const wrapper = mount(Chat, { props });
    expect(wrapper.text()).toContain("发送失败");

    await wrapper.get('[data-testid="chat-retry"]').trigger("click");
    expect(retry).toHaveBeenCalledWith("m-1");
  });

  it("点击发送调用 store.send 并清空输入框", async () => {
    const store = useChatStore();
    const send = vi.spyOn(store, "send").mockResolvedValue(true);

    const wrapper = mount(Chat, { props });
    const input = wrapper.get('[data-testid="chat-input"]');
    await input.setValue("你好");

    await wrapper.get("form").trigger("submit");
    await flushPromises();

    expect(send).toHaveBeenCalledWith("peer-1", "你好");
    expect((input.element as HTMLTextAreaElement).value).toBe("");
  });

  it("空白输入不会触发发送", async () => {
    const store = useChatStore();
    const send = vi.spyOn(store, "send").mockResolvedValue(true);

    const wrapper = mount(Chat, { props });
    await wrapper.get("form").trigger("submit");

    expect(send).not.toHaveBeenCalled();
    expect(wrapper.get('[data-testid="chat-send"]').attributes("disabled")).toBeDefined();
  });

  it("返回按钮向父组件发出 back 事件", async () => {
    const wrapper = mount(Chat, { props });
    await wrapper.get('[data-testid="chat-back"]').trigger("click");
    expect(wrapper.emitted("back")).toHaveLength(1);
  });

  it("清空本会话记录调用后端并清空本地状态（F40）", async () => {
    const store = useChatStore();
    store.receive({ peerId: "peer-1", msgId: "m-1", text: "hi", ts: 1000 });
    const wrapper = mount(Chat, { props });

    await wrapper.get('[data-testid="chat-clear"]').trigger("click");
    await flushPromises();

    expect(api.clearHistory).toHaveBeenCalledWith("peer-1");
    expect(store.messagesFor("peer-1")).toHaveLength(0);
    expect(wrapper.find('[data-testid="chat-cleared"]').exists()).toBe(true);
  });

  it("store 错误以中文提示展示", () => {
    const store = useChatStore();
    store.error = "peer not found";
    const wrapper = mount(Chat, { props });
    expect(wrapper.get('[data-testid="chat-error"]').text()).toBe("发送失败：peer not found");
  });
});
