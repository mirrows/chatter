// 聊天 store 单测：乐观发送、状态更新、接收与重发（对应 chat.ts）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import type { IncomingMessage, SessionUpdate } from "../../src/api/types";

const api = vi.hoisted(() => ({
  sendText: vi.fn(),
  resendText: vi.fn(),
  onMessage: vi.fn(),
  onSessionUpdate: vi.fn(),
  isTauri: vi.fn(),
  refreshDevices: vi.fn(),
  listDevices: vi.fn(),
  getLocalDevice: vi.fn(),
  onDeviceUpdate: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  MESSAGE_NEW_EVENT: "message://new",
  SESSION_UPDATE_EVENT: "session://update",
  TRANSFER_UPDATE_EVENT: "transfer://update",
  sendText: api.sendText,
  resendText: api.resendText,
  onMessage: api.onMessage,
  onSessionUpdate: api.onSessionUpdate,
  isTauri: api.isTauri,
  refreshDevices: api.refreshDevices,
  listDevices: api.listDevices,
  getLocalDevice: api.getLocalDevice,
  onDeviceUpdate: api.onDeviceUpdate,
  getHistory: vi.fn().mockResolvedValue({ messages: [], transfers: [] }),
  listTransfers: vi.fn(),
  onTransferUpdate: vi.fn(),
  pickFile: vi.fn(),
  sendFile: vi.fn(),
  acceptFile: vi.fn(),
  rejectFile: vi.fn(),
  cancelTransfer: vi.fn(),
  getImagePath: vi.fn(),
  getImagePreview: vi.fn(),
  toAssetUrl: vi.fn((path: string) => path),
  addDeviceByIp: vi.fn(),
  saveClipboardImage: vi.fn(),
  sendPaths: vi.fn(),
}));

// 必须在 mock 之后导入被测模块。
import { useChatStore } from "../../src/stores/chat";
import { useDeviceStore } from "../../src/stores/devices";

const incoming = (msgId: string, text: string, ts: number): IncomingMessage => ({
  peerId: "peer-1",
  msgId,
  text,
  ts,
});

