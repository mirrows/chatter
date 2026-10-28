// 剪贴板图片工具单测（F21）。
import { describe, it, expect } from "vitest";
import { imageFileFromItems, readFileAsBase64 } from "../../src/utils/paste";

/** 构造一个最小可用的 DataTransferItem 列表（jsdom 不提供 DataTransferItemList 构造器）。 */
function itemsOf(
  entries: Array<{ kind: string; type: string; file?: File }>,
): DataTransferItemList {
  const list = entries.map((entry) => ({
    kind: entry.kind,
    type: entry.type,
    getAsFile: () => entry.file ?? null,
  }));
  return list as unknown as DataTransferItemList;
}

describe("paste utils", () => {
  it("从粘贴项中取出第一张图片", () => {
    const image = new File([new Uint8Array([1, 2, 3])], "shot.png", { type: "image/png" });
    const items = itemsOf([
      { kind: "string", type: "text/plain" },
      { kind: "file", type: "image/png", file: image },
    ]);
    expect(imageFileFromItems(items)).toBe(image);
  });

  it("纯文本粘贴不返回图片", () => {
    const items = itemsOf([{ kind: "string", type: "text/plain" }]);
    expect(imageFileFromItems(items)).toBeNull();
    expect(imageFileFromItems(null)).toBeNull();
  });

  it("读取图片为 base64 并推断扩展名", async () => {
    const file = new File([new Uint8Array([255, 216, 255])], "shot.png", { type: "image/png" });
    const result = await readFileAsBase64(file);
    expect(result.extension).toBe("png");
    expect(result.data.length).toBeGreaterThan(0);
    expect(result.data.startsWith("data:")).toBe(false);
  });

  it("jpg 类型统一映射为 jpeg 扩展名", async () => {
    const file = new File([new Uint8Array([1])], "shot.jpg", { type: "image/jpg" });
    const result = await readFileAsBase64(file);
    expect(result.extension).toBe("jpeg");
  });
});
