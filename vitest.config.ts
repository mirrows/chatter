import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";

// Vitest 独立配置（与 vite.config.ts 分离，避免类型冲突）。
export default defineConfig({
  plugins: [vue()],
  test: {
    environment: "jsdom",
    include: ["tests/**/*.spec.ts"],
  },
});
