// 长按手势（v3）：移动端用长按代替右键呼出菜单。
//
// 只处理触摸事件，桌面端仍走 `contextmenu`；移动超过容差即取消，避免滚动时误触发。
import { onUnmounted, ref } from "vue";

/** 长按触发点。`centered` 表示移动端应居中展示菜单。 */
export interface LongPressPoint {
  x: number;
  y: number;
  centered: boolean;
}

export interface LongPressOptions {
  /** 触发时长（毫秒），默认 500。 */
  delay?: number;
  /** 位移容差（像素），超过即取消，默认 8。 */
  moveTolerance?: number;
}

/** 长按手势处理器集合（绑定到元素的 touch 事件上）。 */
export interface LongPressHandlers {
  onTouchStart: (event: TouchEvent) => void;
  onTouchMove: (event: TouchEvent) => void;
  onTouchEnd: () => void;
  onTouchCancel: () => void;
  /** 本次触摸是否已经触发过长按（用于抑制随后的 click）。 */
  fired: () => boolean;
}

/** 创建长按手势；组件卸载时自动清理计时器。 */
export function useLongPress(
  handler: (point: LongPressPoint) => void,
  options: LongPressOptions = {},
): LongPressHandlers {
  const delay = options.delay ?? 500;
  const tolerance = options.moveTolerance ?? 8;

  let timer: ReturnType<typeof setTimeout> | null = null;
  let startX = 0;
  let startY = 0;
  const fired = ref(false);

  const clear = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
  };

  const onTouchStart = (event: TouchEvent) => {
    const touch = event.touches[0];
    if (!touch) return;
    startX = touch.clientX;
    startY = touch.clientY;
    fired.value = false;
    clear();
    timer = setTimeout(() => {
      fired.value = true;
      handler({ x: startX, y: startY, centered: true });
    }, delay);
  };

  const onTouchMove = (event: TouchEvent) => {
    const touch = event.touches[0];
    if (!touch) return;
    if (
      Math.abs(touch.clientX - startX) > tolerance ||
      Math.abs(touch.clientY - startY) > tolerance
    ) {
      clear();
    }
  };

  const onTouchEnd = () => clear();

  onUnmounted(clear);

  return {
    onTouchStart,
    onTouchMove,
    onTouchEnd,
    onTouchCancel: onTouchEnd,
    fired: () => fired.value,
  };
}
