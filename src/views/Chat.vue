<template>
  <section class="chat">
    <header class="chat-header">
      <button
        class="back"
        data-testid="chat-back"
        :aria-label="t('chat.back')"
        :title="t('chat.back')"
        @click="emit('back')"
      >
        <svg viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M15 5l-7 7 7 7"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </button>
      <div class="titles">
        <h2 data-testid="chat-title">{{ t("chat.title", { name: peerName }) }}</h2>
        <p class="muted" data-testid="chat-connection">{{ connectionText }}</p>
      </div>
    </header>

    <div class="body">
      <!-- 通用提示条（v3）：错误/传输/粘贴/文件夹提示统一走 NoticeBar 组件 -->
      <NoticeBar
        :key="notice?.key ?? 'none'"
        :text="notice?.text ?? ''"
        :testid="notice ? `chat-notice-${notice.key}` : undefined"
        :closable="true"
        @close="dismissNotice"
      />

      <p v-if="timeline.length === 0" class="empty" data-testid="chat-empty">
        {{ t("chat.empty") }}
      </p>
      <ul
        v-else
        ref="timelineEl"
        class="timeline"
        data-testid="chat-timeline"
        @scroll.passive="onTimelineScroll"
      >
        <!-- 向上无限滚动：顶部加载提示 / 没有更多 -->
        <li v-if="loadingHistory" class="history-hint" data-testid="chat-history-loading">
          {{ t("chat.loadingMore") }}
        </li>
        <li v-else-if="!hasMoreHistory" class="history-hint" data-testid="chat-history-end">
          {{ t("chat.noMore") }}
        </li>
        <template v-for="entry in timeline" :key="entry.key">
          <li v-if="entry.key === dividerKey" class="new-divider" data-testid="chat-new-divider">
            <span>{{ t("chat.newMessages") }}</span>
          </li>
          <MessageBubble
            v-if="entry.kind === 'text' && entry.message"
            :message="entry.message"
            :touch-only="isAndroid"
            @retry="store.retry"
            @copy="copyMessage"
            @menu="(point) => openMessageMenu(entry, point)"
          />
          <TransferBubble
            v-else-if="entry.transfer"
            :transfer="entry.transfer"
            :preview-url="transfers.previews[entry.transfer.transferId]"
            :preview-error="transfers.previewErrors[entry.transfer.transferId]"
            :touch-only="isAndroid"
            :hide-retry="!transfers.isSessionTask(entry.transfer.transferId)"
            @accept="transfers.accept"
            @reject="transfers.reject"
            @cancel="transfers.cancel"
            @retry="transfers.retry"
            @preview="onPreview"
            @open="onTransferOpen(entry)"
            @menu="(point) => openTransferMenu(entry, point)"
          />
        </template>
      </ul>
    </div>

    <form class="composer" @submit.prevent="submit">
      <textarea
        v-model="draft"
        rows="2"
        :placeholder="placeholder"
        data-testid="chat-input"
        @keydown.ctrl.enter.prevent="submit"
        @paste="onPaste"
      />
      <div class="composer-actions">
        <div class="left-actions">
          <button
            type="button"
            class="icon-button"
            data-testid="chat-attach-image"
            :title="t('chat.attachImage')"
            :aria-label="t('chat.attachImage')"
            @click="transfers.pickAndSend(peerId, true)"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M4 5h16v14H4zM4 15l4-4 3 3 3-3 6 6"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linejoin="round"
              />
            </svg>
          </button>
          <button
            type="button"
            class="icon-button"
            data-testid="chat-attach-file"
            :title="t('chat.attachFile')"
            :aria-label="t('chat.attachFile')"
            @click="transfers.pickAndSend(peerId, false)"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M6 3h7l5 5v13H6zM13 3v5h5"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linejoin="round"
              />
            </svg>
          </button>
          <button
            type="button"
            class="icon-button"
            data-testid="chat-clear"
            :title="t('chat.clearHistory')"
            :aria-label="t('chat.clearHistory')"
            @click="clearConversation"
          >
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path
                d="M5 7h14M9 7V5h6v2M7 7l1 13h8l1-13M10 11v6M14 11v6"
                fill="none"
                stroke="currentColor"
                stroke-width="1.6"
                stroke-linecap="round"
                stroke-linejoin="round"
              />
            </svg>
          </button>
        </div>
        <button type="submit" class="send" :disabled="!draft.trim()" data-testid="chat-send">
          {{ t("chat.send") }}
        </button>
      </div>
    </form>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { UnlistenFn } from "@tauri-apps/api/event";
