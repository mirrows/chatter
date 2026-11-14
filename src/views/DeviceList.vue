<template>
  <section class="device-list">
    <!-- 添加设备置顶（v3） -->
    <form class="add" @submit.prevent="addByIp">
      <input
        v-model="addrInput"
        :placeholder="t('device.addPlaceholder')"
        data-testid="device-add-input"
      />
      <button type="submit" data-testid="device-add-button">{{ t("device.addByIp") }}</button>
    </form>

    <!-- 局域网设备标题行：右侧显示本机指纹（v3） -->
    <div class="head">
      <h2>{{ t("device.title") }}</h2>
      <span class="count" data-testid="device-count">（{{ store.count }}）</span>
      <span
        v-if="store.localFingerprint"
        class="local-fingerprint"
        data-testid="local-fingerprint"
        :title="t('device.fingerprint', { code: store.localFingerprint })"
      >
        {{ store.localFingerprint }}
      </span>
    </div>

    <!-- 本机名称（可改）：本机立即生效，其他设备下次连接生效（v3） -->
    <p v-if="store.localName" class="muted local" data-testid="local-device">
      {{ t("device.local", { name: store.localName }) }}
      <button
        type="button"
        class="edit"
        data-testid="local-edit-name"
        :title="t('device.editLocalName')"
        :aria-label="t('device.editLocalName')"
        @click="editLocalName"
      >
        ✎
      </button>
    </p>

    <p v-if="store.error" class="error" data-testid="device-error">{{ store.error }}</p>

    <!-- 系统分享待发送（v3）：选择设备后自动发送 -->
    <div v-if="share.pending" class="share-hint" data-testid="device-share-hint">
      <span class="share-text">{{ t("share.pending", { summary: share.summary }) }}</span>
      <button
        type="button"
        class="share-close"
        data-testid="device-share-dismiss"
        @click="share.clear()"
      >
        {{ t("share.dismiss") }}
      </button>
    </div>

    <ul v-if="store.count > 0" class="list" :class="{ 'is-card': isMobile }">
      <li v-for="d in store.devices" :key="d.deviceId" data-testid="device-item">
        <div
          class="device"
          role="button"
          tabindex="0"
          data-testid="device-open"
          @click="emit('select', d)"
          @keydown.enter.prevent="emit('select', d)"
        >
          <div class="line">
            <span class="name" data-testid="device-name">{{ displayName(d) }}</span>
            <button
              type="button"
              class="edit"
              data-testid="device-edit-name"
              :title="t('device.editName')"
              :aria-label="t('device.editName')"
              @click.stop="editName(d)"
            >
              ✎
            </button>
            <span v-if="chat.unreadFor(d.deviceId) > 0" class="badge" data-testid="device-unread">
              {{ chat.unreadFor(d.deviceId) }}
            </span>
          </div>
          <span class="muted">{{ d.addr }}</span>
          <span class="muted" data-testid="device-fingerprint">
            {{ t("device.fingerprint", { code: d.fingerprint }) }}
          </span>
        </div>
      </li>
    </ul>
    <p v-else class="empty">{{ t("device.empty") }}</p>
  </section>
</template>

<script setup lang="ts">
import { computed, ref } from "vue";
import { usePlatform } from "../composables/usePlatform";
import { t } from "../i18n";
import type { DeviceEntry } from "../api/types";
import { useChatStore } from "../stores/chat";
import { useDeviceStore } from "../stores/devices";
import { message } from "../stores/message";
import { usePeerAliasStore } from "../stores/peerAliases";
import { promptDialog } from "../stores/prompt";
import { useSettingsStore } from "../stores/settings";
import { useShareInboxStore } from "../stores/shareInbox";

const emit = defineEmits<{ select: [device: DeviceEntry] }>();

const store = useDeviceStore();
const chat = useChatStore();
const aliases = usePeerAliasStore();
const settings = useSettingsStore();
const share = useShareInboxStore();
const { isAndroid } = usePlatform();
const addrInput = ref("");

/** 移动端用卡片布局（v3 需求 2）。 */
const isMobile = computed(() => isAndroid.value);

/** 设备展示名：自定义别名优先（v3 需求 3）。 */
function displayName(device: DeviceEntry): string {
  return aliases.nameFor(device.deviceId, device.name);
}

