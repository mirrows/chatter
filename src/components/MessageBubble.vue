<template>
  <article
    class="bubble"
    :class="message.direction"
    :data-testid="`message-${message.direction}`"
    @contextmenu.prevent="openMenu"
    @touchstart.passive="longPress.onTouchStart"
    @touchmove.passive="longPress.onTouchMove"
    @touchend="longPress.onTouchEnd"
    @touchcancel="longPress.onTouchCancel"
  >
    <p class="text">{{ message.text }}</p>
    <span class="meta">
      <template v-if="message.direction === 'out'">{{ t(STATUS_KEY[message.status]) }}</template>
      <button
        v-if="message.direction === 'out' && message.status === 'failed'"
        class="link"
        data-testid="chat-retry"
        @click="emit('retry', message.id)"
      >
        {{ t("chat.retry") }}
      </button>
    </span>
  </article>
</template>

<script setup lang="ts">
// 文本气泡（v3）：复制改为右键/长按菜单，气泡内不再有复制按钮。
import { useLongPress } from "../composables/useLongPress";
import { t, type MessageKey } from "../i18n";
import type { ChatMessage, MessageStatus } from "../stores/chat";

const props = defineProps<{
  message: ChatMessage;
  /** 移动端：只用长按呼出菜单，忽略 contextmenu。 */
  touchOnly?: boolean;
}>();
const emit = defineEmits<{
  retry: [messageId: string];
  copy: [messageId: string];
  menu: [point: { x: number; y: number; centered: boolean }];
}>();

/** 发送状态 → i18n key（避免动态拼接丢失类型检查）。 */
const STATUS_KEY: Record<MessageStatus, MessageKey> = {
  sending: "chat.status.sending",
  delivered: "chat.status.delivered",
  failed: "chat.status.failed",
};

/**
 * 长按行为（v3）：
 * - 移动端（touchOnly）：直接复制，不弹菜单；
 * - 带触摸的桌面设备：仍走菜单。
 */
const longPress = useLongPress((point) => {
  if (props.touchOnly) emit("copy", props.message.id);
  else emit("menu", point);
});

/** PC 右键 → 鼠标附近菜单；移动端只走长按。 */
function openMenu(event: MouseEvent) {
  if (props.touchOnly) return;
  emit("menu", { x: event.clientX, y: event.clientY, centered: false });
}
</script>

<style scoped>
.bubble {
  padding: 8px 12px;
  border-radius: 8px;
  max-width: 80%;
  word-break: break-word;
  /* 长按不触发系统文本选择/高亮，避免与菜单冲突。 */
  -webkit-tap-highlight-color: transparent;
  -webkit-touch-callout: none;
  user-select: none;
  -webkit-user-select: none;
}
.bubble.out {
  align-self: flex-end;
  background: var(--accent);
  color: #fff;
}
.bubble.in {
  align-self: flex-start;
  background: var(--bg);
}
.text {
  margin: 0;
  white-space: pre-wrap;
}
.meta {
  display: flex;
  gap: 8px;
  align-items: center;
  font-size: 12px;
  opacity: 0.75;
  margin-top: 4px;
}
.link {
  border: none;
  background: none;
  color: var(--accent);
  cursor: pointer;
  padding: 0;
  font-size: 12px;
}
.bubble.out .link {
  color: #fff;
  text-decoration: underline;
}
/* 发送方状态（已送达/发送失败）右对齐（v3）。 */
.bubble.out .meta {
  justify-content: flex-end;
}
</style>
