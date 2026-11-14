<template>
  <main class="app">
    <nav v-if="!activePeer" class="topbar">
      <button
        :class="['tab', { active: view === 'devices' }]"
        data-testid="tab-devices"
        @click="view = 'devices'"
      >
        {{ t("nav.devices") }}
      </button>
      <button
        :class="['tab', { active: view === 'settings' }]"
        data-testid="tab-settings"
        @click="view = 'settings'"
      >
        {{ t("nav.settings") }}
      </button>
    </nav>

    <div class="pane">
      <Chat
        v-if="activePeer"
        :peer-id="activePeer.deviceId"
        :peer-name="aliases.nameFor(activePeer.deviceId, activePeer.name)"
        @back="activePeer = null"
      />
      <Settings v-else-if="view === 'settings'" />
      <DeviceList v-else @select="openChat" />
    </div>

    <!-- 全局单例浮层（v3）：右键/长按菜单、图片预览、确认弹窗、提示条 -->
    <ContextMenuHost />
    <ImagePreviewHost />
    <ConfirmHost />
    <PromptHost />
    <MessageHost />
  </main>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import Chat from "./views/Chat.vue";
import ConfirmHost from "./components/ConfirmHost.vue";
import ContextMenuHost from "./components/ContextMenuHost.vue";
import DeviceList from "./views/DeviceList.vue";
import ImagePreviewHost from "./components/ImagePreviewHost.vue";
import MessageHost from "./components/MessageHost.vue";
import PromptHost from "./components/PromptHost.vue";
import Settings from "./views/Settings.vue";
import type { DeviceEntry } from "./api/types";
import { t } from "./i18n";
import { useChatStore } from "./stores/chat";
import { useDeviceStore } from "./stores/devices";
import { usePeerAliasStore } from "./stores/peerAliases";
import { useSettingsStore } from "./stores/settings";
import { useTransferStore } from "./stores/transfers";
import { blockContextMenu } from "./utils/contextmenu";

const devices = useDeviceStore();
const chat = useChatStore();
const transfers = useTransferStore();
const settings = useSettingsStore();
const aliases = usePeerAliasStore();
const activePeer = ref<DeviceEntry | null>(null);
const view = ref<"devices" | "settings">("devices");
let unblockContextMenu: (() => void) | null = null;

/** 打开与某台设备的会话（F10）。 */
function openChat(device: DeviceEntry) {
  activePeer.value = device;
}

/**
 * 页面重新可见/获得焦点时刷新设备列表。
 *
 * 移动端切后台后 WebView 会暂停，期间 `device://update` 的离线事件可能丢失，
 * 导致界面残留已从后端设备表移除的对端（v2/M9 真机「peer not found」）。
 */
function onActive() {
  if (document.visibilityState !== "visible") return;
  void devices.refresh();
}

// 启动时拉取设备快照、订阅设备/消息/传输事件，并应用主题（F1/F4/F10/F33/F52）。
onMounted(() => {
  void devices.init().then(() => {
    // Windows 上禁用 WebView 默认右键菜单（v2/M9：发布版不需要「检查元素/另存为」）。
    if (devices.isWindows && !unblockContextMenu) {
      unblockContextMenu = blockContextMenu();
    }
  });
  void chat.init();
  void transfers.init();
  void settings.load().then(() => settings.applyTheme());
  document.addEventListener("visibilitychange", onActive);
  window.addEventListener("focus", onActive);
});

onUnmounted(() => {
  document.removeEventListener("visibilitychange", onActive);
  window.removeEventListener("focus", onActive);
  unblockContextMenu?.();
  unblockContextMenu = null;
});
</script>

<style>
:root {
  --bg: #f5f7fa;
  --card: #ffffff;
  --text: #1f2937;
  --muted: #6b7280;
  --accent: #1854a8;
}
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --bg: #111827;
    --card: #1f2937;
    --text: #f3f4f6;
    --muted: #9ca3af;
    --accent: #6ea8ff;
  }
}
:root[data-theme="dark"] {
  --bg: #111827;
  --card: #1f2937;
  --text: #f3f4f6;
  --muted: #9ca3af;
  --accent: #6ea8ff;
}
:root[data-theme="light"] {
  --bg: #f5f7fa;
  --card: #ffffff;
  --text: #1f2937;
  --muted: #6b7280;
  --accent: #1854a8;
}
.topbar {
  display: flex;
  gap: 8px;
  margin-bottom: 12px;
}
.tab {
  border: none;
  background: var(--card);
  color: var(--muted);
  padding: 6px 14px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}
.tab.active {
  background: var(--accent);
  color: #fff;
}
* {
  box-sizing: border-box;
}
html,
body,
#app {
  height: 100%;
}
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
  /* 整页不允许滚动，滚动交给各视图内部（v3 布局）。 */
  overflow: hidden;
  overscroll-behavior: none;
  /* 禁止双指缩放（图片预览内部单独开启），只保留平移滚动。 */
  touch-action: pan-x pan-y;
  -webkit-tap-highlight-color: transparent;
}
.app {
  height: 100dvh;
  max-width: 720px;
  margin: 0 auto;
  /* 适配刘海屏/状态栏/导航栏/手势条：不同机型统一由系统安全区决定。 */
  padding: max(8px, env(safe-area-inset-top)) max(8px, env(safe-area-inset-right))
    max(8px, env(safe-area-inset-bottom)) max(8px, env(safe-area-inset-left));
  display: flex;
  flex-direction: column;
  gap: 8px;
}
/* 内容区：占满剩余高度；设备/设置页超高时在这里滚动。 */
.pane {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  scrollbar-width: none;
}
.pane::-webkit-scrollbar {
  display: none;
}
.topbar {
  display: flex;
  gap: 8px;
  flex: 0 0 auto;
}
</style>
