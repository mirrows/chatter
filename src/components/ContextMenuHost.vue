<template>
  <Teleport to="body">
    <div
      v-if="menu.visible"
      class="ctx-root"
      :class="{ 'is-centered': menu.centered }"
      data-testid="context-menu-root"
      @mousedown.self="onBackdrop"
      @touchstart.self="onBackdrop"
    >
      <div v-if="menu.centered" class="mask" data-testid="context-menu-mask"></div>
      <ul ref="listEl" class="menu" :style="menuStyle" data-testid="context-menu" role="menu">
        <li v-for="item in menu.items" :key="item.key">
          <button
            type="button"
            :class="{ danger: item.danger }"
            :disabled="item.disabled"
            :data-testid="`context-menu-${item.key}`"
            @click="select(item)"
          >
            {{ item.label }}
          </button>
        </li>
      </ul>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
// 右键/长按菜单宿主（v3）：全局单例，挂在 App.vue。
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useContextMenuStore, type ContextMenuAction } from "../stores/contextMenu";

/** 与视口边缘保持的最小间距。 */
const EDGE = 8;

const menu = useContextMenuStore();
const listEl = ref<HTMLElement | null>(null);
const pos = ref({ left: 0, top: 0 });

const menuStyle = computed(() =>
  menu.centered ? {} : { left: `${pos.value.left}px`, top: `${pos.value.top}px` },
);

/**
 * 边缘检测：默认在鼠标右下方展开，右/下越界时翻转到另一侧。
 *
 * jsdom 等无布局环境里 `offsetWidth` 为 0，此时用保守的默认尺寸兜底。
 */
async function reposition() {
  if (!menu.visible || menu.centered) return;
  await nextTick();
  const el = listEl.value;
  if (!el) return;
  const width = el.offsetWidth || 160;
  const height = el.offsetHeight || 120;
  let left = menu.x;
  let top = menu.y;
  const viewportWidth = window.innerWidth || 1024;
  const viewportHeight = window.innerHeight || 768;
  if (left + width + EDGE > viewportWidth) left = Math.max(EDGE, menu.x - width);
  if (top + height + EDGE > viewportHeight) top = Math.max(EDGE, menu.y - height);
  pos.value = { left, top };
}

watch(
  () => [menu.visible, menu.centered, menu.x, menu.y, menu.items.length],
  () => {
    void reposition();
  },
);

/** 选中菜单项（先关闭再执行）。 */
function select(action: ContextMenuAction) {
  void menu.select(action);
}

/** 移动端点击遮罩关闭。 */
function onBackdrop() {
  if (menu.centered) menu.close();
}

/** 点击菜单外部关闭（PC）。 */
function onDocumentDown(event: Event) {
  if (!menu.visible) return;
  const el = listEl.value;
  const target = event.target as Node | null;
  if (el && target && el.contains(target)) return;
  menu.close();
}

/** Esc / 滚动 / 缩放窗口时关闭。 */
function onEscape(event: KeyboardEvent) {
  if (event.key === "Escape") menu.close();
}
function onViewportChange() {
  if (menu.visible) menu.close();
}

onMounted(() => {
  document.addEventListener("mousedown", onDocumentDown, true);
  document.addEventListener("keydown", onEscape);
  window.addEventListener("scroll", onViewportChange, true);
  window.addEventListener("resize", onViewportChange);
  window.addEventListener("blur", onViewportChange);
});

onUnmounted(() => {
  document.removeEventListener("mousedown", onDocumentDown, true);
  document.removeEventListener("keydown", onEscape);
  window.removeEventListener("scroll", onViewportChange, true);
  window.removeEventListener("resize", onViewportChange);
  window.removeEventListener("blur", onViewportChange);
});
</script>

<style scoped>
.ctx-root {
  position: fixed;
  inset: 0;
  z-index: 800;
}
/* PC：只有菜单本身接收事件，其余区域不拦截点击。 */
.ctx-root:not(.is-centered) {
  pointer-events: none;
}
.ctx-root:not(.is-centered) .menu {
  pointer-events: auto;
}
.mask {
  position: absolute;
  inset: 0;
  background: rgba(0, 0, 0, 0.35);
}
.menu {
  position: absolute;
  margin: 0;
  padding: 4px;
  list-style: none;
  min-width: 140px;
  max-width: 240px;
  border-radius: 8px;
  background: var(--card, #fff);
  border: 1px solid var(--bg, #e5e7eb);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
}
/* 移动端：正中卡片，宽度随屏幕自适应但不超过 320px。 */
.is-centered .menu {
  left: 50%;
  top: 50%;
  transform: translate(-50%, -50%);
  min-width: min(72vw, 260px);
  max-width: min(80vw, 320px);
}
.menu li {
  margin: 0;
}
.menu button {
  display: block;
  width: 100%;
  padding: 10px 12px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--text, #1f2937);
  font: inherit;
  font-size: 14px;
  text-align: left;
  cursor: pointer;
}
.menu button:hover:not(:disabled) {
  background: var(--bg, #f3f4f6);
}
.menu button.danger {
  color: #c2410c;
}
.menu button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
