// WebView 右键菜单禁用（v2/M9）。
//
// WebView2 的默认上下文菜单会暴露「检查元素 / 另存为 / 重新加载」等入口，
// 发布版不需要这些能力；Android 的长按菜单（复制/粘贴）不受影响，故仅 Windows 调用。

/** 拦截右键菜单；返回解绑函数（便于组件卸载时恢复）。 */
export function blockContextMenu(target: Document | HTMLElement = document): () => void {
  const handler = (event: Event): void => {
    event.preventDefault();
  };
  target.addEventListener("contextmenu", handler);
  return () => target.removeEventListener("contextmenu", handler);
}
