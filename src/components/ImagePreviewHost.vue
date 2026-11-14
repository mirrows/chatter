<template>
  <Teleport to="body">
    <div
      v-if="preview.visible"
      ref="rootEl"
      class="preview-root"
      :class="{ dragging, zoomed: scale > MIN_SCALE }"
      data-testid="image-preview"
      @click.self="onBackdropClick"
      @wheel.prevent="onWheel"
      @mousedown.prevent="onMouseDown"
      @touchstart="onTouchStart"
      @touchmove.prevent="onTouchMove"
      @touchend="onTouchEnd"
      @touchcancel="onTouchEnd"
    >
      <button
        type="button"
        class="close"
        data-testid="image-preview-close"
        aria-label="close"
        @click="preview.close()"
      >
        ×
      </button>
      <img
        ref="imgEl"
        :src="preview.src"
        :alt="preview.alt"
        class="image"
        data-testid="image-preview-img"
        :style="{ transform: imageTransform }"
        draggable="false"
        @click.stop
        @mousedown.stop.prevent="onMouseDown"
      />
    </div>
  </Teleport>
</template>

<script setup lang="ts">
// 图片预览宿主（v3）：全屏遮罩 + contain 居中。
// - PC：鼠标滚轮缩放（以光标为中心）、按住拖拽平移；
// - 移动端：双指缩放、单指拖拽平移；
// - 关闭后复位（缩放与位移都归零）。
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useImagePreviewStore } from "../stores/imagePreview";

/** 缩放范围：1（原始）～ 4 倍。 */
const MIN_SCALE = 1;
const MAX_SCALE = 4;
/** 滚轮每一格的缩放步长。 */
const ZOOM_STEP = 0.2;
/** 位移超过该像素数视为拖拽，避免松手时误触「点遮罩关闭」。 */
const DRAG_THRESHOLD = 3;

const preview = useImagePreviewStore();
const rootEl = ref<HTMLElement | null>(null);
const imgEl = ref<HTMLImageElement | null>(null);
const scale = ref(1);
const offsetX = ref(0);
const offsetY = ref(0);
const dragging = ref(false);
/** 本次按下后是否真的拖动过（用于抑制拖拽结束时的点击关闭）。 */
let movedDuringDrag = false;

/** 图片变换：先平移再缩放（缩放以图片中心为基准）。 */
const imageTransform = computed(
  () => `translate(${offsetX.value}px, ${offsetY.value}px) scale(${scale.value})`,
);

/** 两指间距。 */
function distanceOf(first: Touch, second: Touch): number {
  return Math.hypot(first.clientX - second.clientX, first.clientY - second.clientY);
}

function clampScale(value: number): number {
  return Math.min(MAX_SCALE, Math.max(MIN_SCALE, value));
}

/** 限制平移范围，避免把图片整体拖出屏幕。 */
function clampOffset() {
  const el = imgEl.value;
  if (!el) return;
  const width = el.offsetWidth;
  const height = el.offsetHeight;
  // 未完成布局（如测试环境）时拿不到尺寸，不做限制，避免把偏移强制归零。
  if (width <= 0 || height <= 0) return;
  const maxX = Math.max(0, (width * (scale.value - 1)) / 2);
  const maxY = Math.max(0, (height * (scale.value - 1)) / 2);
  offsetX.value = Math.min(maxX, Math.max(-maxX, offsetX.value));
  offsetY.value = Math.min(maxY, Math.max(-maxY, offsetY.value));
}

/** 复位到原始大小与居中位置。 */
function resetTransform() {
  scale.value = MIN_SCALE;
  offsetX.value = 0;
  offsetY.value = 0;
  dragging.value = false;
  movedDuringDrag = false;
}

/** 缩小回 1 倍时把位移归零，避免残留偏移。 */
function settle() {
  if (scale.value <= MIN_SCALE + 0.01) {
    scale.value = MIN_SCALE;
    offsetX.value = 0;
    offsetY.value = 0;
  } else {
    clampOffset();
  }
}

/** 滚轮缩放（PC）：以光标位置为锚点，保证光标下的图像内容不跳动。 */
function onWheel(event: WheelEvent) {
  const root = rootEl.value;
  if (!root) return;
  const factor = event.deltaY < 0 ? 1 + ZOOM_STEP : 1 / (1 + ZOOM_STEP);
  const next = clampScale(scale.value * factor);
  if (next === scale.value) return;

  const rect = root.getBoundingClientRect();
  const pointerX = event.clientX - (rect.left + rect.width / 2);
  const pointerY = event.clientY - (rect.top + rect.height / 2);
  const ratio = next / scale.value;
  offsetX.value = pointerX - (pointerX - offsetX.value) * ratio;
  offsetY.value = pointerY - (pointerY - offsetY.value) * ratio;
  scale.value = next;
  settle();
}

