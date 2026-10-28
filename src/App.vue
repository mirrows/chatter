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

    <Chat
      v-if="activePeer"
      :peer-id="activePeer.deviceId"
      :peer-name="activePeer.name"
      @back="activePeer = null"
    />
    <Settings v-else-if="view === 'settings'" />
    <DeviceList v-else @select="openChat" />
  </main>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import Chat from "./views/Chat.vue";
import DeviceList from "./views/DeviceList.vue";
import Settings from "./views/Settings.vue";
import type { DeviceEntry } from "./api/types";
import { t } from "./i18n";
import { useChatStore } from "./stores/chat";
import { useDeviceStore } from "./stores/devices";
import { useSettingsStore } from "./stores/settings";
import { useTransferStore } from "./stores/transfers";

const devices = useDeviceStore();
const chat = useChatStore();
const transfers = useTransferStore();
const settings = useSettingsStore();
const activePeer = ref<DeviceEntry | null>(null);
const view = ref<"devices" | "settings">("devices");

/** 打开与某台设备的会话（F10）。 */
function openChat(device: DeviceEntry) {
  activePeer.value = device;
}

// 启动时拉取设备快照、订阅设备/消息/传输事件，并应用主题（F1/F4/F10/F33/F52）。
onMounted(() => {
  void devices.init();
  void chat.init();
  void transfers.init();
  void settings.load().then(() => settings.applyTheme());
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
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
}
.app {
  max-width: 720px;
  margin: 0 auto;
  padding: 16px;
}
</style>
