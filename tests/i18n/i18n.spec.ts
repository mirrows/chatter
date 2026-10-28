// i18n 单测：占位符替换、缺失词条回退与语言切换（F53 / R3.3）。
import { describe, it, expect, afterEach } from "vitest";
import { getLocale, setLocale, t, type MessageKey } from "../../src/i18n";

describe("i18n", () => {
  afterEach(() => {
    setLocale("zh-CN");
  });

  it("按 key 返回中文文案", () => {
    expect(t("device.title")).toBe("局域网设备");
  });

  it("替换占位符", () => {
    expect(t("device.count", { count: 3 })).toBe("在线 3 台");
    expect(t("device.local", { name: "PC-8f3a" })).toBe("本机：PC-8f3a");
  });

  it("缺少参数时保留占位符而不是输出 undefined", () => {
    expect(t("device.local")).toBe("本机：{name}");
  });

  it("未知 key 回退为 key 本身", () => {
    expect(t("device.unknown" as MessageKey)).toBe("device.unknown");
  });

  it("默认语言为中文且可切换", () => {
    expect(getLocale()).toBe("zh-CN");
    setLocale("zh-CN");
    expect(getLocale()).toBe("zh-CN");
  });
});
