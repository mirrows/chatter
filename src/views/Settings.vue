<template>
  <section class="settings">
    <h2>{{ t("settings.title") }}</h2>

    <p v-if="store.error" class="error" data-testid="settings-error">{{ store.error }}</p>
    <p v-if="store.notice === 'saved'" class="ok" data-testid="settings-saved">
      {{ t("settings.saved") }}
    </p>
    <p v-if="store.restartRequired" class="warn" data-testid="settings-restart">
      {{ t("settings.restart") }}
    </p>

    <label class="field">
      <span>{{ t("settings.nickname") }}</span>
      <input v-model="store.settings.nickname" data-testid="settings-nickname" />
    </label>

    <label class="field">
      <span>{{ t("settings.deviceId") }}</span>
      <input :value="store.settings.deviceId" disabled data-testid="settings-device-id" />
    </label>

    <label class="field">
      <span>{{ t("settings.downloadDir") }}</span>
      <input v-model="store.settings.downloadDir" data-testid="settings-download-dir" />
    </label>

    <div class="row">
      <label class="field">
        <span>{{ t("settings.discoveryPort") }}</span>
        <input
          v-model.number="store.settings.discoveryPort"
          type="number"
          min="1"
          max="65535"
          data-testid="settings-discovery-port"
        />
      </label>
      <label class="field">
        <span>{{ t("settings.tcpPort") }}</span>
        <input
          v-model.number="store.settings.tcpPort"
          type="number"
          min="1"
          max="65535"
          data-testid="settings-tcp-port"
        />
      </label>
    </div>
    <p class="hint">{{ t("settings.portsHint") }}</p>

    <label class="field">
      <span>{{ t("settings.theme") }}</span>
      <select v-model="store.settings.theme" data-testid="settings-theme">
        <option value="light">{{ t("settings.theme.light") }}</option>
        <option value="dark">{{ t("settings.theme.dark") }}</option>
      </select>
    </label>

    <label class="check">
      <input
        v-model="store.settings.autoAcceptImages"
        type="checkbox"
        data-testid="settings-auto-images"
      />
      {{ t("settings.autoImages") }}
    </label>
    <label class="check">
      <input
        v-model="store.settings.autoAcceptFiles"
        type="checkbox"
        data-testid="settings-auto-files"
      />
      {{ t("settings.autoFiles") }}
    </label>
    <label class="check">
      <input
        :checked="store.autostart"
        type="checkbox"
        data-testid="settings-autostart"
        @change="onAutostart"
      />
      {{ t("settings.autostart") }}
    </label>

    <div class="actions">
      <button data-testid="settings-save" :disabled="store.saving" @click="save">
        {{ t("settings.save") }}
      </button>
      <button
        class="secondary"
        data-testid="settings-clear-history"
        @click="store.clearAllHistory()"
      >
        {{ t("settings.clearHistory") }}
      </button>
    </div>
    <p v-if="store.notice === 'historyCleared'" class="ok" data-testid="settings-history-cleared">
      {{ t("settings.historyCleared") }}
    </p>
  </section>
</template>

<script setup lang="ts">
import { onMounted, watch } from "vue";
import { t } from "../i18n";
import { useSettingsStore } from "../stores/settings";

const store = useSettingsStore();

onMounted(async () => {
  await store.load();
  store.applyTheme();
});

// 主题改动即时生效（无需保存即可预览）。
watch(
  () => store.settings.theme,
  () => store.applyTheme(),
);

/** 保存设置并应用主题。 */
async function save() {
  await store.save();
  store.applyTheme();
}

/** 自启开关（F50）。 */
function onAutostart(event: Event) {
  const target = event.target as HTMLInputElement;
  void store.toggleAutostart(target.checked);
}
</script>

<style scoped>
.settings {
  background: var(--card);
  border-radius: 8px;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
h2 {
  margin: 0;
  font-size: 18px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 13px;
  color: var(--muted);
}
input,
select {
  font: inherit;
  padding: 6px 8px;
  border-radius: 6px;
  border: 1px solid var(--bg);
  background: var(--bg);
  color: var(--text);
}
input:disabled {
  opacity: 0.6;
}
.row {
  display: flex;
  gap: 12px;
}
.row .field {
  flex: 1;
}
.check {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 14px;
}
.actions {
  display: flex;
  gap: 8px;
}
button {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  background: var(--accent);
  color: #fff;
  cursor: pointer;
}
button.secondary {
  background: transparent;
  color: var(--accent);
  border: 1px solid var(--accent);
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.hint,
.ok,
.warn,
.error {
  margin: 0;
  font-size: 13px;
  color: var(--muted);
}
.ok {
  color: #15803d;
}
.warn {
  color: #b45309;
}
.error {
  color: #c2410c;
}
</style>
