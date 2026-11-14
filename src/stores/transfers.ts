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
  openTransferFile,
  pickFile,
  rejectFile,
  retryTransfer,
  revealTransferFile,
  saveTransferAs,
  sendFile,
  sendPaths,
} from "../api/tauri";
import type { PathSendResult, StoredTransfer, TransferInfo } from "../api/types";
import { t } from "../i18n";
import { useChatStore } from "./chat";
import { useDeviceStore } from "./devices";
import { message, useMessageStore } from "./message";

/** 「正在准备文件」提示的兜底时长（大文件复制可能很久，正常在完成时移除）。 */
const PREPARING_HINT_MS = 10 * 60 * 1000;

/** 传输 store。 */
export const useTransferStore = defineStore("transfers", {
  state: () => ({
    items: [] as TransferInfo[],
    /** 已加载的图片预览（transferId → data URL）。 */
    previews: {} as Record<string, string>,
    /** 图片预览加载失败原因（transferId → 原因）；用于界面区分失败类型。 */
    previewErrors: {} as Record<string, string>,
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
      const isNew = !this.items.some((item) => item.transferId === info.transferId);
      this.upsert(info);
      // 新收到的文件/图片也算一条未读，设备列表徽标才会 +1（v3）。
      if (isNew && info.direction === "receive") {
        useChatStore().noteIncomingTransfer(info.peerId);
      }
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
      // Android 会先把 SAF 的 content:// 整体复制到应用缓存，文件越大耗时越久；
      // 期间给一条常驻提示并在结束时移除，避免用户以为按钮没反应（v3）。
      const preparing = useDeviceStore().isAndroid
        ? message.info(t("chat.preparingFile"), PREPARING_HINT_MS)
        : null;
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
      } finally {
        if (preparing !== null) useMessageStore().remove(preparing);
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
    /**
     * 用系统默认程序打开已完成的传输文件（F23 / v3）。
     *
     * 发送方打开自己的源文件、接收方打开落盘文件；文件不在时按方向给出轻提示（R8.1）。
     */
    async openFile(transferId: string): Promise<boolean> {
      try {
        await openTransferFile(transferId);
        this.error = "";
        return true;
      } catch (err) {
        this.failAction(transferId, err, "transfer.openFailed");
        return false;
      }
    },
    /** 在文件管理器中定位文件（F23；发送方定位源文件所在目录）。 */
    async revealFile(transferId: string) {
      try {
        await revealTransferFile(transferId);
        this.error = "";
      } catch (err) {
        this.failAction(transferId, err, "transfer.openFailed");
      }
    },
    /** 另存为（F23）；返回目标路径，取消时为 null。 */
    async saveAs(transferId: string): Promise<string | null> {
      try {
        const target = await saveTransferAs(transferId);
        this.error = "";
        return target;
      } catch (err) {
        this.failAction(transferId, err, "transfer.saveFailed");
        return null;
      }
    },
    /**
     * 文件类操作的失败提示：区分「文件不在了」与其它失败。
     *
     * 发送方的本地副本被移动/删除 → 「源文件已被移动或删除」；
     * 接收方的落盘文件被清理 → 「文件已被清理，如需重新获取请让对方重新发送」。
     */
    failAction(
      transferId: string,
      err: unknown,
      fallbackKey: "transfer.openFailed" | "transfer.saveFailed",
    ) {
      const raw = err instanceof Error ? err.message : String(err);
      this.error = raw;
      const missing =
        raw.includes("file unavailable") ||
        raw.includes("file not available") ||
        raw.includes("文件不存在") ||
        raw.includes("尚未接收");
      if (!missing) {
        message.error(t(fallbackKey));
        return;
      }
      const direction = this.items.find((item) => item.transferId === transferId)?.direction;
      message.error(
        direction === "send" ? t("transfer.sourceMissing") : t("transfer.receivedMissing"),
      );
    },
    /** 清空某对端的本地任务（配合按会话清空历史，F40）。 */
    clearPeer(peerId: string) {
      this.items = this.items.filter((item) => item.peerId !== peerId);
    },
    /** 载入已接收图片的预览 data URL（F23）；失败时记录原因供界面提示。 */
    async loadPreview(transferId: string) {
      if (this.previews[transferId]) return;
      try {
        const url = await getImagePreview(transferId);
        this.previews[transferId] = url;
        delete this.previewErrors[transferId];
      } catch (err) {
        const reason = err instanceof Error ? err.message : String(err);
        this.previewErrors[transferId] = reason;
        this.error = reason;
      }
    },
    /**
     * 写入一条历史传输记录（v3 分页加载用）。
     *
     * 只做去重与追加：不触发未读计数，也不改变滚动。
     */
    ingestHistory(stored: StoredTransfer) {
      if (this.items.some((item) => item.transferId === stored.transferId)) return;
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
        ts: stored.ts,
      });
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
            ts: stored.ts,
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
      this.previewErrors = {};
      this.started = false;
      this.error = "";
      this.folderRejected = 0;
    },
  },
});
