// 平台能力判断（v3）：把「桌面 / 移动」的分支收敛到一处，
// 组件里不再各自判断 `devices.isAndroid`。
import { computed } from "vue";
import { useDeviceStore } from "../stores/devices";

/** 平台相关能力（响应式）。 */
export function usePlatform() {
  const devices = useDeviceStore();
  const platform = computed(() => devices.platform);
  const isAndroid = computed(() => devices.isAndroid);
  const isWindows = computed(() => devices.isWindows);
  /** 桌面端（Windows / macOS / Linux）。 */
  const isDesktop = computed(
    () => platform.value === "windows" || platform.value === "macos" || platform.value === "linux",
  );

  return { platform, isAndroid, isWindows, isDesktop };
}
