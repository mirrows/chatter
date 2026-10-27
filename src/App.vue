<template>
  <main class="app">
    <DeviceList />
  </main>
</template>

<script setup lang="ts">
import DeviceList from "./views/DeviceList.vue";
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
  :root {
    --bg: #111827;
    --card: #1f2937;
    --text: #f3f4f6;
    --muted: #9ca3af;
    --accent: #6ea8ff;
  }
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
