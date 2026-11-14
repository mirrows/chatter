// 统一错误兜底与上报（v3 / R8.6）：
// - 面向用户的文案不暴露错误码或英文技术细节；
// - 原始信息统一写入本地错误日志（设置页可查看），日志入口只按「目录里是否有日志」显示。
import { t } from "../i18n";
import { useErrorLogStore } from "../stores/errorLog";

/** 判断是否含中文：后端给的用户文案（如「昵称不能为空」）直接透传。 */
const CJK_PATTERN = /[\u4e00-\u9fa5]/;

/** 原始错误 → 字符串（兼容 Error / string / 其它）。 */
export function rawErrorText(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  if (err === null || err === undefined) return "";
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}

/**
 * 面向用户的错误文案：已知的中文提示直接展示，其余统一兜底。
 *
 * 这样既能保留后端已有的友好文案，又不会把 `unknown transfer: xxx`、
 * `permission denied` 这类技术细节直接甩给用户。
 */
export function friendlyError(err: unknown, fallback: string = t("error.unknown")): string {
  const raw = rawErrorText(err).trim();
  if (raw && CJK_PATTERN.test(raw) && raw.length <= 80) return raw;
  return fallback;
}

/** 上报未知错误到本地日志；上报失败不影响主流程。 */
export async function reportUnknownError(scope: string, err: unknown): Promise<void> {
  console.error(`[${scope}]`, err);
  const message = rawErrorText(err) || "unknown error";
  try {
    await useErrorLogStore().report(scope, message);
  } catch {
    // 忽略上报失败：避免错误处理本身再抛错
  }
}

/**
 * store 层统一 catch 处理：上报原始信息，返回可展示的友好文案。
 *
 * 用法：`this.error = handleError("settings.save", err);`
 */
export function handleError(scope: string, err: unknown): string {
  void reportUnknownError(scope, err);
  return friendlyError(err);
}
