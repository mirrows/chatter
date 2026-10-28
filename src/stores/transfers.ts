// 传输任务状态管理（Pinia store）：图片/文件发送、确认与进度（M3-a）。
import { defineStore } from "pinia";
import {
  acceptFile,
  cancelTransfer,
  getHistory,
  getImagePreview,
  isTauri,
  listTransfers,
  onTransferUpdate,
  pickFile,
  rejectFile,
  retryTransfer,
  sendFile,
  sendPaths,
} from "../api/tauri";
import type { PathSendResult, TransferInfo } from "../api/types";

/** 传输 store。 */
export const useTransferStore = defineStore("transfers", {
  state: () => ({
    items: [] as TransferInfo[],
    /** 已加载的图片预览（transferId → data URL）。 */
    previews: {} as Record<string, string>,
    /** 是否已订阅后端事件。 */
    started: false,
    /** 最近一次错误信息（空串表示无错误）。 */
    error: "",
    /** 最近一次拖拽中被拒绝的目录数量（F31）。 */
    folderRejected: 0,
  }),
  getters: {
    /** 指定对端的传输任务（按文件名排序）。 */
    forPeer: (state) => (peerId: string) =>
      state.items
        .filter((t) => t.peerId === peerId)
        .slice()
        .sort((a, b) => a.filename.localeCompare(b.filename)),
    /** 等待用户确认的文件（F34）。 */
    pending: (state) => state.items.filter((t) => t.state === "pending"),
  },
  actions: {
    /** 插入或刷新一条任务快照。 */
    upsert(info: TransferInfo) {
      const index = this.items.findIndex((t) => t.transferId === info.transferId);
      if (index >= 0) this.items[index] = info;
      else this.items.push(info);
    },
    /** 应用一次后端推送（`transfer://update`）。 */
    apply(info: TransferInfo) {
      this.upsert(info);
    },
    /** 拉取已有任务并订阅状态事件；重复调用无副作用。 */
    async init() {
      if (this.started || !isTauri()) return;
      this.started = true;
      try {
        const items = await listTransfers();
        this.items = items;
        await onTransferUpdate((info) => this.apply(info));
        this.error = "";
      } catch (err) {
        this.started = false;
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 选择文件并发送（F20/F30）；用户取消选择时返回 null。 */
    async pickAndSend(peerId: string, imagesOnly: boolean): Promise<TransferInfo | null> {
      try {
        const path = await pickFile(imagesOnly);
        if (!path) return null;
        const info = await sendFile(peerId, path);
        this.upsert(info);
        this.error = "";
        return info;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
        return null;
      }
    },
    /** 按路径发送（拖拽/粘贴入口，F21/F22）。 */
    async sendPath(peerId: string, path: string): Promise<TransferInfo | null> {
      try {
        const info = await sendFile(peerId, path);
        this.upsert(info);
        this.error = "";
        return info;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
        return null;
      }
    },
    /** 批量发送拖拽进来的路径（F22）；目录计入 folderRejected（F31）。 */
    async sendPaths(peerId: string, paths: string[]): Promise<PathSendResult[]> {
      if (paths.length === 0) return [];
      try {
        const results = await sendPaths(peerId, paths);
        let folders = 0;
        for (const result of results) {
          if (result.ok && result.transfer) this.upsert(result.transfer);
          if (result.error === "is_dir") folders += 1;
        }
        this.folderRejected = folders;
        this.error = "";
        return results;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
        return [];
      }
    },
    /** 清除目录提示（F31）。 */
    clearFolderNotice() {
      this.folderRejected = 0;
    },
    /** 接受接收（F34）。 */
    async accept(transferId: string) {
      try {
        await acceptFile(transferId);
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 拒绝接收（F34）。 */
    async reject(transferId: string) {
      try {
        await rejectFile(transferId);
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 取消传输（F33）。 */
    async cancel(transferId: string) {
      try {
        await cancelTransfer(transferId);
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 重试失败的发送任务（M3-b）；成功则登记新的任务快照。 */
    async retry(transferId: string) {
      try {
        const info = await retryTransfer(transferId);
        this.upsert(info);
        this.error = "";
        return info;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
        return null;
      }
    },
    /** 载入已接收图片的预览 data URL（F23）。 */
    async loadPreview(transferId: string) {
      if (this.previews[transferId]) return;
      try {
        const url = await getImagePreview(transferId);
        this.previews[transferId] = url;
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 载入该对端的历史传输记录（F40）。 */
    async loadHistory(peerId: string) {
      if (!isTauri()) return;
      try {
        const history = await getHistory(peerId);
        for (const stored of history.transfers) {
          if (this.items.some((item) => item.transferId === stored.transferId)) continue;
          this.items.push({
            transferId: stored.transferId,
            peerId: stored.peerId,
            direction: stored.direction,
            kind: stored.kind,
            filename: stored.filename,
            size: stored.size,
            transferred: stored.transferred,
            state: stored.state,
            path: stored.path,
            error: null,
          });
        }
        this.error = "";
      } catch (err) {
        this.error = err instanceof Error ? err.message : String(err);
      }
    },
    /** 清空（测试与后续"清空历史"复用）。 */
    clear() {
      this.items = [];
      this.previews = {};
      this.started = false;
      this.error = "";
      this.folderRejected = 0;
    },
  },
});
