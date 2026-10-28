<template>
  <section class="device-list">
    <h2>{{ t("device.title") }}</h2>
    <p class="muted" data-testid="device-count">
      {{ t("device.count", { count: store.count }) }}
    </p>
    <p v-if="store.localName" class="muted" data-testid="local-device">
      {{ t("device.local", { name: store.localName }) }}
    </p>
    <p v-if="store.error" class="error" data-testid="device-error">{{ store.error }}</p>
    <p v-if="store.notice" class="notice" data-testid="device-notice">
      {{
        store.notice.kind === "up"
          ? t("device.notice.up", { name: store.notice.name })
          : t("device.notice.down", { name: store.notice.name })
      }}
    </p>
    <form class="add" @submit.prevent="addByIp">
      <input
        v-model="addrInput"
        :placeholder="t('device.addPlaceholder')"
        data-testid="device-add-input"
      />
      <button type="submit" data-testid="device-add-button">{{ t("device.addByIp") }}</button>
    </form>
    <ul v-if="store.count > 0">
      <li v-for="d in store.devices" :key="d.deviceId" data-testid="device-item">
        <span class="name">{{ d.name }}</span>
        <span class="muted">{{ d.addr }}</span>
        <button class="link" data-testid="device-open" @click="emit('select', d)">
          {{ t("device.open") }}
        </button>
      </li>
    </ul>
    <p v-else class="empty">{{ t("device.empty") }}</p>
  </section>
</template>

<script setup lang="ts">
import { onUnmounted, ref, watch } from "vue";
import { t } from "../i18n";
import type { DeviceEntry } from "../api/types";
import { useDeviceStore } from "../stores/devices";

const emit = defineEmits<{ select: [device: DeviceEntry] }>();

const store = useDeviceStore();
const addrInput = ref("");
let noticeTimer: ReturnType<typeof setTimeout> | null = null;

// 上下线提示 3 秒后自动消失（F4）。
watch(
  () => store.notice,
  (notice) => {
    if (noticeTimer) clearTimeout(noticeTimer);
    if (!notice) return;
    noticeTimer = setTimeout(() => store.clearNotice(), 3000);
  },
);

onUnmounted(() => {
  if (noticeTimer) clearTimeout(noticeTimer);
});

/** 手动添加设备（F3）。 */
async function addByIp() {
  const ok = await store.addByIp(addrInput.value);
  if (ok) addrInput.value = "";
}
</script>

<style scoped>
.device-list {
  background: var(--card);
  border-radius: 8px;
  padding: 16px;
}
h2 {
  margin: 0 0 8px;
  font-size: 18px;
}
ul {
  list-style: none;
  margin: 0;
  padding: 0;
}
li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 4px;
  border-bottom: 1px solid var(--bg);
}
.link {
  border: none;
  background: none;
  color: var(--accent);
  cursor: pointer;
  font-size: 13px;
  padding: 0;
}
.muted {
  color: var(--muted);
  font-size: 13px;
}
.empty {
  color: var(--muted);
}
.error {
  color: #c2410c;
  font-size: 13px;
}
.notice {
  color: var(--accent);
  font-size: 13px;
  margin: 0 0 8px;
}
.add {
  display: flex;
  gap: 8px;
  margin-bottom: 8px;
}
.add input {
  flex: 1;
  padding: 6px 8px;
  border-radius: 6px;
  border: 1px solid var(--bg);
  background: var(--bg);
  color: var(--text);
  font: inherit;
}
.add button {
  padding: 6px 12px;
  border: none;
  border-radius: 6px;
  background: var(--accent);
  color: #fff;
  cursor: pointer;
  font-size: 13px;
}
</style>