/** 修改设备名称：弹窗输入，留空恢复默认；结果缓存到本地（v3 需求 3）。 */
async function editName(device: DeviceEntry) {
  const current = aliases.aliases[device.deviceId] ?? "";
  const value = await promptDialog({
    title: t("device.editName"),
    message: t("device.editNameMessage"),
    value: current,
    placeholder: device.name,
    maxWidth: 360,
  });
  if (value === null) return;
  aliases.setAlias(device.deviceId, value);
  // 修改其他设备的名称只影响本机显示，用轻提示确认（R8.1）。
  message.success(t("device.aliasSaved"));
}

/**
 * 修改本机名称（v3）：写入配置，本机立即显示新名称。
 *
 * 其他设备不做实时回推，它们在下次连接（本应用重启后的广播）时看到新名称。
 */
async function editLocalName() {
  const current = store.localName;
  const value = await promptDialog({
    title: t("device.editLocalName"),
    message: t("device.editLocalNameMessage"),
    value: current,
    maxWidth: 360,
  });
  if (value === null) return;
  const nickname = value.trim();
  if (!nickname) {
    message.error(t("device.editLocalNameEmpty"));
    return;
  }
  if (nickname === current) return;
  const ok = await settings.renameLocal(nickname);
  if (!ok) {
    message.error(settings.error || t("device.editLocalNameFailed"));
    return;
  }
  store.setLocalName(nickname);
  message.success(t("device.editLocalNameSaved"));
}

// 上下线提示统一由 devices store 通过 message 组件弹出（v3 / R8.1），此处不再维护计时器。

/** 手动添加设备（F3）。 */
async function addByIp() {
  const ok = await store.addByIp(addrInput.value);
  if (ok) addrInput.value = "";
}
</script>

<style scoped>
.device-list {
  position: relative;
  background: var(--card);
  border-radius: 8px;
  padding: 16px;
}
/* 标题行：右侧显示本机指纹（v3） */
.local-fingerprint {
  flex: 0 0 auto;
  margin-left: auto;
  font-size: 12px;
  color: var(--muted);
  font-family: "Consolas", "Menlo", monospace;
  letter-spacing: 0.5px;
  cursor: default;
}
.local {
  margin: 0 0 4px;
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}
.head {
  display: flex;
  align-items: baseline;
  gap: 4px;
  margin: 10px 0 4px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
.count {
  font-size: 14px;
  color: var(--muted);
}
.list {
  list-style: none;
  margin: 8px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
/* 桌面端：单列列表行 */
.list .device {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 4px;
  border-bottom: 1px solid var(--bg);
  cursor: pointer;
}
.list .device:hover {
  background: var(--bg);
}
/* 移动端：卡片网格，放不下自动换行（v3 需求 2） */
.list.is-card {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
  gap: 10px;
}
.list.is-card li {
  min-width: 0;
}
.list.is-card .device {
  height: 100%;
  padding: 12px;
  border-radius: 10px;
  border-bottom: none;
  background: var(--bg);
  box-sizing: border-box;
}
.list.is-card .device:hover {
  background: var(--card);
  box-shadow: 0 0 0 2px var(--accent);
}
.line {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.name {
  font-size: 14px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.edit {
  flex: 0 0 auto;
  padding: 0 4px;
  border: none;
  background: transparent;
  color: var(--muted);
  font-size: 13px;
  line-height: 1;
  cursor: pointer;
}
.edit:hover {
  color: var(--accent);
}
.muted {
  color: var(--muted);
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.empty {
  color: var(--muted);
}
.error {
  color: #c2410c;
  font-size: 13px;
}
/* 系统分享待发送提示（v3） */
.share-hint {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 8px;
  padding: 6px 10px;
  border-radius: 8px;
  background: var(--bg);
  font-size: 13px;
}
.share-text {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.share-close {
  flex: 0 0 auto;
  padding: 0 4px;
  border: none;
  background: transparent;
  color: var(--accent);
  font-size: 13px;
  cursor: pointer;
}
.badge {
  background: var(--accent);
  color: #fff;
  border-radius: 10px;
  padding: 1px 8px;
  font-size: 12px;
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
