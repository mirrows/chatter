// 统一错误兜底（v3 / R8.6）：中文文案透传、技术细节隐藏。
import { describe, it, expect } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { friendlyError, handleError, rawErrorText } from "../../src/utils/errors";

describe("errors util", () => {
  it("中文后端文案直接透传（已知友好错误）", () => {
    expect(friendlyError(new Error("昵称不能为空"))).toBe("昵称不能为空");
    expect(friendlyError("文件不存在或已被移动")).toBe("文件不存在或已被移动");
  });

  it("英文/技术错误统一兜底，不裸奔错误码", () => {
    expect(friendlyError(new Error("unknown transfer: t-1"))).toBe("操作失败，请稍后重试");
    expect(friendlyError(new Error("permission denied"))).toBe("操作失败，请稍后重试");
    expect(friendlyError(undefined)).toBe("操作失败，请稍后重试");
  });

  it("超长中文错误也走兜底（避免整段技术细节铺满界面）", () => {
    const long = "错".repeat(200);
    expect(friendlyError(long)).toBe("操作失败，请稍后重试");
  });

  it("rawErrorText 兼容 Error / string / 其它类型", () => {
    expect(rawErrorText(new Error("boom"))).toBe("boom");
    expect(rawErrorText("boom")).toBe("boom");
    expect(rawErrorText({ code: 1 })).toBe('{"code":1}');
    expect(rawErrorText(null)).toBe("");
  });

  it("handleError 返回友好文案并可安全上报（非 Tauri 环境不上报）", () => {
    setActivePinia(createPinia());
    expect(handleError("test.scope", new Error("permission denied"))).toBe("操作失败，请稍后重试");
  });
});
