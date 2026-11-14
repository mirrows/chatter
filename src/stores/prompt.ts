// 输入弹窗（v3）：函数式 `promptDialog`，用于修改设备名称等单行输入场景。
import { defineStore } from "pinia";

/** 输入弹窗配置。 */
export interface PromptOptions {
  /** 标题。 */
  title: string;
  /** 说明文案（可选）。 */
  message?: string;
  /** 初始值。 */
  value?: string;
  /** 输入框占位符。 */
  placeholder?: string;
  /** 确认按钮文案，默认「确定」。 */
  confirmText?: string;
  /** 取消按钮文案，默认「取消」。 */
  cancelText?: string;
  /** 最大宽度（默认 420，夹在 280~560）。 */
  maxWidth?: number;
}

/** 队列中的弹窗。 */
export interface PromptDialog extends PromptOptions {
  id: number;
  resolve: (value: string | null) => void;
}

let sequence = 0;

/** 输入弹窗 store：一次只展示队首弹窗。 */
export const usePromptStore = defineStore("prompt", {
  state: () => ({
    dialogs: [] as PromptDialog[],
    /** 当前输入内容（绑定到宿主输入框）。 */
    draft: "",
  }),
  getters: {
    /** 当前展示的弹窗。 */
    active: (state): PromptDialog | null => state.dialogs[0] ?? null,
  },
  actions: {
    /** 入队并返回输入结果（取消为 null）。 */
    open(options: PromptOptions): Promise<string | null> {
      return new Promise<string | null>((resolve) => {
        sequence += 1;
        this.dialogs.push({ ...options, id: sequence, resolve });
        // 只有成为队首时才初始化输入框，避免排队弹窗覆盖当前值。
        if (this.dialogs.length === 1) this.draft = options.value ?? "";
      });
    },
    /** 确认：返回去除首尾空格后的内容（空串表示恢复默认）。 */
    confirm() {
      const dialog = this.active;
      if (!dialog) return;
      this.finish(dialog, this.draft.trim());
    },
    /** 取消。 */
    cancel() {
      const dialog = this.active;
      if (!dialog) return;
      this.finish(dialog, null);
    },
    /** 出队并回传结果。 */
    finish(dialog: PromptDialog, value: string | null) {
      this.dialogs = this.dialogs.filter((item) => item.id !== dialog.id);
      dialog.resolve(value);
      this.draft = this.dialogs[0]?.value ?? "";
    },
  },
});

/** 函数式调用：返回输入内容，取消返回 null。 */
export function promptDialog(options: PromptOptions): Promise<string | null> {
  return usePromptStore().open(options);
}
