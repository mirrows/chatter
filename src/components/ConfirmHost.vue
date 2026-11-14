<template>
  <Teleport to="body">
    <div v-if="dialog" class="confirm-root" data-testid="confirm-root" @click.self="store.cancel()">
      <div
        class="dialog"
        :style="dialogStyle"
        role="dialog"
        aria-modal="true"
        data-testid="confirm-dialog"
      >
        <h3 class="title" data-testid="confirm-title">{{ dialog.title }}</h3>
        <p v-if="dialog.message" class="message" data-testid="confirm-message">
          {{ dialog.message }}
        </p>
        <div class="actions">
          <button
            type="button"
            class="secondary"
            data-testid="confirm-cancel"
            :disabled="store.loading"
            @click="store.cancel()"
          >
            {{ dialog.cancelText ?? t("common.cancel") }}
          </button>
          <button
            type="button"
            :class="{ danger: dialog.danger }"
            data-testid="confirm-ok"
            :disabled="store.loading"
            @click="store.confirm()"
          >
            {{ dialog.confirmText ?? t("common.ok") }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
// 确认弹窗宿主（v3）：全局单例队列，遮罩 + 屏幕正中，PC/移动端同一套布局。
import { computed, onMounted, onUnmounted } from "vue";
import { t } from "../i18n";
import { useConfirmStore } from "../stores/confirm";

/** 弹窗宽度夹取范围（避免 PC 过宽、移动端溢出）。 */
const MIN_WIDTH = 280;
const MAX_WIDTH = 560;

const store = useConfirmStore();
const dialog = computed(() => store.active);

/** 宽度：`min(clamp(props.maxWidth), 92vw)`。 */
const dialogStyle = computed(() => {
  const requested = dialog.value?.maxWidth ?? 420;
  const clamped = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, requested));
  return { maxWidth: `min(${clamped}px, 92vw)` };
});

/** Esc 关闭（等价于取消）。 */
function onEscape(event: KeyboardEvent) {
  if (event.key === "Escape" && store.active) store.cancel();
}

onMounted(() => document.addEventListener("keydown", onEscape));
onUnmounted(() => document.removeEventListener("keydown", onEscape));
</script>

<style scoped>
.confirm-root {
  position: fixed;
  inset: 0;
  z-index: 900;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  background: rgba(0, 0, 0, 0.45);
}
.dialog {
  width: 100%;
  padding: 18px 18px 14px;
  border-radius: 10px;
  background: var(--card, #fff);
  color: var(--text, #1f2937);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.24);
}
.title {
  margin: 0 0 8px;
  font-size: 16px;
  font-weight: 600;
}
.message {
  margin: 0 0 16px;
  font-size: 14px;
  line-height: 1.5;
  color: var(--muted, #6b7280);
  white-space: pre-wrap;
  word-break: break-word;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
button {
  padding: 7px 16px;
  border: none;
  border-radius: 8px;
  background: var(--accent, #1854a8);
  color: #fff;
  font: inherit;
  cursor: pointer;
}
button.secondary {
  background: transparent;
  color: var(--accent, #1854a8);
  border: 1px solid var(--accent, #1854a8);
}
button.danger {
  background: #c2410c;
}
button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}
</style>
