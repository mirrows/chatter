// 右键/长按菜单（v3）：全局单例，PC 定位到鼠标附近，移动端遮罩 + 居中。
import { defineStore } from "pinia";

/** 菜单项。 */
export interface ContextMenuAction {
  /** 唯一标识（用于 testid）。 */
  key: string;
  /** 展示文案（已通过 i18n 翻译）。 */
  label: string;
  /** 危险操作（红色）。 */
  danger?: boolean;
  /** 禁用（仍展示但不响应）。 */
  disabled?: boolean;
  /** 选中后的回调，可为异步。 */
  onSelect: () => void | Promise<void>;
}

/** 触发点。 */
export interface ContextMenuPoint {
  x: number;
  y: number;
}

/** 上下文菜单 store：任何时刻只展示一个菜单。 */
export const useContextMenuStore = defineStore("contextMenu", {
  state: () => ({
    visible: false,
    /** PC 端定位坐标（相对视口）。 */
    x: 0,
    y: 0,
    /** 是否居中展示（移动端长按）。 */
    centered: false,
    items: [] as ContextMenuAction[],
  }),
  actions: {
    /** 在鼠标位置打开菜单（PC）。 */
    openAtPointer(point: ContextMenuPoint, items: ContextMenuAction[]) {
      if (items.length === 0) return;
      this.visible = true;
      this.centered = false;
      this.x = point.x;
      this.y = point.y;
      this.items = items;
    },
    /** 居中打开菜单（移动端长按）。 */
    openCentered(items: ContextMenuAction[]) {
      if (items.length === 0) return;
      this.visible = true;
      this.centered = true;
      this.items = items;
    },
    /** 关闭菜单。 */
    close() {
      this.visible = false;
      this.items = [];
    },
    /** 选中一项：先关闭再执行回调（避免回调里再次打开菜单被覆盖）。 */
    async select(action: ContextMenuAction) {
      if (action.disabled) return;
      this.close();
      await action.onSelect();
    },
  },
});
