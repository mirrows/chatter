<template>
  <article
    class="transfer"
    :class="isSend ? 'is-send' : 'is-receive'"
    :data-testid="`transfer-${transfer.state}`"
  >
    <!-- 发送方：等待对端接收 -->
    <span v-if="waitingAccept" class="tag" data-testid="transfer-waiting">
      {{ t("transfer.waiting") }}
    </span>
    <!-- 发送方：失败后在方形左侧显示重试 -->
    <button
      v-if="showRetry"
      type="button"
      class="icon-button"
      data-testid="transfer-retry"
      :title="t('transfer.retry')"
      @click.stop="emit('retry', transfer.transferId)"
    >
      ↻
    </button>

    <div
      class="box"
      :class="{ static: !isClickable }"
      data-testid="transfer-box"
      :title="boxTitle"
      @contextmenu.prevent="openMenu"
      @touchstart.passive="longPress.onTouchStart"
      @touchmove.passive="longPress.onTouchMove"
      @touchend="longPress.onTouchEnd"
      @touchcancel="longPress.onTouchCancel"
      @click="onBoxClick"
    >
      <img
        v-if="isImage && previewUrl"
        :src="previewUrl"
        :alt="transfer.filename"
        class="thumb"
        data-testid="transfer-image"
      />
      <div v-else-if="isImage" class="thumb placeholder" data-testid="transfer-image-placeholder">
        <svg viewBox="0 0 24 24" class="glyph" aria-hidden="true">
          <path
            d="M4 5h16v14H4zM4 15l4-4 3 3 3-3 6 6"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linejoin="round"
          />
        </svg>
      </div>
      <div v-else class="file">
        <svg viewBox="0 0 24 24" class="glyph file-glyph" aria-hidden="true">
          <path
            d="M6 3h7l5 5v13H6zM13 3v5h5"
            fill="none"
            stroke="currentColor"
            stroke-width="1.6"
            stroke-linejoin="round"
          />
        </svg>
        <span class="name" data-testid="transfer-name">{{ shortName }}</span>
      </div>

      <!-- 进度仅在传输中展示（v3 需求 5） -->
      <div v-if="showProgress" class="progress" data-testid="transfer-progress">
        <div class="bar" :style="{ width: `${percent}%` }" data-testid="transfer-bar"></div>
      </div>
      <span v-if="showProgress" class="percent" data-testid="transfer-percent">{{ percent }}%</span>
      <span v-if="badge" class="badge" data-testid="transfer-badge">{{ badge }}</span>
    </div>

    <!-- 接收方：pending 显示接收/取消，transferring 显示取消 -->
    <div v-if="receiveActions" class="actions">
      <button
        v-if="canAccept"
        type="button"
        class="icon-button accept"
        data-testid="transfer-accept"
        :title="t('transfer.accept')"
        @click.stop="emit('accept', transfer.transferId)"
      >
        ✓
      </button>
      <button
        v-if="canCancel"
        type="button"
        class="icon-button"
        :data-testid="canAccept ? 'transfer-reject' : 'transfer-cancel'"
        :title="canAccept ? t('transfer.reject') : t('transfer.cancel')"
        @click.stop="emit('reject', transfer.transferId)"
      >
        ×
      </button>
    </div>
  </article>
</template>

<script setup lang="ts">
// 传输气泡（v3）：120px 方形，图片不显示文件名，文件为图标 + 底部单行名（中间省略）。
import { computed } from "vue";
import type { TransferInfo } from "../api/types";
import { useLongPress } from "../composables/useLongPress";
import { t } from "../i18n";
import { middleEllipsis } from "../utils/filename";

const props = defineProps<{
  transfer: TransferInfo;
  /** 图片预览 data URL（发送方用本地源文件，接收方用落盘文件）。 */
  previewUrl?: string;
  /** 预览失败原因（来自后端，展示在 title 里便于排障）。 */
  previewError?: string;
  /** 移动端：只用长按呼出菜单，忽略 contextmenu（否则会先闪一个鼠标位置菜单）。 */
  touchOnly?: boolean;
  /** 失败任务是否隐藏重试：历史加载的失败任务重启后不再可重试（v3）。 */
  hideRetry?: boolean;
}>();

const emit = defineEmits<{
  accept: [transferId: string];
  reject: [transferId: string];
  cancel: [transferId: string];
  retry: [transferId: string];
  preview: [src: string];
  menu: [point: { x: number; y: number; centered: boolean }];
  /** 已完成的文件被点击：交由上层用本地路径打开（v3：发送方打开源文件）。 */
  open: [];
}>();

const isImage = computed(() => props.transfer.kind === "image");
const isSend = computed(() => props.transfer.direction === "send");
const percent = computed(() => {
  const { size, transferred, state } = props.transfer;
  if (size <= 0) return state === "done" ? 100 : 0;
  return Math.min(100, Math.round((transferred / size) * 100));
});

/** 进度条只在传输中显示，完成后隐藏（需求 5）。 */
const showProgress = computed(() => props.transfer.state === "transferring");
const waitingAccept = computed(() => isSend.value && props.transfer.state === "offered");
/** 失败任务只有本次运行内发起的才显示重试（v3：重启后隐藏）。 */
const showRetry = computed(
  () => isSend.value && props.transfer.state === "failed" && !props.hideRetry,
);
const canAccept = computed(() => !isSend.value && props.transfer.state === "pending");
const canCancel = computed(
  () =>
    !isSend.value &&
    (props.transfer.state === "pending" || props.transfer.state === "transferring"),
);
const receiveActions = computed(() => canAccept.value || canCancel.value);

