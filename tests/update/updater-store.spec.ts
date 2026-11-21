// 更新 store 单测（v3）：启动初始化、检查、安装、失败回滚与手动回滚。
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import type { UpdateHandle } from "../../src/api/update";
import type { UpdateStatus } from "../../src/api/types";

const api = vi.hoisted(() => ({
  isTauri: vi.fn(),
  getUpdateStatus: vi.fn(),
  checkForUpdate: vi.fn(),
  downloadAndInstall: vi.fn(),
  prepareUpdate: vi.fn(),
  cancelUpdate: vi.fn(),
  confirmUpdateHealthy: vi.fn(),
  rollbackUpdate: vi.fn(),
  ackRollback: vi.fn(),
  relaunchApp: vi.fn(),
}));

vi.mock("../../src/api/update", () => ({
  RELEASES_URL: "https://example.test/releases",
  getUpdateStatus: api.getUpdateStatus,
  checkForUpdate: api.checkForUpdate,
  downloadAndInstall: api.downloadAndInstall,
  prepareUpdate: api.prepareUpdate,
  cancelUpdate: api.cancelUpdate,
  confirmUpdateHealthy: api.confirmUpdateHealthy,
  rollbackUpdate: api.rollbackUpdate,
  ackRollback: api.ackRollback,
  relaunchApp: api.relaunchApp,
}));

vi.mock("../../src/api/tauri", () => ({ isTauri: api.isTauri }));

// 必须在 mock 之后导入被测模块。
import { useUpdaterStore } from "../../src/stores/updater";
import { useMessageStore } from "../../src/stores/message";

/** 构造更新状态。 */
function status(overrides: Partial<UpdateStatus> = {}): UpdateStatus {
  return {
    currentVersion: "1.0.0",
    platform: "windows",
    backupAvailable: false,
    pending: null,
    lastRollback: null,
    ...overrides,
  };
}

/** 构造检查结果。 */
function handle(version = "1.1.0"): UpdateHandle {
  return {
    info: {
      version,
      currentVersion: "1.0.0",
      notes: null,
      date: null,
    },
    raw: {} as UpdateHandle["raw"],
  };
}

