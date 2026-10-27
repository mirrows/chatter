<template>
  <section class="device-list">
    <h2>局域网设备</h2>
    <p class="muted" data-testid="device-count">在线 {{ store.count }} 台</p>
    <ul v-if="store.count > 0">
      <li v-for="d in store.devices" :key="d.deviceId" data-testid="device-item">
        <span class="name">{{ d.name }}</span>
        <span class="muted">{{ d.addr }}</span>
      </li>
    </ul>
    <p v-else class="empty">正在扫描局域网… 未发现其他设备</p>
  </section>
</template>

<script setup lang="ts">
import { useDeviceStore } from "../stores/devices";

const store = useDeviceStore();
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
  justify-content: space-between;
  padding: 8px 4px;
  border-bottom: 1px solid var(--bg);
}
.muted {
  color: var(--muted);
  font-size: 13px;
}
.empty {
  color: var(--muted);
}
</style>