// ---- 鼠标拖拽（PC） ----
let dragStartX = 0;
let dragStartY = 0;
let dragOriginX = 0;
let dragOriginY = 0;

function onMouseDown(event: MouseEvent) {
  if (event.button !== 0) return;
  dragging.value = true;
  movedDuringDrag = false;
  dragStartX = event.clientX;
  dragStartY = event.clientY;
  dragOriginX = offsetX.value;
  dragOriginY = offsetY.value;
  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp);
}

function onMouseMove(event: MouseEvent) {
  if (!dragging.value) return;
  const dx = event.clientX - dragStartX;
  const dy = event.clientY - dragStartY;
  if (Math.abs(dx) > DRAG_THRESHOLD || Math.abs(dy) > DRAG_THRESHOLD) movedDuringDrag = true;
  if (scale.value <= MIN_SCALE) {
    // 未放大时不允许把图片拖走（保持居中）。
    offsetX.value = 0;
    offsetY.value = 0;
    return;
  }
  offsetX.value = dragOriginX + dx;
  offsetY.value = dragOriginY + dy;
  clampOffset();
}

function onMouseUp() {
  dragging.value = false;
  window.removeEventListener("mousemove", onMouseMove);
  window.removeEventListener("mouseup", onMouseUp);
}

// ---- 触摸：单指拖拽 / 双指缩放 ----
let touchStartDistance = 0;
let touchStartScale = 1;
let touchOriginX = 0;
let touchOriginY = 0;
let touchStartX = 0;
let touchStartY = 0;

function onTouchStart(event: TouchEvent) {
  if (event.touches.length === 1) {
    touchStartX = event.touches[0].clientX;
    touchStartY = event.touches[0].clientY;
    touchOriginX = offsetX.value;
    touchOriginY = offsetY.value;
    movedDuringDrag = false;
    return;
  }
  if (event.touches.length === 2) {
    touchStartDistance = distanceOf(event.touches[0], event.touches[1]);
    touchStartScale = scale.value;
  }
}

function onTouchMove(event: TouchEvent) {
  if (event.touches.length === 1) {
    if (scale.value <= MIN_SCALE) return;
    const dx = event.touches[0].clientX - touchStartX;
    const dy = event.touches[0].clientY - touchStartY;
    if (Math.abs(dx) > DRAG_THRESHOLD || Math.abs(dy) > DRAG_THRESHOLD) movedDuringDrag = true;
    offsetX.value = touchOriginX + dx;
    offsetY.value = touchOriginY + dy;
    clampOffset();
    return;
  }
  if (event.touches.length !== 2 || touchStartDistance <= 0) return;
  const next =
    (distanceOf(event.touches[0], event.touches[1]) / touchStartDistance) * touchStartScale;
  scale.value = clampScale(next);
  settle();
}

function onTouchEnd() {
  touchStartDistance = 0;
  settle();
}

/** 点击遮罩关闭；刚拖拽过则不关（避免拖到边缘松手就退出）。 */
function onBackdropClick() {
  if (movedDuringDrag) {
    movedDuringDrag = false;
    return;
  }
  preview.close();
}

// 每次关闭都复位，下次打开保持原始大小（需求 6）。
watch(
  () => preview.visible,
  (visible) => {
    if (!visible) resetTransform();
  },
);

/** Esc 关闭。 */
function onEscape(event: KeyboardEvent) {
  if (event.key === "Escape" && preview.visible) preview.close();
}

onMounted(() => document.addEventListener("keydown", onEscape));
onUnmounted(() => {
  document.removeEventListener("keydown", onEscape);
  window.removeEventListener("mousemove", onMouseMove);
  window.removeEventListener("mouseup", onMouseUp);
});
</script>

<style scoped>
.preview-root {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.85);
  /* 允许双指缩放与单指拖拽（全局其它区域已禁用）。 */
  touch-action: none;
  user-select: none;
}
/* 放大后可拖拽：光标提示可移动 */
.preview-root.zoomed {
  cursor: grab;
}
.preview-root.zoomed.dragging {
  cursor: grabbing;
}
.image {
  max-width: 80vw;
  max-height: 80vh;
  object-fit: contain;
  border-radius: 4px;
  transform-origin: center center;
  will-change: transform;
  touch-action: none;
}
.close {
  position: absolute;
  top: 12px;
  right: 16px;
  width: 36px;
  height: 36px;
  border: none;
  border-radius: 50%;
  background: rgba(255, 255, 255, 0.16);
  color: #fff;
  font-size: 22px;
  line-height: 1;
  cursor: pointer;
}
.close:hover {
  background: rgba(255, 255, 255, 0.28);
}
</style>
