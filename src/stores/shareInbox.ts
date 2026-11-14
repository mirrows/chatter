// 系统分享入口（v3）：Android `ACTION_SEND` 的内容暂存在这里，选中设备后发送。
import { defineStore } from "pinia";
import { importSharedFile, isTauri, takeSharePayload } from "../api/tauri";
import type { SharePayload } from "../api/types";

/** 文本摘要的最大长度。 */
const SUMMARY_MAX = 40;

/** 系统分享收件箱。 */
export const useShareInboxStore = defineStore("shareInbox", {
  state: () => ({
    /** 待发送的分享内容（null 表示没有）。 */
    pending: null as SharePayload | null,
  }),
  getters: {
    /** 设备页提示用的摘要（文本截断，文件显示文件名）。 */
    summary: (state): string => {
      const pending = state.pending;
      if (!pending) return "";
      if (pending.kind === "text") {
        const text = pending.text.trim();
        return text.length > SUMMARY_MAX ? `${text.slice(0, SUMMARY_MAX)}…` : text;
      }
      return pending.name?.trim() || "文件";
    },
  },
  actions: {
    /**
     * 拉取一次待处理分享（启动与回到前台时调用）。
     *
     * 消费型接口：后端 `take_share_payload` 取走后即清空，重复调用不会重复发送。
     */
    async refresh() {
      if (!isTauri()) return;
      try {
        const payload = await takeSharePayload();
        if (payload) this.pending = payload;
      } catch {
        // 分享入口失败不影响主流程
      }
    },
    /** 丢弃当前分享。 */
    clear() {
      this.pending = null;
    },
    /**
     * 把分享的 `content://` 导入缓存，返回本地路径（失败返回 null）。
     *
     * Android 端由 Kotlin 复制到 `cacheDir/imports/`，与大文件导入共用同一通道。
     */
    async importFile(uri: string): Promise<string | null> {
      try {
        return await importSharedFile(uri);
      } catch {
        return null;
      }
    },
  },
});
