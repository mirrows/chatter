// 文件名展示工具（v3）：方形气泡底部单行显示，中间省略但保留后缀。

/**
 * 从中间省略文件名，保留扩展名。
 *
 * 例：`2026 年度总结报告最终版.pdf`（max=12）→ `2026 年…终版.pdf`
 */
export function middleEllipsis(name: string, max = 16): string {
  const trimmed = name.trim();
  if (!trimmed) return "";
  if (trimmed.length <= max) return trimmed;

  const dot = trimmed.lastIndexOf(".");
  // 后缀过长（如 `.tar.gz.bak`）时不视为扩展名，按纯文本截断。
  const ext = dot > 0 && trimmed.length - dot <= 6 ? trimmed.slice(dot) : "";
  const base = ext ? trimmed.slice(0, dot) : trimmed;

  // 连「省略号 + 后缀」都放不下时，退化为尾部保留（最少保证能看出文件类型）。
  if (max <= ext.length + 2) {
    return `${trimmed.slice(0, Math.max(1, max - ext.length - 1))}…${ext}`;
  }

  const budget = max - ext.length - 1; // 1 个字符留给省略号
  const head = Math.ceil(budget / 2);
  const tail = budget - head;
  return `${base.slice(0, head)}…${base.slice(base.length - tail)}${ext}`;
}
