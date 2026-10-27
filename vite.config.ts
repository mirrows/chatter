import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri 前端构建配置：开发端口 5173（tauri.conf.json 中 devUrl 指向此处）。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
  build: {
    outDir: "dist",
    target: "es2021",
  },
});
