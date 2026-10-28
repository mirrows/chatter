// 剪贴板工具单测：优先 clipboard API，失败回退 execCommand（F12）。
import { describe, it, expect, vi, afterEach } from "vitest";
import { copyText } from "../../src/utils/clipboard";

describe("copyText", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("优先使用 navigator.clipboard.writeText", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });

    await expect(copyText("你好")).resolves.toBe(true);
    expect(writeText).toHaveBeenCalledWith("你好");
  });

  it("clipboard 被拒绝时回退到 execCommand", async () => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) },
      configurable: true,
    });
    const exec = vi.fn().mockReturnValue(true);
    Object.defineProperty(document, "execCommand", { value: exec, configurable: true });

    await expect(copyText("hi")).resolves.toBe(true);
    expect(exec).toHaveBeenCalledWith("copy");
  });

  it("两条路径都失败时返回 false 而不抛错", async () => {
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    Object.defineProperty(document, "execCommand", {
      value: vi.fn(() => {
        throw new Error("nope");
      }),
      configurable: true,
    });

    await expect(copyText("hi")).resolves.toBe(false);
  });
});
