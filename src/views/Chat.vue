<template>
  <section class="chat">
    <header class="chat-header">
      <button class="link" data-testid="chat-back" @click="emit('back')">
        {{ t("chat.back") }}
      </button>
      <h2 data-testid="chat-title">{{ t("chat.title", { name: peerName }) }}</h2>
      <p class="muted" data-testid="chat-connection">
        {{ connectionText }}
      </p>
      <div class="attach">
        <button
          class="link"
          data-testid="chat-attach-image"
          @click="transfers.pickAndSend(peerId, true)"
        >
          {{ t("chat.attachImage") }}
        </button>
        <button
          class="link"
          data-testid="chat-attach-file"
          @click="transfers.pickAndSend(peerId, false)"
        >
          {{ t("chat.attachFile") }}
        </button>
        <button class="link" data-testid="chat-clear" @click="clearConversation">
          {{ t("chat.clearHistory") }}
        </button>
      </div>
    </header>

    <p v-if="error" class="error" data-testid="chat-error">
      {{ t("chat.error", { reason: error }) }}
    </p>

    <p v-if="transfers.folderRejected > 0" class="warn" data-testid="folder-notice">
      {{ t("chat.folderNotSupported", { count: transfers.folderRejected }) }}
      <button class="link" data-testid="folder-notice-close" @click="transfers.clearFolderNotice()">
        ×
      </button>
    </p>

    <p v-if="pasteError" class="error" data-testid="paste-error">
      {{ t("chat.pasteFailed", { reason: pasteError }) }}
    </p>

    <p v-if="cleared" class="ok" data-testid="chat-cleared">{{ t("chat.cleared") }}</p>

    <section v-if="peerTransfers.length > 0" class="transfers">
      <TransferCard
        v-for="item in peerTransfers"
        :key="item.transferId"
        :transfer="item"
        :preview-url="transfers.previews[item.transferId]"
        @accept="transfers.accept"
        @reject="transfers.reject"
        @cancel="transfers.cancel"
        @retry="transfers.retry"
        @open="transfers.openFile"
        @reveal="transfers.revealFile"
        @save-as="transfers.saveAs"
      />
    </section>

    <p v-if="messages.length === 0" class="empty">{{ t("chat.empty") }}</p>
    <ul v-else class="messages">
      <li
        v-for="m in messages"
        :key="m.id"
        class="bubble"
        :class="m.direction"
        :data-testid="`message-${m.direction}`"
      >
        <p class="text">{{ m.text }}</p>
        <span class="meta">
          <template v-if="m.direction === 'out'">{{ t(STATUS_KEY[m.status]) }}</template>
          <button
            v-if="m.direction === 'out' && m.status === 'failed'"
            class="link"
            data-testid="chat-retry"
            @click="store.retry(m.id)"
          >
            {{ t("chat.retry") }}
          </button>
          <button class="link" data-testid="message-copy" @click="copyMessage(m)">
            {{ t("chat.copy") }}
          </button>
          <span v-if="copiedId === m.id" data-testid="message-copied">{{ t("chat.copied") }}</span>
        </span>
      </li>
    </ul>

    <form class="composer" @submit.prevent="submit">
      <textarea
        v-model="draft"
        rows="3"
        :placeholder="t('chat.placeholder')"
        data-testid="chat-input"
        @keydown.ctrl.enter.prevent="submit"
        @paste="onPaste"
      />
      <button type="submit" :disabled="!draft.trim()" data-testid="chat-send">
        {{ t("chat.send") }}
      </button>
    </form>
  </section>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";
import TransferCard from "../components/TransferCard.vue";
import { clearHistory, isTauri, saveClipboardImage } from "../api/tauri";
import { t, type MessageKey } from "../i18n";
import { copyText } from "../utils/clipboard";
import { imageFileFromItems, readFileAsBase64 } from "../utils/paste";
import { useChatStore, type ChatMessage, type MessageStatus } from "../stores/chat";
import { useTransferStore } from "../stores/transfers";

const props = defineProps<{ peerId: string; peerName: string }>();
const emit = defineEmits<{ back: [] }>();

const store = useChatStore();
const transfers = useTransferStore();
const draft = ref("");
const copiedId = ref("");
const pasteError = ref("");
const cleared = ref(false);
let unlistenDrop: UnlistenFn | null = null;

/** 发送状态 → i18n key（避免动态拼接 key 丢失类型检查）。 */
const STATUS_KEY: Record<MessageStatus, MessageKey> = {
  sending: "chat.status.sending",
  delivered: "chat.status.delivered",
  failed: "chat.status.failed",
};

