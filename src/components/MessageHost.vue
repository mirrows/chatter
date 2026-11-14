<template>
  <Teleport to="body">
    <transition-group
      name="toast"
      tag="div"
      class="message-root"
      :class="{ 'is-mobile': isAndroid }"
      data-testid="message-root"
    >
      <div
        v-for="item in store.visible"
        :key="item.id"
        class="toast"
        :class="item.type"
        :data-testid="`message-${item.type}`"
      >
        {{ item.text }}
      </div>
    </transition-group>
  </Teleport>
</template>

<script setup lang="ts">
// 提示条宿主（v3）：PC 顶部居中、移动端右上贴边；同屏最多 3 条，超出排队。
import { usePlatform } from "../composables/usePlatform";
import { useMessageStore } from "../stores/message";

const store = useMessageStore();
const { isAndroid } = usePlatform();
</script>

<style scoped>
.message-root {
  position: fixed;
  top: max(12px, env(safe-area-inset-top));
  left: 50%;
  transform: translateX(-50%);
  z-index: 1100;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  pointer-events: none;
}
/* 移动端：右上贴边 */
.message-root.is-mobile {
  left: auto;
  right: max(8px, env(safe-area-inset-right));
  transform: none;
  align-items: flex-end;
}
.toast {
  max-width: min(78vw, 420px);
  padding: 8px 14px;
  border-radius: 8px;
  font-size: 13px;
  line-height: 1.4;
  color: #fff;
  background: rgba(31, 41, 55, 0.92);
  box-shadow: 0 8px 20px rgba(0, 0, 0, 0.18);
  word-break: break-word;
}
.toast.success {
  background: #15803d;
}
.toast.error {
  background: #c2410c;
}
/* 动画：下滑淡入、上滑淡出 */
.toast-enter-active,
.toast-leave-active {
  transition:
    opacity 0.18s ease,
    transform 0.18s ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(-8px);
}
</style>
