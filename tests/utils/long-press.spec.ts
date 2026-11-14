// 长按手势（v3）：移动端长按呼出菜单，移动/抬起即取消。
import { describe, it, expect, vi, afterEach } from "vitest";
import { defineComponent } from "vue";
import { mount } from "@vue/test-utils";
import {
  useLongPress,
  type LongPressHandlers,
  type LongPressPoint,
} from "../../src/composables/useLongPress";

/** 用最小组件承载 composable（onUnmounted 需要组件上下文）。 */
function mountLongPress(handler: (point: LongPressPoint) => void) {
  const spy = vi.fn(handler);
  const Comp = defineComponent({
    setup() {
      const longPress = useLongPress(spy, { delay: 500, moveTolerance: 8 });
      return { longPress };
    },
    template: '<div data-testid="target" />',
  });
  const wrapper = mount(Comp);
  return {
    wrapper,
    spy,
    handlers: (wrapper.vm as unknown as { longPress: LongPressHandlers }).longPress,
  };
}

const touch = (x: number, y: number) =>
  ({ touches: [{ clientX: x, clientY: y }] }) as unknown as TouchEvent;

describe("useLongPress", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("按住 500ms 触发一次，坐标为按下点且 centered=true", () => {
    vi.useFakeTimers();
    const { handlers, spy } = mountLongPress(() => {});
    handlers.onTouchStart(touch(12, 34));
    expect(spy).not.toHaveBeenCalled();

    vi.advanceTimersByTime(520);
    expect(spy).toHaveBeenCalledWith({ x: 12, y: 34, centered: true });
    expect(handlers.fired()).toBe(true);
  });

  it("移动超过容差取消触发", () => {
    vi.useFakeTimers();
    const { handlers, spy } = mountLongPress(() => {});
    handlers.onTouchStart(touch(10, 10));
    handlers.onTouchMove(touch(40, 10));
    vi.advanceTimersByTime(600);
    expect(spy).not.toHaveBeenCalled();
  });

  it("抬起取消触发", () => {
    vi.useFakeTimers();
    const { handlers, spy } = mountLongPress(() => {});
    handlers.onTouchStart(touch(10, 10));
    handlers.onTouchEnd();
    vi.advanceTimersByTime(600);
    expect(spy).not.toHaveBeenCalled();
  });

  it("小幅抖动不取消（容差内）", () => {
    vi.useFakeTimers();
    const { handlers, spy } = mountLongPress(() => {});
    handlers.onTouchStart(touch(10, 10));
    handlers.onTouchMove(touch(13, 12));
    vi.advanceTimersByTime(600);
    expect(spy).toHaveBeenCalledTimes(1);
  });
});
