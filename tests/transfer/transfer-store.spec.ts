// 传输 store 单测：任务合并、文件选择发送、确认/取消与预览缓存（M3-a）。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import type { TransferInfo } from "../../src/api/types";

const api = vi.hoisted(() => ({
  listTransfers: vi.fn(),
  onTransferUpdate: vi.fn(),
  pickFile: vi.fn(),
  sendFile: vi.fn(),
  acceptFile: vi.fn(),
  rejectFile: vi.fn(),
  cancelTransfer: vi.fn(),
  getImagePreview: vi.fn(),
  retryTransfer: vi.fn(),
  sendPaths: vi.fn(),
  isTauri: vi.fn(),
}));

vi.mock("../../src/api/tauri", () => ({
  TRANSFER_UPDATE_EVENT: "transfer://update",
  listTransfers: api.listTransfers,
  onTransferUpdate: api.onTransferUpdate,
  pickFile: api.pickFile,
  sendFile: api.sendFile,
  acceptFile: api.acceptFile,
  rejectFile: api.rejectFile,
  cancelTransfer: api.cancelTransfer,
  retryTransfer: api.retryTransfer,
  getImagePreview: api.getImagePreview,
  sendPaths: api.sendPaths,
  isTauri: api.isTauri,
}));

// 必须在 mock 之后导入被测模块。
import { useTransferStore } from "../../src/stores/transfers";

const transfer = (overrides: Partial<TransferInfo> = {}): TransferInfo => ({
  transferId: "t-1",
  peerId: "peer-1",
  direction: "receive",
  kind: "image",
  filename: "photo.png",
  size: 100,
  transferred: 0,
  state: "pending",
  path: null,
  error: null,
  ...overrides,
});

