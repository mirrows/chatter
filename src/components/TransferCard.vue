<template>
  <article class="card" :data-testid="`transfer-${transfer.state}`">
    <header class="row">
      <span class="name" data-testid="transfer-name">{{ transfer.filename }}</span>
      <span class="muted">{{ directionLabel }} · {{ stateLabel }}</span>
    </header>

    <div v-if="showProgress" class="progress">
      <div class="track">
        <div class="bar" :style="{ width: `${percent}%` }" data-testid="transfer-bar"></div>
      </div>
      <span class="muted" data-testid="transfer-percent">
        {{ t("transfer.progress", { percent }) }}
      </span>
    </div>

    <p v-if="showSpeed" class="muted" data-testid="transfer-speed">
      {{ t("transfer.speed", { speed: speedLabel }) }}
      <template v-if="etaLabel">· {{ t("transfer.remaining", { time: etaLabel }) }}</template>
    </p>

    <img
      v-if="isDoneImage && previewUrl"
      :src="previewUrl"
      class="preview"
      :alt="transfer.filename"
      data-testid="transfer-image"
    />
    <p v-if="isDoneImage && !previewUrl" class="muted" data-testid="transfer-preview-unavailable">
      {{ t("transfer.previewUnavailable") }}
    </p>

    <p v-if="transfer.error" class="error" data-testid="transfer-error">{{ transfer.error }}</p>

    <div class="actions">
      <template v-if="transfer.state === 'pending'">
        <button data-testid="transfer-accept" @click="emit('accept', transfer.transferId)">
          {{ t("transfer.accept") }}
        </button>
        <button
          class="secondary"
          data-testid="transfer-reject"
          @click="emit('reject', transfer.transferId)"
        >
          {{ t("transfer.reject") }}
        </button>
      </template>
      <button
        v-else-if="!isTerminal"
        class="secondary"
        data-testid="transfer-cancel"
        @click="emit('cancel', transfer.transferId)"
      >
        {{ t("transfer.cancel") }}
      </button>
      <button
        v-else-if="canRetry"
        data-testid="transfer-retry"
        @click="emit('retry', transfer.transferId)"
      >
        {{ t("transfer.retry") }}
      </button>
    </div>
  </article>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { t, type MessageKey } from "../i18n";
import { formatEta, formatSpeed } from "../utils/format";
import type { TransferInfo, TransferState } from "../api/types";

const props = defineProps<{ transfer: TransferInfo; previewUrl?: string }>();
const emit = defineEmits<{
  accept: [transferId: string];
  reject: [transferId: string];
  cancel: [transferId: string];
  retry: [transferId: string];
}>();

/** 状态 → i18n key（避免动态拼接丢失类型检查）。 */
const STATE_KEY: Record<TransferState, MessageKey> = {
  pending: "transfer.state.pending",
  offered: "transfer.state.offered",
  transferring: "transfer.state.transferring",
  done: "transfer.state.done",
  failed: "transfer.state.failed",
  cancelled: "transfer.state.cancelled",
  rejected: "transfer.state.rejected",
};

const percent = computed(() => {
  const { size, transferred, state } = props.transfer;
  if (size <= 0) return state === "done" ? 100 : 0;
  return Math.min(100, Math.round((transferred / size) * 100));
});

const stateLabel = computed(() => t(STATE_KEY[props.transfer.state]));
const directionLabel = computed(() =>
  props.transfer.direction === "send"
    ? t("transfer.direction.send")
    : t("transfer.direction.receive"),
);
const isTerminal = computed(() =>
  ["done", "failed", "cancelled", "rejected"].includes(props.transfer.state),
);
const showProgress = computed(
  () =>
    props.transfer.state === "transferring" ||
    props.transfer.state === "offered" ||
    props.transfer.state === "pending" ||
    props.transfer.state === "done",
);
const isDoneImage = computed(
  () => props.transfer.kind === "image" && props.transfer.state === "done",
);

/** 失败/取消的发送任务可重试（M3-b）。 */
const canRetry = computed(
  () =>
    props.transfer.direction === "send" &&
    (props.transfer.state === "failed" || props.transfer.state === "cancelled"),
);

// 速度采样：按进度变化计算，间隔过短时跳过，避免抖动。
const speed = ref(0);
let lastTransferred = props.transfer.transferred;
let lastSampledAt = Date.now();

watch(
  () => props.transfer.transferred,
  (value) => {
    const now = Date.now();
    const seconds = (now - lastSampledAt) / 1000;
    if (seconds < 0.3) return;
    const delta = value - lastTransferred;
    if (delta > 0) speed.value = delta / seconds;
    lastTransferred = value;
    lastSampledAt = now;
  },
);

const showSpeed = computed(() => props.transfer.state === "transferring" && speed.value > 0);
const speedLabel = computed(() => formatSpeed(speed.value));
const etaLabel = computed(() => {
  if (!showSpeed.value) return "";
  const remaining = props.transfer.size - props.transfer.transferred;
  return remaining > 0 ? formatEta(remaining / speed.value) : "";
});
</script>

<style scoped>
.card {
  border: 1px solid var(--bg);
  border-radius: 8px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.row {
  display: flex;
  justify-content: space-between;
  gap: 8px;
  align-items: baseline;
}
.name {
  font-size: 14px;
  word-break: break-all;
}
.muted {
  color: var(--muted);
  font-size: 12px;
  margin: 0;
}
.progress {
  display: flex;
  align-items: center;
  gap: 8px;
}
.track {
  flex: 1;
  height: 4px;
  background: var(--bg);
  border-radius: 2px;
  overflow: hidden;
}
.bar {
  height: 100%;
  background: var(--accent);
  transition: width 0.2s linear;
}
.preview {
  max-width: 100%;
  max-height: 220px;
  object-fit: contain;
  border-radius: 6px;
}
.error {
  color: #c2410c;
  font-size: 12px;
  margin: 0;
}
.actions {
  display: flex;
  gap: 8px;
}
button {
  padding: 4px 12px;
  border: none;
  border-radius: 6px;
  background: var(--accent);
  color: #fff;
  cursor: pointer;
  font-size: 13px;
}
button.secondary {
  background: transparent;
  color: var(--accent);
  border: 1px solid var(--accent);
}
</style>
