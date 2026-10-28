// 聊天状态管理（Pinia store）：文字消息收发与发送状态。
import { defineStore } from "pinia";
import { getHistory, isTauri, onMessage, onSessionUpdate, sendText } from "../api/tauri";
import type { IncomingMessage, SessionUpdate, TextStatus } from "../api/types";

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
    /** 最近一次错误信息（空串表示无错误）。 */
    error: "",
  }),
  getters: {
    /** 指定对端的消息（按时间升序）。 */
    messagesFor: (state) => (peerId: string) =>
      state.messages.filter((m) => m.peerId === peerId).sort((a, b) => a.ts - b.ts),
    /** 是否与指定对端建立了会话。 */
    isConnected: (state) => (peerId: string) => state.connected.includes(peerId),
  },
  actions: {
    /** 记录一条收到的消息（F10）。 */
    receive(message: IncomingMessage) {
      this.messages.push({
        id: message.msgId,
        peerId: message.peerId,
        direction: "in",
        text: message.text,
        ts: message.ts,
        status: "delivered",
      });
    },
    /** 应用会话连接状态事件。 */
    applySession(update: SessionUpdate) {
      if (update.kind === "connected") {
        if (!this.connected.includes(update.peerId)) this.connected.push(update.peerId);
      } else {
        this.connected = this.connected.filter((id) => id !== update.peerId);
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
        return false;
      }
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
    /** 清空全部消息与会话状态（测试与后续"清空历史"复用）。 */
    clear() {
      this.messages = [];
      this.connected = [];
      this.started = false;
      this.error = "";
    },
  },
});