describe("updater store", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it("非 Tauri 环境不读取状态（v3）", async () => {
    api.isTauri.mockReturnValue(false);
    const store = useUpdaterStore();

    await store.init();

    expect(api.getUpdateStatus).not.toHaveBeenCalled();
    expect(store.initialized).toBe(false);
  });

  it("init 读取状态并在窗口稳定后静默检查更新（v3）", async () => {
    api.isTauri.mockReturnValue(true);
    api.getUpdateStatus.mockResolvedValue(status());
    api.checkForUpdate.mockResolvedValue(null);

    const store = useUpdaterStore();
    await store.init();

    expect(store.currentVersion).toBe("1.0.0");
    expect(store.phase).toBe("idle");
    expect(api.checkForUpdate).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(5000);

    expect(api.checkForUpdate).toHaveBeenCalledTimes(1);
    expect(store.phase).toBe("latest");
  });

  it("init 在 Android 上标记 unsupported 且不检查更新（v3）", async () => {
    api.isTauri.mockReturnValue(true);
    api.getUpdateStatus.mockResolvedValue(status({ platform: "android" }));

    const store = useUpdaterStore();
    await store.init();
    await vi.advanceTimersByTimeAsync(10000);

    expect(store.phase).toBe("unsupported");
    expect(api.checkForUpdate).not.toHaveBeenCalled();
  });

  it("init 提示上次回滚并清除记录（v3）", async () => {
    api.isTauri.mockReturnValue(true);
    api.getUpdateStatus.mockResolvedValue(
      status({ lastRollback: { from: "1.1.0", to: "1.0.0", at: 1 } }),
    );
    api.ackRollback.mockResolvedValue(undefined);

    const store = useUpdaterStore();
    await store.init();

    expect(api.ackRollback).toHaveBeenCalledTimes(1);
    expect(store.status?.lastRollback).toBeNull();
    expect(useMessageStore().visible.map((item) => item.text)).toContain(
      "新版本 1.1.0 无法正常启动，已自动回滚到 1.0.0",
    );
  });

  it("init 对 pending 更新在 8 秒后确认健康（v3）", async () => {
    api.isTauri.mockReturnValue(true);
    api.getUpdateStatus.mockResolvedValue(
      status({ pending: { from: "1.0.0", to: "1.1.0", attempts: 1 } }),
    );

    const store = useUpdaterStore();
    await store.init();
    expect(api.confirmUpdateHealthy).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(8000);

    expect(api.confirmUpdateHealthy).toHaveBeenCalledTimes(1);
    expect(store.status?.pending).toBeNull();
  });

  it("check 没有新版本时进入 latest（v3）", async () => {
    api.checkForUpdate.mockResolvedValue(null);
    const store = useUpdaterStore();

    await expect(store.check()).resolves.toBeNull();

    expect(store.phase).toBe("latest");
    expect(useMessageStore().visible.map((item) => item.text)).toContain("已是最新版本");
  });

  it("check 有新版本时记录版本并进入 available（v3）", async () => {
    api.checkForUpdate.mockResolvedValue(handle("1.1.0"));
    const store = useUpdaterStore();

    const result = await store.check();

    expect(result?.info.version).toBe("1.1.0");
    expect(store.phase).toBe("available");
    expect(store.available?.version).toBe("1.1.0");
  });

  it("check 失败时记录错误进入 error（v3）", async () => {
    api.checkForUpdate.mockRejectedValue(new Error("network down"));
    const store = useUpdaterStore();

    await store.check();

    expect(store.phase).toBe("error");
    expect(store.error).toBe("操作失败，请稍后重试");
  });

  it("install 先备份当前版本，安装成功后重启应用（v3）", async () => {
    api.checkForUpdate.mockResolvedValue(handle("1.1.0"));
    api.prepareUpdate.mockResolvedValue(undefined);
    api.downloadAndInstall.mockImplementation(
      async (_handle: UpdateHandle, onProgress?: (percent: number) => void) => {
        onProgress?.(50);
        onProgress?.(100);
      },
    );
    api.relaunchApp.mockResolvedValue(undefined);

    const store = useUpdaterStore();
    await store.check();
    await store.install();

    expect(api.prepareUpdate).toHaveBeenCalledWith("1.0.0", "1.1.0");
    expect(api.downloadAndInstall).toHaveBeenCalledTimes(1);
    expect(store.progress).toBe(100);
    expect(store.phase).toBe("installing");
    expect(api.relaunchApp).toHaveBeenCalledTimes(1);
  });

  it("install 失败时撤销 pending 并记录错误（v3）", async () => {
    api.checkForUpdate.mockResolvedValue(handle("1.1.0"));
    api.prepareUpdate.mockResolvedValue(undefined);
    api.downloadAndInstall.mockRejectedValue(new Error("checksum mismatch"));
    api.cancelUpdate.mockResolvedValue(undefined);

    const store = useUpdaterStore();
    await store.check();
    await store.install();

    expect(api.cancelUpdate).toHaveBeenCalledTimes(1);
    expect(store.phase).toBe("error");
    expect(api.relaunchApp).not.toHaveBeenCalled();
  });

  it("rollback 调用后端并重启（v3）", async () => {
    api.rollbackUpdate.mockResolvedValue(undefined);
    api.relaunchApp.mockResolvedValue(undefined);

    const store = useUpdaterStore();
    await store.rollback();

    expect(api.rollbackUpdate).toHaveBeenCalledTimes(1);
    expect(api.relaunchApp).toHaveBeenCalledTimes(1);
    expect(useMessageStore().visible.map((item) => item.text)).toContain(
      "正在回滚，应用将重新启动…",
    );
  });

  it("canRollback 反映后端备份状态（v3）", () => {
    const store = useUpdaterStore();
    expect(store.canRollback).toBe(false);

    store.status = status({ backupAvailable: true });
    expect(store.canRollback).toBe(true);
    expect(store.isDesktop).toBe(true);

    store.status = status({ platform: "android" });
    expect(store.isDesktop).toBe(false);
  });
});
