// 格式化工具单测：字节、速度与剩余时间展示（M3-b）。
import { describe, it, expect } from "vitest";
import { formatBytes, formatEta, formatSpeed } from "../../src/utils/format";

describe("format utils", () => {
  it("formatBytes 按单位换算并保留合理精度", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(-1)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(2 * 1024 * 1024 * 1024)).toBe("2.0 GB");
  });

  it("formatSpeed 追加 /s 后缀", () => {
    expect(formatSpeed(0)).toBe("0 B/s");
    expect(formatSpeed(1024 * 1024)).toBe("1.0 MB/s");
  });

  it("formatEta 输出 mm:ss 与 h:mm:ss", () => {
    expect(formatEta(0)).toBe("00:00");
    expect(formatEta(-5)).toBe("00:00");
    expect(formatEta(9)).toBe("00:09");
    expect(formatEta(65)).toBe("01:05");
    expect(formatEta(3725)).toBe("1:02:05");
  });
});
