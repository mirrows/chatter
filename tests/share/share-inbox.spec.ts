// 系统分享入口（v3）：收件箱状态、摘要与文件导入。
import { describe, it, expect, vi, beforeEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";

const api = vi.hoisted(() => ({
  takeSharePayload: vi.fn(),
  importSharedFile: vi.fn(),
  isTauri: vi.fn(() => true),
}));

vi.mock("../../src/api/tauri", () => ({
  takeSharePayload: api.takeSharePayload,
  importSharedFile: api.importSharedFile,
  isTauri: api.isTauri,
  // 其它模块可能间接引用，这里给空实现避免解析失败
  DEVICE_UPDATE_EVENT: "device://update",
}));

// 必须在 mock 之后导入被测模块。
import { useShareInboxStore } from "../../src/stores/shareInbox";

describe("share inbox store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    api.isTauri.mockReturnValue(true);
  });

  it("refresh 拉取到文本分享并计算摘要", async () => {
    api.takeSharePayload.mockResolvedValue({ kind: "text", text: "共享的一段文字" });
    const store = useShareInboxStore();

    await store.refresh();

    expect(store.pending).toEqual({ kind: "text", text: "共享的一段文字" });
    expect(store.summary).toBe("共享的一段文字");
  });

  it("长文本摘要会截断，文件摘要取文件名", async () => {
    const store = useShareInboxStore();

    store.pending = { kind: "text", text: "一".repeat(100) };
    expect(store.summary).toBe(`${"一".repeat(40)}…`);

    store.pending = { kind: "file", uri: "content://media/1", name: "报告.pdf" };
    expect(store.summary).toBe("报告.pdf");

    store.pending = { kind: "file", uri: "content://media/2" };
    expect(store.summary).toBe("文件");
  });

  it("无分享时不覆盖已有待发送内容，clear 可取消", async () => {
    api.takeSharePayload.mockResolvedValue(null);
    const store = useShareInboxStore();
    store.pending = { kind: "text", text: "保留" };

    await store.refresh();
    expect(store.pending).toEqual({ kind: "text", text: "保留" });

    store.clear();
    expect(store.pending).toBeNull();
  });

  it("importFile 成功返回路径，失败返回 null（v3）", async () => {
    const store = useShareInboxStore();

    api.importSharedFile.mockResolvedValue("C:/cache/imports/a.png");
    await expect(store.importFile("content://media/1")).resolves.toBe("C:/cache/imports/a.png");

    api.importSharedFile.mockRejectedValue(new Error("cannot open input stream"));
    await expect(store.importFile("content://media/2")).resolves.toBeNull();
  });

  it("非 Tauri 环境 refresh 不调用后端", async () => {
    api.isTauri.mockReturnValue(false);
    const store = useShareInboxStore();
    await store.refresh();
    expect(api.takeSharePayload).not.toHaveBeenCalled();
    expect(store.pending).toBeNull();
  });
});