/**
 * 气泡是否可点击（v3）。
 *
 * - 已完成：两个方向都可点开（发送方打开源文件、接收方打开落盘文件）；
 * - 已拒绝/失败/取消：**只有发送方**可以点（打开自己的源文件），接收方无本地副本，不可点。
 */
const isClickable = computed(() => {
  const { state } = props.transfer;
  if (state === "done") return true;
  // 失败/拒绝/取消：接收方没有本地副本不可点；发送方仍可打开自己的源文件。
  if (state === "failed" || state === "rejected" || state === "cancelled") return isSend.value;
  // 进行中：只有已经拿到预览数据的图片可点（点开预览）。
  return isImage.value && Boolean(props.previewUrl);
});

/** 失败/取消/拒绝才显示小角标；done 不显示任何状态文案（需求 5）。 */
const badge = computed(() => {
  switch (props.transfer.state) {
    case "failed":
      return t("transfer.state.failed");
    case "cancelled":
      return t("transfer.state.cancelled");
    case "rejected":
      return t("transfer.state.rejected");
    default:
      return "";
  }
});

const shortName = computed(() => middleEllipsis(props.transfer.filename, 14));

/** 预览失败原因 → 文案（只作为 title，不占用气泡空间）。 */
const previewErrorText = computed(() => {
  const reason = props.previewError ?? "";
  if (!reason) return "";
  if (reason.includes("too large")) return t("transfer.previewTooLarge");
  if (reason.includes("not received yet")) return t("transfer.previewPending");
  if (reason.includes("file unavailable")) return t("transfer.previewMissing");
  return t("transfer.previewUnknown");
});
const boxTitle = computed(() =>
  previewErrorText.value
    ? `${props.transfer.filename}（${previewErrorText.value}）`
    : props.transfer.filename,
);

/** 移动端长按 → 居中菜单；同时抑制随后触发的 click（避免误开预览）。 */
const longPress = useLongPress((point) => emit("menu", point));

/** PC 右键 → 鼠标附近菜单；移动端只走长按（避免先闪出鼠标位置的菜单）。 */
function openMenu(event: MouseEvent) {
  if (props.touchOnly) return;
  emit("menu", { x: event.clientX, y: event.clientY, centered: false });
}

/** 点击气泡：完成的图片打开预览；可点的文件请求上层用本地路径打开（v3）。 */
function onBoxClick() {
  if (longPress.fired()) return;
  if (!isClickable.value) return;
  const { state } = props.transfer;
  if (state === "failed" || state === "rejected" || state === "cancelled") {
    // 发送方点开自己的源文件；找不到时由上层统一提示。
    emit("open");
    return;
  }
  if (isImage.value) {
    if (props.previewUrl) emit("preview", props.previewUrl);
    return;
  }
  if (state === "done") emit("open");
}
</script>

<style scoped>
.transfer {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
.is-receive {
  justify-content: flex-start;
}
.is-send {
  justify-content: flex-end;
}
.box {
  position: relative;
  width: 120px;
  height: 120px;
  flex: 0 0 120px;
  border-radius: 10px;
  background: var(--bg);
  overflow: hidden;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: default;
  /* 长按时不显示系统高亮/选择框，避免气泡边缘出现残缺的描边（v3 需求 1）。 */
  -webkit-tap-highlight-color: transparent;
  -webkit-touch-callout: none;
  user-select: none;
  -webkit-user-select: none;
}
.box:hover {
  box-shadow: 0 0 0 2px var(--accent-hover, rgba(24, 84, 168, 0.25));
}
/* 不可点击（接收方的失败/拒绝任务）：不显示 hover 反馈（v3） */
.box.static:hover {
  box-shadow: none;
}
.thumb {
  width: 100%;
  height: 100%;
  object-fit: contain; /* 不拉伸，按比例完整展示 */
  display: block;
}
.placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--muted);
}
.glyph {
  width: 40px;
  height: 40px;
}
.file {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
  height: 100%;
  padding: 8px 6px;
  box-sizing: border-box;
}
.file-glyph {
  width: 38px;
  height: 38px;
  color: var(--accent);
}
.name {
  max-width: 100%;
  font-size: 11px;
  line-height: 1.2;
  color: var(--text);
  text-align: center;
  white-space: nowrap;
  overflow: hidden;
}
.progress {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 3px;
  background: rgba(0, 0, 0, 0.12);
}
.bar {
  height: 100%;
  background: var(--accent);
  transition: width 0.2s linear;
}
.percent {
  position: absolute;
  right: 4px;
  bottom: 7px;
  padding: 0 4px;
  border-radius: 4px;
  font-size: 10px;
  color: #fff;
  background: rgba(0, 0, 0, 0.45);
}
.badge {
  position: absolute;
  left: 4px;
  top: 4px;
  padding: 0 4px;
  border-radius: 4px;
  font-size: 10px;
  color: #fff;
  background: #c2410c;
}
.tag {
  font-size: 11px;
  color: var(--muted);
  white-space: nowrap;
}
.actions {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.icon-button {
  width: 28px;
  height: 28px;
  padding: 0;
  border: 1px solid var(--accent);
  border-radius: 50%;
  background: var(--card);
  color: var(--accent);
  font-size: 15px;
  line-height: 1;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.icon-button.accept {
  background: var(--accent);
  color: #fff;
}
</style>
