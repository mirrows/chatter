// 通用确认弹窗（v3，参考 Element Plus MessageBox）：函数式调用、支持多个弹窗排队。
import { defineStore } from "pinia";

/** 单个确认弹窗的配置。 */
export interface ConfirmOptions {
  /** 标题。 */
  title: string;
  /** 正文（可选）。 */
  message?: string;
  /** 确认按钮文案，默认「确定」。 */
  confirmText?: string;
  /** 取消按钮文案，默认「取消」。 */
  cancelText?: string;
  /** 危险操作（确认按钮红色）。 */
  danger?: boolean;
  /** 最大宽度（默认 420，内部会夹在 280~560 之间）。 */
  maxWidth?: number;
  /** 点击确认时执行；可为异步，期间按钮 loading 且防重复点击。 */
  onConfirm?: () => void | Promise<void>;
  /** 点击取消/遮罩/Esc 时执行。 */
  onCancel?: () => void;
}

/** 队列中的弹窗（带 id 与 Promise 回调）。 */
export interface ConfirmDialog extends ConfirmOptions {
  id: number;
  resolve: (confirmed: boolean) => void;
}

let sequence = 0;

/** 确认弹窗 store：同一时刻只展示队首弹窗。 */
export const useConfirmStore = defineStore("confirm", {
  state: () => ({
    dialogs: [] as ConfirmDialog[],
    /** 确认按钮是否正在执行异步 onConfirm。 */
    loading: false,
  }),
  getters: {
    /** 当前展示的弹窗。 */
    active: (state): ConfirmDialog | null => state.dialogs[0] ?? null,
  },
  actions: {
    /** 入队并返回「是否确认」的 Promise。 */
    open(options: ConfirmOptions): Promise<boolean> {
      return new Promise<boolean>((resolve) => {
        sequence += 1;
        this.dialogs.push({ ...options, id: sequence, resolve });
      });
    },
    /** 点击确认：执行 onConfirm（可异步）后关闭并 resolve(true)。 */
    async confirm() {
      const dialog = this.active;
      if (!dialog || this.loading) return;
      this.loading = true;
      try {
        await dialog.onConfirm?.();
        this.finish(dialog, true);
      } finally {
        this.loading = false;
      }
    },
    /** 取消：执行 onCancel 后关闭并 resolve(false)。 */
    cancel() {
      const dialog = this.active;
      if (!dialog || this.loading) return;
      dialog.onCancel?.();
      this.finish(dialog, false);
    },
    /** 出队并回传结果（队列中的下一个会自动展示）。 */
    finish(dialog: ConfirmDialog, confirmed: boolean) {
      this.dialogs = this.dialogs.filter((item) => item.id !== dialog.id);
      dialog.resolve(confirmed);
    },
  },
});

/** 函数式调用：返回用户是否点了「确定」。 */
export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  return useConfirmStore().open(options);
}
