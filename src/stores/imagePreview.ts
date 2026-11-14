// 图片预览（v3）：全局单例弹窗，承载「点击缩略图看大图」。
import { defineStore } from "pinia";

/** 图片预览 store。 */
export const useImagePreviewStore = defineStore("imagePreview", {
  state: () => ({
    visible: false,
    src: "",
    alt: "",
  }),
  actions: {
    /** 打开预览；`src` 为 data URL 或可访问的 URL。 */
    open(src: string, alt = "") {
      if (!src) return;
      this.src = src;
      this.alt = alt;
      this.visible = true;
    },
    /** 关闭预览。 */
    close() {
      this.visible = false;
      this.src = "";
      this.alt = "";
    },
  },
});