import MessageBubble from "../components/MessageBubble.vue";
import NoticeBar from "../components/NoticeBar.vue";
import TransferBubble from "../components/TransferBubble.vue";
import { clearHistory, getHistoryPage, isTauri, saveClipboardImage } from "../api/tauri";
import type { TransferInfo } from "../api/types";
import { t } from "../i18n";
import { confirmDialog } from "../stores/confirm";
import { copyText } from "../utils/clipboard";
import { friendlyError, reportUnknownError } from "../utils/errors";
import { imageFileFromItems, readFileAsBase64 } from "../utils/paste";
import { useChatStore, type ChatMessage } from "../stores/chat";
import { useContextMenuStore, type ContextMenuAction } from "../stores/contextMenu";
import { useImagePreviewStore } from "../stores/imagePreview";
import { message as toast } from "../stores/message";
import { usePlatform } from "../composables/usePlatform";
import { useTransferStore } from "../stores/transfers";

const props = defineProps<{ peerId: string; peerName: string }>();
const emit = defineEmits<{ back: [] }>();

/** 时间线条目：文本与传输任务统一按时间排序（v3 需求 2）。 */
interface TimelineEntry {
  kind: "text" | "transfer";
  key: string;
  ts: number;
  message?: ChatMessage;
  transfer?: TransferInfo;
}

const store = useChatStore();
const transfers = useTransferStore();
const contextMenu = useContextMenuStore();
const imagePreview = useImagePreviewStore();
const { isAndroid } = usePlatform();
const draft = ref("");
const pasteError = ref("");
let unlistenDrop: UnlistenFn | null = null;

/** 输入框占位文案：移动端没有 Ctrl+Enter，去掉快捷键提示。 */
const placeholder = computed(() =>
  isAndroid.value ? t("chat.placeholderMobile") : t("chat.placeholder"),
);

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

/**
 * 时间线：文本与图片/文件统一按创建时间升序（v3 需求 2）。
 *
 * 同一时间戳用 `key` 兜底排序，保证渲染顺序稳定（旧历史缺 ts 时 ts=0 排最前）。
 */
const timeline = computed<TimelineEntry[]>(() => {
  const entries: TimelineEntry[] = [
    ...messages.value.map((message) => ({
      kind: "text" as const,
      key: `m-${message.id}`,
      ts: message.ts,
      message,
    })),
    ...peerTransfers.value.map((transfer) => ({
      kind: "transfer" as const,
      key: `t-${transfer.transferId}`,
      ts: transfer.ts ?? 0,
      transfer,
    })),
  ];
  return entries.sort((a, b) => a.ts - b.ts || a.key.localeCompare(b.key));
});

/**
 * 当前需要展示的提示（同一时刻只显示一条，关闭后自动展示下一条）。
 *
 * v3：未知错误统一走友好兜底文案，不把错误码/英文技术细节展示给用户。
 */
const notice = computed(() => {
  if (error.value)
    return { key: "error" as const, text: t("chat.error", { reason: friendlyError(error.value) }) };
  if (transfers.error)
    return {
      key: "transfer" as const,
      text: t("chat.transferFailed", { reason: friendlyError(transfers.error) }),
    };
  if (pasteError.value)
    return {
      key: "paste" as const,
      text: t("chat.pasteFailed", { reason: friendlyError(pasteError.value) }),
    };
  if (transfers.folderRejected > 0)
    return {
      key: "folder" as const,
      text: t("chat.folderNotSupported", { count: transfers.folderRejected }),
    };
  return null;
});

/** 关闭当前提示：按来源清空各自状态，浮层随之切换到下一条。 */
function dismissNotice() {
  switch (notice.value?.key) {
    case "error":
      store.error = "";
      break;
    case "transfer":
      transfers.error = "";
      break;
    case "paste":
      pasteError.value = "";
      break;
    case "folder":
      transfers.clearFolderNotice();
      break;
  }
}

/** 打开图片预览（全局单例弹窗）。 */
function onPreview(src: string) {
  imagePreview.open(src);
}

/**
 * 点击已完成的文件：用本地路径打开（v3）。
 *
 * 发送方打开自己的源文件（历史里有 `sourcePath`，可随时找回），
 * 接收方打开落盘文件；文件被移动/清理的提示由 transfer store 按方向给出。
 */
