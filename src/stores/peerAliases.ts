// 设备别名（v3）：用户自定义的设备展示名，缓存到 localStorage。
import { defineStore } from "pinia";

/** 本地缓存 key。 */
const STORAGE_KEY = "chatters.peerAliases";

/** 读取缓存；内容损坏或类型不符时忽略（不阻塞启动）。 */
function loadAliases(): Record<string, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object") return {};
    const result: Record<string, string> = {};
    for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
      if (typeof value === "string" && value.trim()) result[key] = value.trim();
    }
    return result;
  } catch {
    return {};
  }
}

/** 设备别名 store。 */
export const usePeerAliasStore = defineStore("peerAliases", {
  state: () => ({
    /** device_id → 自定义名称。 */
    aliases: loadAliases(),
  }),
  actions: {
    /** 设备展示名：优先自定义，其次对端昵称。 */
    nameFor(deviceId: string, fallback: string): string {
      return this.aliases[deviceId] ?? fallback;
    },
    /** 保存别名；传入空名表示恢复默认（删除自定义）。 */
    setAlias(deviceId: string, name: string) {
      const trimmed = name.trim();
      if (trimmed) this.aliases[deviceId] = trimmed;
      else delete this.aliases[deviceId];
      this.persist();
    },
    /** 写入本地缓存（失败只影响持久化，不影响本次会话）。 */
    persist() {
      try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(this.aliases));
      } catch {
        // localStorage 不可用（隐私模式/配额）时静默降级。
      }
    },
    /** 清空全部别名（设置页「恢复默认」可用）。 */
    clear() {
      this.aliases = {};
      this.persist();
    },
  },
});