describe("chat store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("send 先插入发送中消息，收到回执后更新为已送达", async () => {
    api.sendText.mockResolvedValue({ msgId: "m-1", status: "delivered" });
    const store = useChatStore();

    const pending = store.send("peer-1", "  你好  ");
    expect(store.messagesFor("peer-1")[0].status).toBe("sending");
    expect(store.messagesFor("peer-1")[0].text).toBe("你好");

    await expect(pending).resolves.toBe(true);
    const message = store.messagesFor("peer-1")[0];
    expect(message.id).toBe("m-1");
    expect(message.status).toBe("delivered");
    expect(api.sendText).toHaveBeenCalledWith("peer-1", "你好");
  });

  it("后端返回 failed 时消息标记为发送失败", async () => {
    api.sendText.mockResolvedValue({ msgId: "m-2", status: "failed" });
    const store = useChatStore();

    await expect(store.send("peer-1", "hello")).resolves.toBe(false);
    expect(store.messagesFor("peer-1")[0].status).toBe("failed");
  });

  it("retry 复用原 msg_id（v3）：不新增消息、id 与时间戳不变", async () => {
    api.sendText.mockResolvedValue({ msgId: "m-1", status: "failed" });
    api.resendText.mockResolvedValue({ msgId: "m-1", status: "delivered" });
    const store = useChatStore();

    await store.send("peer-1", "hello");
    const before = store.messagesFor("peer-1")[0];
    expect(before.status).toBe("failed");
    const beforeTs = before.ts;

    await expect(store.retry("m-1")).resolves.toBe(true);

    const after = store.messagesFor("peer-1");
    expect(after).toHaveLength(1);
    expect(after[0].id).toBe("m-1");
    expect(after[0].status).toBe("delivered");
    expect(after[0].ts).toBe(beforeTs);
    expect(api.resendText).toHaveBeenCalledWith("m-1");
    // 不再调用 send_text，避免历史里产生第二条记录
    expect(api.sendText).toHaveBeenCalledTimes(1);
  });

  it("收到同一 msg_id 的重复消息只保留一条（v3）", () => {
    const store = useChatStore();
    store.receive({ peerId: "peer-1", msgId: "m-1", text: "hi", ts: 1 });
    store.receive({ peerId: "peer-1", msgId: "m-1", text: "hi", ts: 1 });

    expect(store.messagesFor("peer-1")).toHaveLength(1);
    expect(store.unreadFor("peer-1")).toBe(1);
  });

  it("调用失败时保留消息并标记失败，记录错误原因", async () => {
    api.sendText.mockRejectedValue(new Error("peer not found"));
    const store = useChatStore();

    await expect(store.send("peer-1", "hello")).resolves.toBe(false);
    expect(store.messagesFor("peer-1")[0].status).toBe("failed");
    expect(store.error).toBe("peer not found");
  });

  it("空消息不发送也不产生气泡", async () => {
    const store = useChatStore();
    await expect(store.send("peer-1", "   ")).resolves.toBe(false);
    expect(store.messagesFor("peer-1")).toHaveLength(0);
    expect(api.sendText).not.toHaveBeenCalled();
  });

  it("receive 按时间排序并只归属对应设备", () => {
    const store = useChatStore();
    store.receive(incoming("m-2", "第二条", 2000));
    store.receive(incoming("m-1", "第一条", 1000));

    const messages = store.messagesFor("peer-1");
    expect(messages.map((m) => m.id)).toEqual(["m-1", "m-2"]);
    expect(messages[0].direction).toBe("in");
    expect(store.messagesFor("peer-2")).toHaveLength(0);
  });

  it("接收未知对端消息后由 device://update 补登记，消息与未读保留（F3 入站）", () => {
    const chat = useChatStore();
    const devices = useDeviceStore();

    chat.receive({ peerId: "peer-x", msgId: "m-x", text: "hi", ts: 1 });
    expect(chat.messagesFor("peer-x")).toHaveLength(1);
    expect(chat.unreadFor("peer-x")).toBe(1);

    devices.applyUpdate({
      kind: "up",
      device: {
        deviceId: "peer-x",
        name: "PC-x",
        addr: "10.0.0.2:28101",
        tcpPort: 28101,
        fingerprint: "AAAA-BBBB-CCCC-DDDD",
      },
    });
    expect(devices.devices.some((device) => device.deviceId === "peer-x")).toBe(true);
  });

  it("applySession 维护已连接设备列表", () => {
    const store = useChatStore();
    store.applySession({ kind: "connected", peerId: "peer-1", name: "PC-peer" });
    store.applySession({ kind: "connected", peerId: "peer-1", name: "PC-peer" });
    expect(store.isConnected("peer-1")).toBe(true);
    expect(store.connected).toHaveLength(1);

    store.applySession({ kind: "disconnected", peerId: "peer-1" });
    expect(store.isConnected("peer-1")).toBe(false);
  });

  it("applySession 重连中记录尝试次数，连接成功后清除（v2/M10）", () => {
    const store = useChatStore();
    store.applySession({ kind: "connected", peerId: "peer-1", name: "PC-peer" });
    expect(store.isConnected("peer-1")).toBe(true);

    store.applySession({ kind: "reconnecting", peerId: "peer-1", attempt: 2 });
    expect(store.isConnected("peer-1")).toBe(false);
    expect(store.reconnectAttempt("peer-1")).toBe(2);

    store.applySession({ kind: "connected", peerId: "peer-1", name: "PC-peer" });
    expect(store.reconnectAttempt("peer-1")).toBe(0);
  });

  it("applySession 重连失败后清空重连状态（v2/M10）", () => {
    const store = useChatStore();
    store.applySession({ kind: "reconnecting", peerId: "peer-1", attempt: 1 });
    expect(store.reconnectAttempt("peer-1")).toBe(1);

    store.applySession({ kind: "reconnectFailed", peerId: "peer-1" });
    expect(store.reconnectAttempt("peer-1")).toBe(0);
  });

  it("retry 复用原 msg_id 并推进状态（v3）", async () => {
    api.sendText.mockResolvedValueOnce({ msgId: "m-1", status: "failed" });
    const store = useChatStore();
    await store.send("peer-1", "hello");
    const failedId = store.messagesFor("peer-1")[0].id;

    api.resendText.mockResolvedValueOnce({ msgId: "m-1", status: "delivered" });
    await expect(store.retry(failedId)).resolves.toBe(true);

    const message = store.messagesFor("peer-1")[0];
    expect(message.id).toBe("m-1");
    expect(message.status).toBe("delivered");
    expect(api.resendText).toHaveBeenCalledWith("m-1");
    // 只发送过一次（重发走 resend_text，不再调用 send_text）
    expect(api.sendText).toHaveBeenCalledTimes(1);
  });

  it("init 订阅消息与会话事件，非 Tauri 环境跳过", async () => {
    api.isTauri.mockReturnValue(true);
    api.onMessage.mockResolvedValue(() => {});
    api.onSessionUpdate.mockResolvedValue(() => {});

    const store = useChatStore();
    await store.init();
    await store.init();
    expect(api.onMessage).toHaveBeenCalledTimes(1);
    expect(api.onSessionUpdate).toHaveBeenCalledTimes(1);

    const handler = api.onMessage.mock.calls[0][0] as (m: IncomingMessage) => void;
    handler(incoming("m-3", "来自对端", 3000));
    expect(store.messagesFor("peer-1")[0].text).toBe("来自对端");

    const sessionHandler = api.onSessionUpdate.mock.calls[0][0] as (u: SessionUpdate) => void;
    sessionHandler({ kind: "connected", peerId: "peer-1", name: "PC-peer" });
    expect(store.isConnected("peer-1")).toBe(true);

    setActivePinia(createPinia());
    api.isTauri.mockReturnValue(false);
    const offline = useChatStore();
    await offline.init();
    expect(api.onMessage).toHaveBeenCalledTimes(1);
    expect(offline.started).toBe(false);
  });

  it("clear 重置消息与会话状态", async () => {
    const store = useChatStore();
    store.receive(incoming("m-1", "hi", 1000));
    store.applySession({ kind: "connected", peerId: "peer-1", name: "PC-peer" });
    store.clear();
    expect(store.messages).toHaveLength(0);
    expect(store.connected).toHaveLength(0);
    expect(store.error).toBe("");
  });

  it("未读数累加与已读清零（F42）", () => {
    const store = useChatStore();
    store.receive(incoming("m-1", "a", 1000));
    store.receive(incoming("m-2", "b", 2000));
    expect(store.unreadFor("peer-1")).toBe(2);
    expect(store.unreadFor("peer-2")).toBe(0);

    store.markRead("peer-1");
    expect(store.unreadFor("peer-1")).toBe(0);
  });

  it("clearPeer 只清除该对端消息与未读（F40）", () => {
    const store = useChatStore();
    store.receive(incoming("m-1", "a", 1000));
    store.receive({ peerId: "peer-2", msgId: "m-2", text: "b", ts: 2000 });

    store.clearPeer("peer-1");
    expect(store.messagesFor("peer-1")).toHaveLength(0);
    expect(store.messagesFor("peer-2")).toHaveLength(1);
    expect(store.unreadFor("peer-1")).toBe(0);
  });

  it("loadHistory 载入历史且不重复插入已知消息（F40）", async () => {
    api.isTauri.mockReturnValue(true);
    const store = useChatStore();
    store.receive(incoming("m-1", "已有", 1000));

    const getHistory = (await import("../../src/api/tauri")).getHistory as unknown as {
      mockResolvedValue: (value: unknown) => void;
    };
    getHistory.mockResolvedValue({
      messages: [
        {
          msgId: "m-1",
          peerId: "peer-1",
          direction: "in",
          text: "已有",
          ts: 1000,
          status: "delivered",
        },
        {
          msgId: "m-2",
          peerId: "peer-1",
          direction: "out",
          text: "历史",
          ts: 2000,
          status: "failed",
        },
      ],
      transfers: [],
    });

    await store.loadHistory("peer-1");
    const messages = store.messagesFor("peer-1");
    expect(messages.map((m) => m.id)).toEqual(["m-1", "m-2"]);
    expect(messages[1].status).toBe("failed");
  });
  it("对端不在设备表时触发一次重新发现（v2/M9 真机问题）", async () => {
    api.isTauri.mockReturnValue(true);
    api.refreshDevices.mockResolvedValue(undefined);
    api.listDevices.mockResolvedValue([]);
    api.sendText.mockRejectedValue(new Error("peer not found: peer-1"));

    const store = useChatStore();
    await expect(store.send("peer-1", "hello")).resolves.toBe(false);

    expect(api.refreshDevices).toHaveBeenCalledTimes(1);
    expect(store.error).toBe("peer not found: peer-1");
  });

  it("其他发送错误不触发重新发现", async () => {
    api.isTauri.mockReturnValue(true);
    api.sendText.mockRejectedValue(new Error("empty message"));

    const store = useChatStore();
    await store.send("peer-1", "hello");

    expect(api.refreshDevices).not.toHaveBeenCalled();
  });
  it("当前会话收到消息不累计未读（v2/M9）", () => {
    const store = useChatStore();
    store.setActive("peer-1");
    store.receive(incoming("m-1", "hi", 1));

    expect(store.messagesFor("peer-1")).toHaveLength(1);
    expect(store.unreadFor("peer-1")).toBe(0);
  });

  it("不在该会话页时累计未读，进入会话后清零（v2/M9）", () => {
    const store = useChatStore();

    // 不在任何会话页 → 累计。
    store.receive(incoming("m-1", "a", 1));
    expect(store.unreadFor("peer-1")).toBe(1);

    // 正在看别的会话 → 仍然累计。
    store.setActive("peer-2");
    store.receive(incoming("m-2", "b", 2));
    expect(store.unreadFor("peer-1")).toBe(2);

    // 进入该会话即清零。
    store.setActive("peer-1");
    expect(store.unreadFor("peer-1")).toBe(0);

    // 离开会话后重新累计。
    store.setActive(null);
    store.receive(incoming("m-3", "c", 3));
    expect(store.unreadFor("peer-1")).toBe(1);
  });
});
