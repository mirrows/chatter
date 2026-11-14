// 文件名中间省略（v3）：方形气泡底部单行展示用。
import { describe, it, expect } from "vitest";
import { middleEllipsis } from "../../src/utils/filename";

describe("middleEllipsis", () => {
  it("不超过上限时原样返回", () => {
    expect(middleEllipsis("a.png", 16)).toBe("a.png");
  });

  it("超长文件名从中间省略并保留后缀", () => {
    const text = middleEllipsis("2026 年度总结报告最终版.pdf", 12);
    expect(text).toContain("…");
    expect(text.endsWith(".pdf")).toBe(true);
    expect(text.length).toBeLessThanOrEqual(12);
    expect(text.startsWith("2026")).toBe(true);
  });

  it("无后缀时按整体中间省略", () => {
    const text = middleEllipsis("a-very-long-name-without-extension", 10);
    expect(text).toContain("…");
    expect(text.length).toBeLessThanOrEqual(10);
  });

  it("空名与空白名返回空串", () => {
    expect(middleEllipsis("", 8)).toBe("");
    expect(middleEllipsis("   ", 8)).toBe("");
  });

  it("上限极小时仍保留后缀", () => {
    const text = middleEllipsis("报告.pdf", 5);
    expect(text.endsWith(".pdf")).toBe(true);
    expect(text).toContain("…");
  });

  it("后缀过长（如 .tar.gz.bak）时按无后缀处理", () => {
    const text = middleEllipsis("archive.tar.gz.bak", 10);
    expect(text.length).toBeLessThanOrEqual(10);
    expect(text).toContain("…");
  });
});
