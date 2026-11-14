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
  getHistoryPage: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  MESSAGE_NEW_EVENT: "message://new",
  SESSION_UPDATE_EVENT: "session://update",
  TRANSFER_UPDATE_EVENT: "transfer://update",
  sendText: api.sendText,
  onMessage: api.onMessage,
  onSessionUpdate: api.onSessionUpdate,
  isTauri: api.isTauri,
  listDevices: vi.fn().mockResolvedValue([]),
  getLocalDevice: vi.fn(),
  onDeviceUpdate: vi.fn(),
  refreshDevices: vi.fn(),
  getHistory: vi.fn().mockResolvedValue({ messages: [], transfers: [] }),
  getHistoryPage: api.getHistoryPage,
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

// 剪贴板单独 mock，便于断言「复制」动作（jsdom 无 navigator.clipboard）。
const clipboard = vi.hoisted(() => ({ copyText: vi.fn() }));
vi.mock("../../src/utils/clipboard", () => ({ copyText: clipboard.copyText }));

// 必须在 mock 之后导入被测模块。
import Chat from "../../src/views/Chat.vue";
import { useChatStore } from "../../src/stores/chat";
import { useContextMenuStore } from "../../src/stores/contextMenu";
import { useDeviceStore } from "../../src/stores/devices";
import { useMessageStore } from "../../src/stores/message";
import { useTransferStore } from "../../src/stores/transfers";

const props = { peerId: "peer-1", peerName: "PC-peer" };

