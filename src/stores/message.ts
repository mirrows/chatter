// 顶部提示条（v3，参考 Element Plus Message）：同屏最多 3 条，超出排队。
//
// 项目规范 R8.1：所有轻量级提示（成功/失败/信息）统一走本组件，
// 默认 1.5s 后自动消失；禁止 window.alert，确认与输入分别用 confirmDialog / promptDialog。
import { defineStore } from "pinia";

/** 提示类型。 */
export type MessageType = "success" | "error" | "info";

/** 一条提示。 */
export interface MessageItem {
  id: number;
  type: MessageType;
  text: string;
  /** 展示时长（毫秒）。 */
  duration: number;
}

/** 入参。 */
export interface MessageOptions {
  text: string;
  type?: MessageType;
  /** 默认 1500ms。 */
  duration?: number;
}

/** 同屏最多展示条数。 */
export const MAX_VISIBLE_MESSAGES = 3;
/** 默认展示时长。 */
export const DEFAULT_MESSAGE_DURATION = 1500;

let sequence = 0;
/** 每条提示的自动消失计时器（不放进 store state，避免被序列化）。 */
const timers = new Map<number, ReturnType<typeof setTimeout>>();

/** 提示 store。 */
export const useMessageStore = defineStore("message", {
  state: () => ({
    /** 当前展示的提示（最多 3 条）。 */
    visible: [] as MessageItem[],
    /** 等待展示的提示。 */
    queue: [] as MessageItem[],
  }),
  actions: {
    /** 追加一条提示；超过 3 条时入队。 */
    push(options: MessageOptions): number {
      sequence += 1;
      const item: MessageItem = {
        id: sequence,
        type: options.type ?? "info",
        text: options.text,
        duration: options.duration ?? DEFAULT_MESSAGE_DURATION,
      };
      if (this.visible.length >= MAX_VISIBLE_MESSAGES) {
        this.queue.push(item);
        return item.id;
      }
      this.show(item);
      return item.id;
    },
    /** 展示并定时移除。 */
    show(item: MessageItem) {
      this.visible.push(item);
      timers.set(
        item.id,
        setTimeout(() => this.remove(item.id), item.duration),
      );
    },
    /** 移除一条；队列中若有等待项，立刻补位展示。 */
    remove(id: number) {
      const timer = timers.get(id);
      if (timer) {
        clearTimeout(timer);
        timers.delete(id);
      }
      this.visible = this.visible.filter((item) => item.id !== id);
      const next = this.queue.shift();
      if (next) this.show(next);
    },
    /** 清空全部提示与计时器（测试/退出登录场景）。 */
    clear() {
      for (const timer of timers.values()) clearTimeout(timer);
      timers.clear();
      this.visible = [];
      this.queue = [];
    },
  },
});

/** 函数式提示。 */
export function showMessage(text: string, type: MessageType = "info", duration?: number): number {
  return useMessageStore().push({ text, type, duration });
}

/** 便捷入口：`message.success("已复制")`。 */
export const message = {
  success: (text: string, duration?: number) => showMessage(text, "success", duration),
  error: (text: string, duration?: number) => showMessage(text, "error", duration),
  info: (text: string, duration?: number) => showMessage(text, "info", duration),
};
