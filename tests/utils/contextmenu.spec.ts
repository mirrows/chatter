// 右键菜单拦截（v2/M9）：Windows 发布版禁用 WebView 默认上下文菜单。
import { describe, it, expect } from "vitest";
import { blockContextMenu } from "../../src/utils/contextmenu";

/** 造一个可取消的 contextmenu 事件。 */
function contextMenuEvent(): MouseEvent {
  return new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
}

describe("blockContextMenu", () => {
  it("拦截 contextmenu 并阻止默认行为", () => {
    const unblock = blockContextMenu(document);
    const event = contextMenuEvent();
    document.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    unblock();
  });

  it("解绑后恢复默认行为", () => {
    const unblock = blockContextMenu(document);
    unblock();

    const event = contextMenuEvent();
    document.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(false);
  });
});