describe("transfer store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
  });

  it("upsert 按 transferId 合并，不产生重复任务", () => {
    const store = useTransferStore();
    store.upsert(transfer());
    store.upsert(transfer({ state: "transferring", transferred: 50 }));
    expect(store.items).toHaveLength(1);
    expect(store.items[0].state).toBe("transferring");
    expect(store.items[0].transferred).toBe(50);
  });

  it("forPeer 只返回该对端任务并按文件名排序", () => {
    const store = useTransferStore();
    store.upsert(transfer({ transferId: "b", filename: "b.png" }));
    store.upsert(transfer({ transferId: "a", filename: "a.png" }));
    store.upsert(transfer({ transferId: "x", peerId: "peer-2", filename: "x.png" }));

    expect(store.forPeer("peer-1").map((t) => t.transferId)).toEqual(["a", "b"]);
    expect(store.forPeer("peer-2")).toHaveLength(1);
  });

  it("pending 只包含等待确认的接收任务", () => {
    const store = useTransferStore();
    store.upsert(transfer());
    store.upsert(transfer({ transferId: "t-2", state: "transferring" }));
    expect(store.pending.map((t) => t.transferId)).toEqual(["t-1"]);
  });

  it("pickAndSend 在用户取消选择时不发送", async () => {
    api.pickFile.mockResolvedValue(null);
    const store = useTransferStore();

    await expect(store.pickAndSend("peer-1", true)).resolves.toBeNull();
    expect(api.sendFile).not.toHaveBeenCalled();
    expect(api.pickFile).toHaveBeenCalledWith(true);
  });

  it("pickAndSend 选中文件后调用 sendFile 并登记任务", async () => {
    api.pickFile.mockResolvedValue("C:/tmp/photo.png");
    api.sendFile.mockResolvedValue(transfer({ state: "offered", direction: "send" }));
    const store = useTransferStore();

    const info = await store.pickAndSend("peer-1", true);
    expect(api.sendFile).toHaveBeenCalledWith("peer-1", "C:/tmp/photo.png");
    expect(info?.state).toBe("offered");
    expect(store.items).toHaveLength(1);
  });

  it("发送失败时记录错误并返回 null（不抛异常）", async () => {
    api.pickFile.mockResolvedValue("C:/tmp/broken.bin");
    api.sendFile.mockRejectedValue(new Error("peer not found"));
    const store = useTransferStore();

    await expect(store.pickAndSend("peer-1", false)).resolves.toBeNull();
    expect(store.error).toBe("peer not found");
    expect(store.items).toHaveLength(0);
  });

  it("accept/reject/cancel 调用对应命令", async () => {
    api.acceptFile.mockResolvedValue(undefined);
    api.rejectFile.mockResolvedValue(undefined);
    api.cancelTransfer.mockResolvedValue(undefined);
    const store = useTransferStore();

    await store.accept("t-1");
    await store.reject("t-2");
    await store.cancel("t-3");

    expect(api.acceptFile).toHaveBeenCalledWith("t-1");
    expect(api.rejectFile).toHaveBeenCalledWith("t-2");
    expect(api.cancelTransfer).toHaveBeenCalledWith("t-3");
  });

  it("accept 失败时记录错误", async () => {
    api.acceptFile.mockRejectedValue(new Error("invalid transfer state"));
    const store = useTransferStore();
    await store.accept("t-1");
    expect(store.error).toBe("invalid transfer state");
  });

  it("retry 成功后登记新任务，失败时记录错误", async () => {
    api.retryTransfer.mockResolvedValue(
      transfer({ transferId: "t-2", direction: "send", state: "offered" }),
    );
    const store = useTransferStore();
    const info = await store.retry("t-1");
    expect(api.retryTransfer).toHaveBeenCalledWith("t-1");
    expect(info?.transferId).toBe("t-2");
    expect(store.items).toHaveLength(1);

    api.retryTransfer.mockRejectedValue(new Error("missing source path"));
    await expect(store.retry("t-1")).resolves.toBeNull();
    expect(store.error).toBe("missing source path");
  });

  it("sendPaths 汇总拖拽结果：成功的登记任务，目录计入提示（F22/F31）", async () => {
    api.sendPaths.mockResolvedValue([
      {
        path: "C:/tmp/a.png",
        ok: true,
        error: null,
        transfer: transfer({ transferId: "t-9", direction: "send", state: "offered" }),
      },
      { path: "C:/tmp/folder", ok: false, error: "is_dir", transfer: null },
      { path: "C:/tmp/missing", ok: false, error: "not_found", transfer: null },
    ]);
    const store = useTransferStore();

    const results = await store.sendPaths("peer-1", ["C:/tmp/a.png", "C:/tmp/folder"]);
    expect(api.sendPaths).toHaveBeenCalledWith("peer-1", ["C:/tmp/a.png", "C:/tmp/folder"]);
    expect(results).toHaveLength(3);
    expect(store.folderRejected).toBe(1);
    expect(store.items.map((item) => item.transferId)).toEqual(["t-9"]);

    store.clearFolderNotice();
    expect(store.folderRejected).toBe(0);
  });

  it("sendPaths 无路径时不调用后端", async () => {
    const store = useTransferStore();
    await expect(store.sendPaths("peer-1", [])).resolves.toEqual([]);
    expect(api.sendPaths).not.toHaveBeenCalled();
  });

  it("loadPreview 只取一次并缓存", async () => {
    api.getImagePreview.mockResolvedValue("data:image/png;base64,AAA");
    const store = useTransferStore();

    await store.loadPreview("t-1");
    await store.loadPreview("t-1");
    expect(api.getImagePreview).toHaveBeenCalledTimes(1);
    expect(store.previews["t-1"]).toBe("data:image/png;base64,AAA");
  });

  it("init 拉取任务并订阅事件；非 Tauri 环境跳过", async () => {
    api.isTauri.mockReturnValue(true);
    api.listTransfers.mockResolvedValue([transfer()]);
    api.onTransferUpdate.mockResolvedValue(() => {});

    const store = useTransferStore();
    await store.init();
    await store.init();
    expect(api.listTransfers).toHaveBeenCalledTimes(1);
    expect(api.onTransferUpdate).toHaveBeenCalledTimes(1);
    expect(store.items).toHaveLength(1);

    const handler = api.onTransferUpdate.mock.calls[0][0] as (info: TransferInfo) => void;
    handler(transfer({ state: "done", transferred: 100, path: "C:/dl/photo.png" }));
    expect(store.items[0].state).toBe("done");

    setActivePinia(createPinia());
    api.isTauri.mockReturnValue(false);
    const offline = useTransferStore();
    await offline.init();
    expect(api.listTransfers).toHaveBeenCalledTimes(1);
    expect(offline.started).toBe(false);
  });
});
