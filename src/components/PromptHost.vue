<template>
  <Teleport to="body">
    <div v-if="dialog" class="prompt-root" data-testid="prompt-root" @click.self="store.cancel()">
      <div
        class="dialog"
        :style="dialogStyle"
        role="dialog"
        aria-modal="true"
        data-testid="prompt-dialog"
      >
        <h3 class="title" data-testid="prompt-title">{{ dialog.title }}</h3>
        <p v-if="dialog.message" class="message" data-testid="prompt-message">
          {{ dialog.message }}
        </p>
        <input
          ref="inputEl"
          v-model="store.draft"
          class="input"
          type="text"
          data-testid="prompt-input"
          :placeholder="dialog.placeholder ?? ''"
          @keydown.enter.prevent="store.confirm()"
        />
        <div class="actions">
          <button
            type="button"
            class="secondary"
            data-testid="prompt-cancel"
            @click="store.cancel()"
          >
            {{ dialog.cancelText ?? t("common.cancel") }}
          </button>
          <button type="button" data-testid="prompt-ok" @click="store.confirm()">
            {{ dialog.confirmText ?? t("common.ok") }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
// 输入弹窗宿主（v3）：遮罩 + 正中卡片，Enter 确认、Esc 取消。
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { t } from "../i18n";
import { usePromptStore } from "../stores/prompt";

/** 宽度夹取范围。 */
const MIN_WIDTH = 280;
const MAX_WIDTH = 560;

const store = usePromptStore();
const inputEl = ref<HTMLInputElement | null>(null);
const dialog = computed(() => store.active);

const dialogStyle = computed(() => {
  const requested = dialog.value?.maxWidth ?? 420;
  const clamped = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, requested));
  return { maxWidth: `min(${clamped}px, 92vw)` };
});

/** 打开时聚焦并全选，便于直接覆盖旧名称。 */
watch(
  () => dialog.value?.id,
  async (id) => {
    if (!id) return;
    await nextTick();
    inputEl.value?.focus();
    inputEl.value?.select();
  },
);

/** Esc 取消。 */
function onEscape(event: KeyboardEvent) {
  if (event.key === "Escape" && store.active) store.cancel();
}

onMounted(() => document.addEventListener("keydown", onEscape));
onUnmounted(() => document.removeEventListener("keydown", onEscape));
</script>

<style scoped>
.prompt-root {
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
  margin: 0 0 10px;
  font-size: 13px;
  color: var(--muted, #6b7280);
}
.input {
  width: 100%;
  box-sizing: border-box;
  padding: 8px 10px;
  margin-bottom: 16px;
  border-radius: 8px;
  border: 1px solid var(--bg, #e5e7eb);
  background: var(--bg, #f9fafb);
  color: var(--text, #1f2937);
  font: inherit;
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
</style>
