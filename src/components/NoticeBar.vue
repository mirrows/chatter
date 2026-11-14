<template>
  <!-- 通用提示条（v3）：绝对定位在父容器顶部，下滑出现、可手动关闭。 -->
  <transition name="slide-down">
    <div v-if="text" class="notice-layer" :data-testid="testid">
      <span class="notice-text">{{ text }}</span>
      <button
        v-if="closable"
        type="button"
        class="notice-close"
        data-testid="notice-close"
        @click="emit('close')"
      >
        ×
      </button>
    </div>
  </transition>
</template>

<script setup lang="ts">
// 对话区提示浮层抽成的通用组件（v3）：对话页的错误/传输提示、以及后续其它页面复用。
// 注意：布尔 prop 默认值必须用 withDefaults 声明，否则缺省时会被 Vue 当作 false。
withDefaults(
  defineProps<{
    /** 提示文案；空串表示不显示。 */
    text: string;
    /** 供测试定位用的 data-testid。 */
    testid?: string;
    /** 是否显示关闭按钮（默认显示）。 */
    closable?: boolean;
  }>(),
  { testid: undefined, closable: true },
);

const emit = defineEmits<{ close: [] }>();
</script>

<style scoped>
/* 浮层：贴在父容器顶部居中，下滑出现（v3）。 */
.notice-layer {
  position: absolute;
  top: 8px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: calc(100% - 24px);
  padding: 6px 10px 6px 12px;
  border-radius: 8px;
  background: rgba(15, 23, 42, 0.9);
  color: #fff;
  font-size: 13px;
  box-shadow: 0 6px 18px rgba(0, 0, 0, 0.18);
}
.notice-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.notice-close {
  flex: 0 0 auto;
  padding: 0 2px;
  border: none;
  background: transparent;
  color: #fff;
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  opacity: 0.7;
}
.notice-close:hover {
  opacity: 1;
}
.slide-down-enter-active,
.slide-down-leave-active {
  transition:
    opacity 0.18s ease,
    transform 0.18s ease;
}
.slide-down-enter-from,
.slide-down-leave-to {
  opacity: 0;
  transform: translate(-50%, -8px);
}
</style>