describe("Chat", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    // 默认：首屏分页为空（各用例可覆盖）
    api.getHistoryPage.mockResolvedValue({ items: [], hasMore: false });
  });

  it("显示会话标题与空状态", () => {
    const wrapper = mount(Chat, { props });
    expect(wrapper.get('[data-testid="chat-title"]').text()).toBe("与 PC-peer 的会话");
    expect(wrapper.text()).toContain("还没有消息");
    expect(wrapper.get('[data-testid="chat-connection"]').text()).toBe("未连接（发送时自动连接）");
  });

  it("重连中显示尝试次数（v2/M10）", () => {
    const store = useChatStore();
    store.applySession({ kind: "reconnecting", peerId: "peer-1", attempt: 2 });
    const wrapper = mount(Chat, { props });
    expect(wrapper.get('[data-testid="chat-connection"]').text()).toBe("正在重连…（第 2 次）");
  });

  it("重连失败后回到未连接状态（v2/M10）", () => {
    const store = useChatStore();
    store.applySession({ kind: "reconnecting", peerId: "peer-1", attempt: 3 });
    store.applySession({ kind: "reconnectFailed", peerId: "peer-1" });
    const wrapper = mount(Chat, { props });
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
    expect(wrapper.find('[data-testid="chat-notice-cleared"]').exists()).toBe(true);
  });

  it("store 错误以中文提示展示", () => {
    const store = useChatStore();
    store.error = "peer not found";
    const wrapper = mount(Chat, { props });
    expect(wrapper.get('[data-testid="chat-notice-error"]').text()).toContain(
      "发送失败：peer not found",
    );
  });

  it("文本与传输任务按时间戳混排（v3 需求 2）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];

    store.messages.push({
      id: "m-1",
      peerId: "peer-1",
      direction: "in",
      text: "先到的文本",
      ts: 100,
      status: "delivered",
    });
    store.messages.push({
      id: "m-2",
      peerId: "peer-1",
      direction: "in",
      text: "后到的文本",
      ts: 300,
      status: "delivered",
    });
    transfers.upsert({
      transferId: "t-1",
      peerId: "peer-1",
      direction: "send",
      kind: "image",
      filename: "a.png",
      size: 10,
      transferred: 10,
      state: "done",
      path: null,
      error: null,
      ts: 200,
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    const order = wrapper
      .findAll('[data-testid="chat-timeline"] > *')
      .map((node) => node.attributes("data-testid") ?? "")
      .filter((id) => !id.startsWith("chat-history-"));
    expect(order).toEqual(["message-in", "transfer-done", "message-in"]);

    const texts = wrapper.findAll('[data-testid="chat-timeline"] .text').map((n) => n.text());
    expect(texts).toEqual(["先到的文本", "后到的文本"]);
  });

  it("传输失败时显示错误并可关闭（v2/M9）", async () => {
    const transfers = useTransferStore();
    transfers.error = "unsupported file path";
    const wrapper = mount(Chat, { props });

    expect(wrapper.get('[data-testid="chat-notice-transfer"]').text()).toContain(
      "操作失败：unsupported file path",
    );

    await wrapper.get('[data-testid="chat-notice-close"]').trigger("click");
    expect(transfers.error).toBe("");
  });
  it("接收完成的图片：气泡无按钮，菜单只有「另存为」（需求 3）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    const menu = useContextMenuStore();
    store.messages = [];
    transfers.items = [];
    transfers.upsert({
      transferId: "t-img",
      peerId: "peer-1",
      direction: "receive",
      kind: "image",
      filename: "photo.png",
      size: 10,
      transferred: 10,
      state: "done",
      path: "C:/dl/photo.png",
      error: null,
      ts: 100,
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();
    // 气泡内不再有任何常驻操作按钮
    expect(wrapper.find('[data-testid="transfer-open"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="transfer-save-as"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="transfer-reveal"]').exists()).toBe(false);

    await wrapper
      .get('[data-testid="transfer-box"]')
      .trigger("contextmenu", { clientX: 10, clientY: 10 });
    expect(menu.items.map((item) => item.key)).toEqual(["saveAs"]);
  });

  it("接收完成的文件：菜单为「打开文件 + 另存为」（需求 4）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    const menu = useContextMenuStore();
    store.messages = [];
    transfers.items = [];
    transfers.upsert({
      transferId: "t-file",
      peerId: "peer-1",
      direction: "receive",
      kind: "file",
      filename: "a.zip",
      size: 10,
      transferred: 10,
      state: "done",
      path: "C:/dl/a.zip",
      error: null,
      ts: 100,
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    await wrapper
      .get('[data-testid="transfer-box"]')
      .trigger("contextmenu", { clientX: 10, clientY: 10 });
    expect(menu.items.map((item) => item.key)).toEqual(["open", "saveAs"]);
  });
  it("输入框占位文案区分平台：移动端不显示 Ctrl+Enter（v3 需求 3）", async () => {
    const devices = useDeviceStore();
    const base = {
      deviceId: "self",
      name: "self",
      discoveryPort: 28100,
      tcpPort: 28101,
      fingerprint: "1111-2222-3333-4444",
    };

    devices.local = { ...base, platform: "android" };
    const mobile = mount(Chat, { props });
    expect(mobile.get('[data-testid="chat-input"]').attributes("placeholder")).toBe("输入消息");

    devices.local = { ...base, platform: "windows" };
    const desktop = mount(Chat, { props });
    expect(desktop.get('[data-testid="chat-input"]').attributes("placeholder")).toBe(
      "输入消息，Ctrl+Enter 发送",
    );
  });

  it("提示浮层可手动关闭，且关闭后切换到下一条（v3 需求 2）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];
    store.error = "peer not found";
    transfers.error = "unsupported file path";

    const wrapper = mount(Chat, { props });
    await flushPromises();

    // 第一条（会话错误）优先展示
    expect(wrapper.find('[data-testid="chat-notice-error"]').exists()).toBe(true);
    await wrapper.get('[data-testid="chat-notice-close"]').trigger("click");
    await flushPromises();
    expect(store.error).toBe("");
    // 关闭后展示下一条（传输错误）
    expect(wrapper.find('[data-testid="chat-notice-transfer"]').exists()).toBe(true);
  });
});