const messages = computed(() => store.messagesFor(props.peerId));
const connected = computed(() => store.isConnected(props.peerId));
const error = computed(() => store.error);
const peerTransfers = computed(() => transfers.forPeer(props.peerId));
// 连接状态文案（v2/M10）：已连接 / 正在第 n 次重连 / 未连接。
const connectionText = computed(() => {
  if (connected.value) return t("chat.connected");
  const attempt = store.reconnectAttempt(props.peerId);
  return attempt > 0 ? t("chat.reconnecting", { attempt }) : t("chat.disconnected");
});

// 已完成的图片自动取回预览数据；失败时由 TransferCard 提示（F23）。
watch(
  peerTransfers,
  (items) => {
    for (const item of items) {
      if (item.kind === "image" && item.state === "done") {
        void transfers.loadPreview(item.transferId);
      }
    }
  },
  { immediate: true, deep: true },
);

/** 发送输入框内容；Ctrl+Enter 与点击按钮共用。 */
async function submit() {
  const text = draft.value;
  if (!text.trim()) return;
  draft.value = "";
  await store.send(props.peerId, text);
}

/** 复制消息内容（F12）。 */
async function copyMessage(message: ChatMessage) {
  const ok = await copyText(message.text);
  copiedId.value = ok ? message.id : "";
}

/** 粘贴图片直接发送（F21）。 */
async function onPaste(event: ClipboardEvent) {
  const file = imageFileFromItems(event.clipboardData?.items ?? null);
  if (!file) return;
  event.preventDefault();
  pasteError.value = "";
  try {
    const { data, extension } = await readFileAsBase64(file);
    const path = await saveClipboardImage(data, extension);
    await transfers.sendPath(props.peerId, path);
  } catch (err) {
    pasteError.value = err instanceof Error ? err.message : String(err);
  }
}

/** 清空本会话语义（F40）：删除该对端历史记录并清空本地消息/任务。 */
async function clearConversation() {
  try {
    await clearHistory(props.peerId);
    store.clearPeer(props.peerId);
    transfers.clearPeer(props.peerId);
    cleared.value = true;
  } catch (err) {
    store.error = err instanceof Error ? err.message : String(err);
  }
}

// 打开会话时载入历史（F40）、清除未读（F42），并订阅拖拽发送（F22/F31）。
onMounted(async () => {
  store.markRead(props.peerId);
  void store.loadHistory(props.peerId);
  void transfers.loadHistory(props.peerId);
  if (!isTauri()) return;
  try {
    unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "drop") {
        void transfers.sendPaths(props.peerId, event.payload.paths);
      }
    });
  } catch (err) {
    // 浏览器预览或平台不支持拖拽时静默降级（仍可通过按钮选择文件）。
    console.warn("drag-drop unavailable", err);
  }
});

onUnmounted(() => {
  unlistenDrop?.();
  unlistenDrop = null;
});
</script>

<style scoped>
.chat {
  display: flex;
  flex-direction: column;
  gap: 12px;
  background: var(--card);
  border-radius: 8px;
  padding: 16px;
}
.chat-header {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 8px;
}
.attach {
  display: flex;
  gap: 12px;
  margin-left: auto;
}
.transfers {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
.link {
  border: none;
  background: none;
  color: var(--accent);
  cursor: pointer;
  padding: 0;
  font-size: 13px;
}
.muted {
  color: var(--muted);
  font-size: 13px;
  margin: 0;
}
.messages {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 420px;
  overflow-y: auto;
}
.bubble {
  padding: 8px 12px;
  border-radius: 8px;
  max-width: 80%;
  word-break: break-word;
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
.bubble.out .link {
  color: #fff;
  text-decoration: underline;
}
.empty,
.error,
.ok {
  color: var(--muted);
  margin: 0;
}
.ok {
  color: #15803d;
  font-size: 13px;
}
.error {
  color: #c2410c;
  font-size: 13px;
}
.composer {
  display: flex;
  gap: 8px;
  align-items: flex-end;
}
textarea {
  flex: 1;
  resize: vertical;
  font: inherit;
  padding: 8px;
  border-radius: 6px;
  border: 1px solid var(--bg);
  background: var(--bg);
  color: var(--text);
}
button[type="submit"] {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  background: var(--accent);
  color: #fff;
  cursor: pointer;
}
button[type="submit"]:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
