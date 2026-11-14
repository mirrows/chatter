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

    <div class="field">
      <span>{{ t("settings.downloadDir") }}</span>
      <div class="field-row">
        <!-- 只读：点击弹系统目录选择框（v3 需求 1） -->
        <input
          :value="store.settings.downloadDir"
          readonly
          :class="{ readonly: true, locked: isAndroid }"
          data-testid="settings-download-dir"
          :title="isAndroid ? t('settings.downloadDirLocked') : t('settings.changeDir')"
          @click="pickDownloadDir"
        />
        <button
          v-if="!isAndroid"
          type="button"
          class="secondary"
          data-testid="settings-open-download-dir"
          @click="openDownloadDir"
        >
          {{ t("settings.openFolder") }}
        </button>
      </div>
    </div>

    <!-- 错误日志（v3）：日志目录里有内容才显示；桌面端点击打开文件夹，移动端展示路径 -->
    <div v-if="errorLog.info.hasLogs" class="field" data-testid="settings-error-log">
      <span>{{ t("error.logHint") }}</span>
      <div class="field-row">
        <input
          v-if="isAndroid"
          :value="errorLog.info.dir"
          readonly
          class="readonly locked"
          data-testid="settings-error-log-path"
        />
        <button
          v-else
          type="button"
          class="secondary"
          data-testid="settings-error-log-open"
          @click="openErrorLogDir"
        >
          {{ t("error.logOpen") }}
        </button>
      </div>
    </div>

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

    <!-- 定时清理（v3，仅移动端）：默认关闭，开启后每次启动清理超期接收文件 -->
    <div v-if="isAndroid" class="field" data-testid="settings-auto-clean">
      <span>{{ t("settings.autoClean") }}</span>
      <div class="field-row">
        <label class="check inline">
          <input
            v-model="store.settings.autoCleanEnabled"
            type="checkbox"
            data-testid="settings-auto-clean-toggle"
          />
          {{ t("settings.autoCleanLabel") }}
        </label>
        <input
          v-model.number="store.settings.autoCleanDays"
          type="number"
          class="days"
          min="1"
          step="1"
          inputmode="numeric"
          data-testid="settings-auto-clean-days"
          :disabled="!store.settings.autoCleanEnabled"
          @input="onCleanDaysInput"
          @blur="onCleanDaysInput"
        />
        <span class="unit">{{ t("settings.autoCleanUnit") }}</span>
      </div>
      <p class="hint" data-testid="settings-auto-clean-hint">
        {{ t("settings.autoCleanHint") }}
      </p>
    </div>
    <label class="check">
      <input
        :checked="store.autostart"
        type="checkbox"
        data-testid="settings-autostart"
        @change="onAutostart"
      />
      {{ t("settings.autostart") }}
    </label>

    <label class="check">
      <input v-model="store.deleteFiles" type="checkbox" data-testid="settings-delete-files" />
      {{ t("settings.deleteFiles") }}
    </label>

    <div class="actions">
      <button data-testid="settings-save" :disabled="store.saving" @click="save">
        {{ t("settings.save") }}
      </button>
      <button class="secondary" data-testid="settings-clear-history" @click="clearAllHistory">
        {{ t("settings.clearHistory") }}
      </button>
    </div>
  </section>
</template>

<script setup lang="ts">
import { onMounted, watch } from "vue";
import { usePlatform } from "../composables/usePlatform";
import { t } from "../i18n";
import { confirmDialog } from "../stores/confirm";
import { useErrorLogStore } from "../stores/errorLog";
import { message } from "../stores/message";
import { normalizeCleanDays, useSettingsStore } from "../stores/settings";

const store = useSettingsStore();
const errorLog = useErrorLogStore();
const { isAndroid } = usePlatform();

onMounted(async () => {
  await store.load();
  store.applyTheme();
  // 错误日志入口只在目录里已有日志时显示（v3）。
  void errorLog.refresh();
});

/** 打开下载目录（v3）：桌面端在文件管理器中打开。 */
async function openDownloadDir() {
  await store.openDownloadDir();
}

/** 点击下载目录输入框 → 弹系统目录选择框并即时保存（v3 需求 1）。 */
async function pickDownloadDir() {
  // Android 的下载目录固定在应用私有目录：用轻提示说明（R8.1/R8.3），不写页面级 error。
  if (isAndroid.value) {
    message.info(t("settings.downloadDirLocked"));
    return;
  }
  await store.pickDownloadDir();
}

/** 打开错误日志目录（桌面端）；失败给轻提示。 */
async function openErrorLogDir() {
  const ok = await errorLog.openDir();
  if (!ok) message.error(t("error.logOpenFailed"));
}

/**
 * 定时清理天数输入净化（v3）：只允许大于 0 的整数。
 *
 * 输入为空/0/小数时立即纠正（小数向下取整，非法值回退 7 天），
 * 避免把非法值提交到后端（后端也会拒绝 0）。
 */
function onCleanDaysInput() {
  store.settings.autoCleanDays = normalizeCleanDays(store.settings.autoCleanDays);
}

/**
 * 清空全部历史：始终二次确认，文案随「同时删除文件」勾选变化（v3 需求 7）。
 *
 * 勾选时后端会把下载目录内的文件一并清空（含历史记录里没有的）。
 */
async function clearAllHistory() {
  const confirmed = await confirmDialog({
    title: t("settings.clearHistory"),
    message: store.deleteFiles
      ? t("settings.clearHistoryConfirmFiles")
      : t("settings.clearHistoryConfirm"),
    danger: true,
    confirmText: t("common.ok"),
  });
  if (confirmed) await store.clearAllHistory();
}

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
.field-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.field-row input {
  flex: 1;
}
/* 只读输入：看起来可点击，提示点击后可更改目录（v3） */
.field-row input.readonly {
  cursor: pointer;
  opacity: 0.9;
}
/* 移动端目录不可改：光标恢复默认，避免误以为可点开选择框（v3） */
.field-row input.readonly.locked {
  cursor: default;
}
.field-row button {
  flex: 0 0 auto;
  padding: 6px 12px;
  font-size: 13px;
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
/* 定时清理（v3）：勾选框 + 天数 + 单位同一行 */
.check.inline {
  flex: 0 0 auto;
}
.field-row input.days {
  flex: 0 0 64px;
  width: 64px;
  padding: 6px 8px;
  text-align: center;
}
.field-row .unit {
  flex: 0 0 auto;
  font-size: 13px;
  color: var(--muted);
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