// v3 追加用例：独立 describe，复用与上面一致的初始化（pinia + IPC mock 默认值）。
describe("Chat（v3 追加用例）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    api.getHistoryPage.mockResolvedValue({ items: [], hasMore: false });
  });

  it("新消息进入时间线后自动滚动到底部（v3）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];
    store.messages.push({
      id: "m-1",
      peerId: "peer-1",
      direction: "in",
      text: "hi",
      ts: 1,
      status: "delivered",
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    const list = wrapper.get('[data-testid="chat-timeline"]').element as HTMLElement;
    Object.defineProperty(list, "scrollHeight", { value: 640, configurable: true });

    store.messages.push({
      id: "m-2",
      peerId: "peer-1",
      direction: "in",
      text: "again",
      ts: 2,
      status: "delivered",
    });
    await flushPromises();

    expect(list.scrollTop).toBe(640);
  });
  it("新收到的文件在「询问接收」时置底一次，之后状态推进不再影响滚动（v3）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];
    store.messages.push({
      id: "m-1",
      peerId: "peer-1",
      direction: "in",
      text: "hi",
      ts: 1,
      status: "delivered",
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    const list = wrapper.get('[data-testid="chat-timeline"]').element as HTMLElement;
    Object.defineProperty(list, "scrollHeight", { value: 800, configurable: true });

    // 1) 文件到达（pending：展示接受/取消）→ 新增条目 → 置底
    transfers.upsert({
      transferId: "t-1",
      peerId: "peer-1",
      direction: "receive",
      kind: "file",
      filename: "a.zip",
      size: 100,
      transferred: 0,
      state: "pending",
      path: null,
      error: null,
      ts: 2,
    });
    await flushPromises();
    expect(list.scrollTop).toBe(800);

    // 2) 用户手动上滑查看历史
    list.scrollTop = 120;

    // 3) 同一任务推进（接受 → 传输中 → 完成）不应改变滚动位置
    for (const state of ["transferring", "done"] as const) {
      transfers.upsert({
        transferId: "t-1",
        peerId: "peer-1",
        direction: "receive",
        kind: "file",
        filename: "a.zip",
        size: 100,
        transferred: state === "done" ? 100 : 50,
        state,
        path: state === "done" ? "C:/dl/a.zip" : null,
        error: null,
        ts: 2,
      });
      await flushPromises();
      expect(list.scrollTop).toBe(120);
    }
  });
  it("上滑翻历史时收到新消息：不置底，改为显示「以下为新消息」分割线（v3）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];
    store.messages.push({
      id: "m-1",
      peerId: "peer-1",
      direction: "in",
      text: "history",
      ts: 1,
      status: "delivered",
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    const list = wrapper.get('[data-testid="chat-timeline"]').element as HTMLElement;
    Object.defineProperty(list, "scrollHeight", { value: 800, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 400, configurable: true });

    // 用户滚到中间（离开底部）
    list.scrollTop = 200;
    list.dispatchEvent(new Event("scroll"));
    await flushPromises();

    store.messages.push({
      id: "m-2",
      peerId: "peer-1",
      direction: "in",
      text: "new",
      ts: 2,
      status: "delivered",
    });
    await flushPromises();

    // 不打断滚动
    expect(list.scrollTop).toBe(200);
    // 分割线出现在新内容之前
    const ids = wrapper
      .findAll('[data-testid="chat-timeline"] > *')
      .map((node) => node.attributes("data-testid") ?? "")
      .filter((id) => !id.startsWith("chat-history-"));
    expect(ids).toEqual(["message-in", "chat-new-divider", "message-in"]);

    // 继续往上滑：旧分割线消失
    list.scrollTop = 100;
    list.dispatchEvent(new Event("scroll"));
    await flushPromises();
    expect(wrapper.find('[data-testid="chat-new-divider"]').exists()).toBe(false);

    // 回到底部：恢复自动置底
    list.scrollTop = 400;
    list.dispatchEvent(new Event("scroll"));
    await flushPromises();
    store.messages.push({
      id: "m-3",
      peerId: "peer-1",
      direction: "in",
      text: "newer",
      ts: 3,
      status: "delivered",
    });
    await flushPromises();
    expect(list.scrollTop).toBe(800);
  });

  it("上滑翻历史时收到文件：仍强制置底展示接受/取消，且不显示分割线（v3）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];
    store.messages.push({
      id: "m-1",
      peerId: "peer-1",
      direction: "in",
      text: "history",
      ts: 1,
      status: "delivered",
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    const list = wrapper.get('[data-testid="chat-timeline"]').element as HTMLElement;
    Object.defineProperty(list, "scrollHeight", { value: 800, configurable: true });
    Object.defineProperty(list, "clientHeight", { value: 400, configurable: true });

    list.scrollTop = 200;
    list.dispatchEvent(new Event("scroll"));
    await flushPromises();

    transfers.upsert({
      transferId: "t-file",
      peerId: "peer-1",
      direction: "receive",
      kind: "file",
      filename: "a.zip",
      size: 100,
      transferred: 0,
      state: "pending",
      path: null,
      error: null,
      ts: 2,
    });
    await flushPromises();

    expect(list.scrollTop).toBe(800);
    expect(wrapper.find('[data-testid="chat-new-divider"]').exists()).toBe(false);
  });
  it("打开会话按 100 条分页拉取历史（v3 无限滚动）", async () => {
    mount(Chat, { props });
    await flushPromises();

    expect(api.getHistoryPage).toHaveBeenCalledWith("peer-1", undefined, 100);
  });

  it("滚到顶部加载更早一页，并保持视口位置（v3 无限滚动）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];

    // 首屏返回一条较新的消息，且还有更早记录
    api.getHistoryPage.mockResolvedValueOnce({
      items: [
        {
          itemType: "message",
          msgId: "m-2",
          peerId: "peer-1",
          direction: "in",
          text: "new",
          ts: 200,
          status: "delivered",
        },
      ],
      hasMore: true,
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    const list = wrapper.get('[data-testid="chat-timeline"]').element as HTMLElement;
    // 高度随消息数增长：1 条 600px、2 条 700px
    Object.defineProperty(list, "scrollHeight", {
      get: () => 500 + store.messages.length * 100,
      configurable: true,
    });
    Object.defineProperty(list, "clientHeight", { value: 400, configurable: true });

    // 向上翻页返回更早一页，且没有更早记录
    api.getHistoryPage.mockResolvedValueOnce({
      items: [
        {
          itemType: "message",
          msgId: "m-1",
          peerId: "peer-1",
          direction: "in",
          text: "old",
          ts: 100,
          status: "delivered",
        },
      ],
      hasMore: false,
    });

    // 滚到顶部触发加载
    list.scrollTop = 0;
    list.dispatchEvent(new Event("scroll"));
    await flushPromises();

    // 游标是最老条目的 ts
    expect(api.getHistoryPage).toHaveBeenLastCalledWith("peer-1", 200, 100);
    expect(store.messagesFor("peer-1").map((message) => message.id)).toEqual(["m-1", "m-2"]);
    // 视口保持：新高度 700 - 旧高度 600 + 旧 top 0 = 100
    expect(list.scrollTop).toBe(100);
    // 已到最早：顶部提示「没有更多了」
    expect(wrapper.find('[data-testid="chat-history-end"]').exists()).toBe(true);
  });
  it("清空会话后不再向上加载旧记录（后端已删除，避免翻回已清空内容，v3）", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    store.messages = [];
    transfers.items = [];

    // 首屏返回一条消息，且「还有更早记录」
    api.getHistoryPage.mockResolvedValueOnce({
      items: [
        {
          itemType: "message",
          msgId: "m-1",
          peerId: "peer-1",
          direction: "in",
          text: "old",
          ts: 100,
          status: "delivered",
        },
      ],
      hasMore: true,
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();
    expect(api.getHistoryPage).toHaveBeenCalledTimes(1);

    // 清空本会话（后端会真正删除该对端记录）
    api.clearHistory.mockResolvedValue(0);
    await wrapper.get('[data-testid="chat-clear"]').trigger("click");
    await flushPromises();
    expect(store.messagesFor("peer-1")).toHaveLength(0);

    // 清空后来一条新消息并滚到顶部：不应再请求更早的分页
    store.messages.push({
      id: "m-2",
      peerId: "peer-1",
      direction: "in",
      text: "after clear",
      ts: 500,
      status: "delivered",
    });
    await flushPromises();

    const list = wrapper.get('[data-testid="chat-timeline"]').element as HTMLElement;
    list.scrollTop = 0;
    list.dispatchEvent(new Event("scroll"));
    await flushPromises();

    expect(api.getHistoryPage).toHaveBeenCalledTimes(1);
    // 清空后只剩新消息
    expect(store.messagesFor("peer-1").map((message) => message.id)).toEqual(["m-2"]);
  });
});

describe("Chat 文本复制（v3 需求 8）", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    api.getHistoryPage.mockResolvedValue({ items: [], hasMore: false });
    clipboard.copyText.mockResolvedValue(true);
  });

  it("文本气泡右键菜单只有「复制」，复制成功弹提示", async () => {
    const store = useChatStore();
    const transfers = useTransferStore();
    const menu = useContextMenuStore();
    const messages = useMessageStore();
    store.messages = [];
    transfers.items = [];
    store.messages.push({
      id: "m-1",
      peerId: "peer-1",
      direction: "in",
      text: "hello",
      ts: 1,
      status: "delivered",
    });

    const wrapper = mount(Chat, { props });
    await flushPromises();

    // 气泡内不再有复制按钮
    expect(wrapper.find('[data-testid="message-copy"]').exists()).toBe(false);

    await wrapper
      .get('[data-testid="message-in"]')
      .trigger("contextmenu", { clientX: 20, clientY: 30 });
    expect(menu.items.map((item) => item.key)).toEqual(["copy"]);

    await menu.select(menu.items[0]);
    await flushPromises();

    expect(clipboard.copyText).toHaveBeenCalledWith("hello");
    expect(messages.visible[messages.visible.length - 1]?.text).toBe("已复制");
  });
});
