// 剪贴板图片工具（F21）：从 paste 事件中提取图片并转 base64。
export function imageFileFromItems(items: DataTransferItemList | null | undefined): File | null {
  if (!items) return null;
  for (const item of Array.from(items)) {
    if (item.kind === "file" && item.type.startsWith("image/")) {
      const file = item.getAsFile();
      if (file) return file;
    }
  }
  return null;
}

/** 读取图片为 base64 数据（去掉 data URL 前缀）与扩展名。 */
export function readFileAsBase64(file: File): Promise<{ data: string; extension: string }> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("读取剪贴板图片失败"));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      const comma = result.indexOf(",");
      if (comma < 0) {
        reject(new Error("读取剪贴板图片失败"));
        return;
      }
      const header = result.slice(0, comma);
      const data = result.slice(comma + 1);
      const matched = /^data:image\/([a-zA-Z0-9+.-]+);base64$/.exec(header);
      let extension = matched ? matched[1].toLowerCase() : "png";
      if (extension === "jpg") extension = "jpeg";
      resolve({ data, extension });
    };
    reader.readAsDataURL(file);
  });
}