function onTransferOpen(entry: TimelineEntry) {
  const transfer = entry.transfer;
  if (!transfer || transfer.state !== "done" || transfer.kind !== "file") return;
  void transfers.openFile(transfer.transferId);
}

/**
 * 对话区滚动策略（v3 需求 2 / 用户二次澄清）：
 *
 * 1. 停在底部时：任何新增内容（收到/发送的文字、图片、文件）都置底一次；
 * 2. 用户上滑翻历史时：新增内容不打断滚动，改为在「新内容起点」插入一条分割线；
 *    但**收到的文件除外**（需要用户点接受/取消，因此强制置底并清掉分割线）；
 * 3. 再次滑到底部：清除分割线，恢复「新增即置底」；
 * 4. 有分割线时继续上滑：清掉旧分割线，之后的新内容重新计算起点。
 */
const timelineEl = ref<HTMLElement | null>(null);
const timelineSignature = computed(() => timeline.value.map((entry) => entry.key).join("|"));
/** 已经出现在时间线里的条目 key（用于判断「新增」的是哪一条）。 */
const knownKeys = new Set<string>();
/** 首次载入（含历史加载完成）是否已置底。 */
let initialScrollDone = false;
/** 距离底部多少像素内视为「在底部」。 */
const BOTTOM_THRESHOLD = 24;
/** 距离顶部多少像素内触发加载更早一页。 */
const HISTORY_TOP_THRESHOLD = 32;
/** 当前是否停在底部（决定新增内容是否自动置底）。 */
const atBottom = ref(true);
/** 分割线插在哪条新内容之前（null 表示无分割线）。 */
const dividerKey = ref<string | null>(null);
/** 上一次滚动位置，用于识别「继续往上滑」。 */
let lastScrollTop = 0;
/** 向上翻页预置历史期间禁止自动置底（watcher 立即执行，故须在 watch 之前声明）。 */
let suppressAutoScroll = false;

function scrollToBottom() {
  const el = timelineEl.value;
  if (!el) return;
  el.scrollTop = el.scrollHeight;
  lastScrollTop = el.scrollTop;
}

/** 监听滚动：维护「是否在底部」，并在上滑时清除旧分割线。 */
function onTimelineScroll() {
  const el = timelineEl.value;
  if (!el) return;
  const scrollTop = el.scrollTop;
  const scrollingUp = scrollTop < lastScrollTop - 1;
  lastScrollTop = scrollTop;

  const nearBottom = el.scrollHeight - scrollTop - el.clientHeight <= BOTTOM_THRESHOLD;
  atBottom.value = nearBottom;
  if (nearBottom) {
    // 回到底部：清除分割线并恢复自动置底。
    dividerKey.value = null;
  } else if (scrollingUp) {
    // 继续往上翻：旧分割线失效，后续新内容重新计算起点。
    dividerKey.value = null;
  }

  // 接近顶部：向上加载更早一页（无限滚动）。
  if (scrollTop <= HISTORY_TOP_THRESHOLD) void loadOlderHistory();
}

/** 需要用户确认接收的文件：这类新内容强制置底。 */
function isIncomingFile(entry: TimelineEntry): boolean {
  return (
    entry.kind === "transfer" &&
    entry.transfer?.direction === "receive" &&
    entry.transfer?.kind === "file"
  );
}

watch(
  timelineSignature,
  () => {
    const entries = timeline.value;
    const added = entries.filter((entry) => !knownKeys.has(entry.key));
    for (const entry of entries) knownKeys.add(entry.key);
    if (added.length === 0) return;
    // 向上翻页预置历史：保持当前视口，不自动置底。
    if (suppressAutoScroll) return;

    // 首次载入（打开会话 / 历史加载完成）：直接置底，不显示分割线。
    if (!initialScrollDone) {
      initialScrollDone = true;
      dividerKey.value = null;
      void nextTick(scrollToBottom);
      return;
    }

    // 上滑翻历史中，且新内容不是「待确认的文件」→ 不打断，只标一条分割线。
    if (!atBottom.value && !added.some(isIncomingFile)) {
      dividerKey.value ??= added[0].key;
      return;
    }

    dividerKey.value = null;
    void nextTick(scrollToBottom);
  },
  { flush: "post", immediate: true },
);

/** 每页条数（v3：默认 100 条/页）。 */
const HISTORY_PAGE_SIZE = 100;
/** 是否还有更早的历史。 */
const hasMoreHistory = ref(true);
/** 是否正在加载历史（首屏或向上翻页）。 */
const loadingHistory = ref(false);

