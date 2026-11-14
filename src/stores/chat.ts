// 聊天状态管理（Pinia store）：文字消息收发与发送状态。
import { defineStore } from "pinia";
import { getHistory, isTauri, onMessage, onSessionUpdate, sendText } from "../api/tauri";
import type { IncomingMessage, SessionUpdate, StoredMessage, TextStatus } from "../api/types";
import { useDeviceStore } from "./devices";

/** 界面上的消息发送状态（含乐观发送中的临时状态）。 */
export type MessageStatus = TextStatus | "sending";

/** 一条聊天消息。 */
export interface ChatMessage {
  /** 消息 ID（发送成功后替换为后端生成的 ID）。 */
  id: string;
  /** 对端设备 ID。 */
  peerId: string;
  /** 方向：out 本机发出，in 收到。 */
  direction: "out" | "in";
  /** 文本内容。 */
  text: string;
  /** 时间戳（Unix 毫秒）。 */
  ts: number;
  /** 发送状态（收到的消息恒为 delivered）。 */
  status: MessageStatus;
}

let localSeq = 0;

/** 生成本地临时消息 ID（后端返回真实 ID 后会被替换）。 */
function nextLocalId(): string {
  localSeq += 1;
  return `local-${Date.now()}-${localSeq}`;
}

/** 聊天 store。 */
export const useChatStore = defineStore("chat", {
  state: () => ({
    messages: [] as ChatMessage[],
    /** 已建立会话的对端 ID 列表。 */
    connected: [] as string[],
    /** 是否已订阅后端事件。 */
    started: false,
    /** 当前正在查看的会话（空串表示不在会话页，v2/M9 未读判定）。 */
    activePeer: "",
    /** 最近一次错误信息（空串表示无错误）。 */
    error: "",
    /** 各对端未读消息数（F42）。 */
    unread: {} as Record<string, number>,
    /** 各对端当前自动重连尝试次数（v2/M10；0 表示未在重连）。 */
    reconnecting: {} as Record<string, number>,
  }),
  getters: {
    /** 指定对端的消息（按时间升序）。 */
    messagesFor: (state) => (peerId: string) =>
      state.messages.filter((m) => m.peerId === peerId).sort((a, b) => a.ts - b.ts),
    /** 是否与指定对端建立了会话。 */
    isConnected: (state) => (peerId: string) => state.connected.includes(peerId),
    /** 指定对端的未读数（F42）。 */
    unreadFor: (state) => (peerId: string) => state.unread[peerId] ?? 0,
    /** 指定对端的自动重连尝试次数（v2/M10，0 表示未在重连）。 */
    reconnectAttempt: (state) => (peerId: string) => state.reconnecting[peerId] ?? 0,
  },
  actions: {
    /** 记录一条收到的消息（F10）；只有不在该会话页时才累计未读（F42）。 */
    receive(message: IncomingMessage) {
      this.messages.push({
        id: message.msgId,
        peerId: message.peerId,
        direction: "in",
        text: message.text,
        ts: message.ts,
        status: "delivered",
      });
      if (this.activePeer !== message.peerId) {
        this.unread[message.peerId] = (this.unread[message.peerId] ?? 0) + 1;
      }
    },
    /**
     * 设置/清除当前打开的会话。
     *
     * 进入会话即视为已读（清零未读），离开后新消息才会重新累计未读（v2/M9）。
     */
    setActive(peerId: string | null) {
      this.activePeer = peerId ?? "";
      if (peerId) this.unread[peerId] = 0;
    },
    /**
     * 收到新的文件/图片时计入未读（与文字消息同一套规则，v3）。
     *
     * 正在与该对端聊天时不计入，避免自己看着的会话出现红色徽标。
     */
    noteIncomingTransfer(peerId: string) {
      if (this.activePeer === peerId) return;
      this.unread[peerId] = (this.unread[peerId] ?? 0) + 1;
    },
    /** 打开会话时清除该对端未读数（F42）。 */
    markRead(peerId: string) {
      this.unread[peerId] = 0;
    },
    /** 清空某对端的本地消息（配合按会话清空历史，F40）。 */
    clearPeer(peerId: string) {
      this.messages = this.messages.filter((m) => m.peerId !== peerId);
      this.unread[peerId] = 0;
    },
    /** 应用会话连接状态事件（v2/M10 增加重连中/重连失败）。 */
    applySession(update: SessionUpdate) {
      if (update.kind === "connected") {
        if (!this.connected.includes(update.peerId)) this.connected.push(update.peerId);
        delete this.reconnecting[update.peerId];
        return;
      }
      this.connected = this.connected.filter((id) => id !== update.peerId);
      if (update.kind === "reconnecting") {
        this.reconnecting[update.peerId] = update.attempt;
      } else {
        delete this.reconnecting[update.peerId];
      }
    },
    /**
     * 发送文字消息：先乐观插入"发送中"，收到后端结果后更新为已送达/失败（F11）。
     *
     * 返回是否送达，便于界面提示。
     */
    async send(peerId: string, text: string): Promise<boolean> {
      const content = text.trim();
      if (!content) return false;
      const tempId = nextLocalId();
      this.messages.push({
        id: tempId,
        peerId,
        direction: "out",
        text: content,
        ts: Date.now(),
        status: "sending",
      });
      try {
        const result = await sendText(peerId, content);
        const message = this.messages.find((m) => m.id === tempId);
        if (message) {
          message.id = result.msgId;
          message.status = result.status;
        }
        this.error = "";
        return result.status === "delivered";
      } catch (err) {
        const message = this.messages.find((m) => m.id === tempId);
        if (message) message.status = "failed";
        this.error = err instanceof Error ? err.message : String(err);
        await this.refreshDevicesIfPeerMissing(this.error);
        return false;
      }
    },
    /** 重发失败的消息（F11）。 */
    async retry(messageId: string): Promise<boolean> {
      const message = this.messages.find((m) => m.id === messageId);
      if (!message || message.direction !== "out") return false;
      message.status = "sending";
      try {
        const result = await sendText(message.peerId, message.text);
        message.id = result.msgId;
        message.ts = Date.now();
        message.status = result.status;
        this.error = "";
        return result.status === "delivered";
      } catch (err) {
        message.status = "failed";
        this.error = err instanceof Error ? err.message : String(err);
        await this.refreshDevicesIfPeerMissing(this.error);
        return false;
      }
    },
    /**
     * 对端不在设备表时触发一次重新发现（v2/M9：移动端广播被过滤导致设备被超时移除）。
     *
     * 只刷新设备列表，不自动重发——ACK 丢失时自动重发会产生重复消息。
     */
    async refreshDevicesIfPeerMissing(reason: string) {
      if (!reason.includes("peer not found")) return;
      await useDeviceStore().refresh();
    },
    /** 订阅后端消息与会话事件；重复调用无副作用。 */
    async init() {
      if (this.started || !isTauri()) return;
      this.started = true;
      try {
        await onMessage((message) => this.receive(message));
        await onSessionUpdate((update) => this.applySession(update));
        this.error = "";
      } catch (err) {
        this.started = false;
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 载入该对端的历史消息（F40）；已在内存中的消息不重复插入。 */
    async loadHistory(peerId: string) {
      if (!isTauri()) return;
      try {
        const history = await getHistory(peerId);
        for (const stored of history.messages) {
          if (this.messages.some((existing) => existing.id === stored.msgId)) continue;
          this.messages.push({
            id: stored.msgId,
            peerId: stored.peerId,
            direction: stored.direction,
            text: stored.text,
            ts: stored.ts,
            status: stored.status,
          });
        }
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /**
     * 写入一条历史消息（v3 分页加载用）。
     *
     * 与 `loadHistory` 的区别：只做去重与追加，不触发其它副作用，便于按页拼接。
     */
    ingestMessage(stored: StoredMessage) {
      if (this.messages.some((message) => message.id === stored.msgId)) return;
      this.messages.push({
        id: stored.msgId,
        peerId: stored.peerId,
        direction: stored.direction,
        text: stored.text,
        ts: stored.ts,
        status: stored.status,
      });
    },
    /** 清空全部消息与会话状态（测试与后续"清空历史"复用）。 */
    clear() {
      this.messages = [];
      this.connected = [];
      this.unread = {};
      this.reconnecting = {};
      this.started = false;
      this.activePeer = "";
      this.error = "";
    },
    /**
     * 清空本地缓存的全部消息与未读（v3）。
     *
     * 与 `clear()` 的区别：**保持事件订阅**（`started` 不变），
     * 用于设置页「清空全部历史」后立即让对话页变空，且不影响后续收消息。
     */
    resetAll() {
      this.messages = [];
      this.unread = {};
      this.error = "";
    },
  },
});