/** 拉取一页历史并写入各 store；返回新增条目数。 */
async function loadHistoryPage(beforeTs?: number): Promise<number> {
  const page = await getHistoryPage(props.peerId, beforeTs, HISTORY_PAGE_SIZE);
  for (const item of page.items) {
    if (item.itemType === "message") store.ingestMessage(item);
    else transfers.ingestHistory(item);
  }
  hasMoreHistory.value = page.hasMore;
  return page.items.length;
}

/** 首次载入：取最新一页（置底由时间线 watcher 完成）。 */
async function loadInitialHistory() {
  loadingHistory.value = true;
  try {
    await loadHistoryPage();
  } catch (err) {
    store.error = err instanceof Error ? err.message : String(err);
  } finally {
    loadingHistory.value = false;
  }
}

/** 向上翻页：加载更早一页并保持视口内容不跳动。 */
async function loadOlderHistory() {
  if (loadingHistory.value || !hasMoreHistory.value) return;
  const el = timelineEl.value;
  if (!el || timeline.value.length === 0) return;

  loadingHistory.value = true;
  suppressAutoScroll = true;
  const previousHeight = el.scrollHeight;
  const previousTop = el.scrollTop;
  try {
    const added = await loadHistoryPage(timeline.value[0].ts);
    if (added > 0) {
      await nextTick();
      el.scrollTop = el.scrollHeight - previousHeight + previousTop;
      lastScrollTop = el.scrollTop;
    }
  } catch (err) {
    store.error = err instanceof Error ? err.message : String(err);
  } finally {
    suppressAutoScroll = false;
    loadingHistory.value = false;
  }
}

/**
 * 传输气泡的右键/长按菜单（v3）。
 *
 * - 只对已完成的任务出菜单（发送/接收都支持）；
 * - 图片：点击即可预览，菜单只保留「另存为」；
 * - 文件：菜单提供「打开文件」+「另存为」；
 * - 「打开所在文件夹」一律移除。
 */
function openTransferMenu(
  entry: TimelineEntry,
  point: { x: number; y: number; centered: boolean },
) {
  const transfer = entry.transfer;
  if (!transfer) return;
  if (transfer.state !== "done") return;

  const items: ContextMenuAction[] = [];
  if (transfer.kind === "file") {
    items.push({
      key: "open",
      label: t("transfer.open"),
      onSelect: async () => {
        await transfers.openFile(transfer.transferId);
      },
    });
  }
  items.push({
    key: "saveAs",
    label: t("transfer.saveAs"),
    onSelect: async () => {
      await transfers.saveAs(transfer.transferId);
    },
  });

  if (point.centered) contextMenu.openCentered(items);
  else contextMenu.openAtPointer({ x: point.x, y: point.y }, items);
}

// 已完成的图片自动取回预览数据；失败原因由 TransferBubble 写入 title（F23）。
watch(
  peerTransfers,
  (items) => {
    for (const item of items) {
      // 发送方的图片在本地，立即可预览；接收方需等落盘完成（v2/M9）。
      if (item.kind === "image" && (item.direction === "send" || item.state === "done")) {
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

/** 复制消息内容（F12）：结果用顶部提示条反馈（v3 需求 8）。 */
async function copyMessage(messageId: string) {
  const target = store.messages.find((item) => item.id === messageId);
  if (!target) return;
  const ok = await copyText(target.text);
  if (ok) toast.success(t("chat.copied"));
  else toast.error(t("chat.copyFailed"));
}

/** 文本气泡菜单：目前只有「复制」。 */
function openMessageMenu(entry: TimelineEntry, point: { x: number; y: number; centered: boolean }) {
  const target = entry.message;
  if (!target) return;
  const items: ContextMenuAction[] = [
    {
      key: "copy",
      label: t("chat.copy"),
      onSelect: () => copyMessage(target.id),
    },
  ];
  if (point.centered) contextMenu.openCentered(items);
  else contextMenu.openAtPointer({ x: point.x, y: point.y }, items);
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

/**
 * 清空本会话语义（F40）：删除该对端历史记录并清空本地消息/任务。
 *
 * 同时复位分页状态：后端已把记录真正删除（`history.json`），
 * 复位后不会再去请求更早的数据，避免清空后又把旧记录翻回来。
 */
async function clearConversation() {
  // 二次确认：清空后不可恢复，统一用通用确认弹窗（v3）。
  const confirmed = await confirmDialog({
    title: t("chat.clearHistory"),
    message: t("chat.clearHistoryConfirm"),
    danger: true,
    confirmText: t("common.ok"),
  });
  if (!confirmed) return;
  try {
    await clearHistory(props.peerId);
    store.clearPeer(props.peerId);
    transfers.clearPeer(props.peerId);
    // 分页与滚动状态复位：已无更早记录，分割线也失效。
    hasMoreHistory.value = false;
    knownKeys.clear();
    dividerKey.value = null;
    atBottom.value = true;
    initialScrollDone = false;
    // 结果用轻提示（R8.1），不再占用对话区浮层。
    toast.success(t("chat.cleared"));
  } catch (err) {
    store.error = friendlyError(err);
    void reportUnknownError("chat.clearHistory", err);
  }
}

// 打开会话时载入历史（F40）、标记当前会话已读（F42）、并订阅拖拽发送（F22/F31）。
onMounted(async () => {
  // 标记为当前会话：此期间收到该对端消息不累计未读（v2/M9）。
  store.setActive(props.peerId);
  store.markRead(props.peerId);
  void loadInitialHistory();
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
  // 离开会话页：之后该对端的消息重新累计未读（v2/M9）。
  store.setActive(null);
  unlistenDrop?.();
  unlistenDrop = null;
});
</script>

<style scoped>
/* 三段式布局：头部固定、对话区自适应铺满、输入区固定在底部（v3）。 */
.chat {
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex: 1;
  min-height: 0;
  background: var(--card);
  border-radius: 8px;
  padding: 10px 10px 8px;
}
.chat-header {
  position: relative;
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 2px 40px;
  min-height: 40px;
}
/* 返回按钮绝对定位，不参与标题/状态的水平居中计算。 */
.back {
  position: absolute;
  left: 0;
  top: 50%;
  transform: translateY(-50%);
  width: 32px;
  height: 32px;
  padding: 0;
  border: none;
  border-radius: 50%;
  background: transparent;
  color: var(--text);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.back:hover {
  background: var(--bg);
}
.back svg {
  width: 22px;
  height: 22px;
}
.titles {
  text-align: center;
  line-height: 1.25;
}
h2 {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
}
.muted {
  color: var(--muted);
  font-size: 12px;
  margin: 2px 0 0;
}
/* 对话区：占满剩余高度；滚动条隐藏（需求 2）。 */
.body {
  position: relative;
  flex: 1;
  min-height: 0;
  height: 0;
  display: flex;
  flex-direction: column;
}
.timeline {
  list-style: none;
  margin: 0;
  /* 左右留出内边距，避免 120px 气泡贴边被裁掉描边（需求 1）。 */
  padding: 6px 8px 8px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex: 1;
  height: 0;
  overflow-y: auto;
  scrollbar-width: none;
  overscroll-behavior: contain;
}
.timeline::-webkit-scrollbar {
  display: none;
}
/* 向上无限滚动：顶部加载提示 / 没有更多。 */
.history-hint {
  list-style: none;
  text-align: center;
  color: var(--muted);
  font-size: 12px;
  padding: 4px 0;
}
/* 新内容分割线：上滑读历史时插入，回到底部或继续上滑后消失。 */
.new-divider {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  margin: 2px 0;
  color: var(--muted);
  font-size: 12px;
  list-style: none;
}
.new-divider::before,
.new-divider::after {
  content: "";
  flex: 1;
  height: 1px;
  background: var(--bg);
}
.empty {
  color: var(--muted);
  margin: 8px;
}
/* 输入区：固定在窗口下方，宽度 100%（需求 3）。 */
.composer {
  flex: 0 0 auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}
textarea {
  width: 100%;
  box-sizing: border-box;
  resize: none;
  font: inherit;
  padding: 8px;
  min-height: 56px;
  max-height: 30vh;
  border-radius: 8px;
  border: 1px solid var(--bg);
  background: var(--bg);
  color: var(--text);
}
.composer-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.left-actions {
  display: flex;
  gap: 4px;
}
.icon-button {
  width: 34px;
  height: 34px;
  padding: 0;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}
.icon-button:hover {
  background: var(--bg);
  color: var(--accent);
}
.icon-button svg {
  width: 20px;
  height: 20px;
}
.send {
  margin-left: auto;
  padding: 8px 18px;
  border: none;
  border-radius: 8px;
  background: var(--accent);
  color: #fff;
  font: inherit;
  cursor: pointer;
}
.send:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
