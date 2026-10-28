// 极简 i18n（F53 / rules.md R3.3）：首版仅中文，结构上预留多语言目录。
// 组件内不得硬编码用户可见文案，必须通过 t() 取词条。

/** 支持的语言；后续新增语言在此扩展。 */
export type Locale = "zh-CN";

/** 中文词条表。 */
const zhCN = {
  "app.title": "Chatters",
  "device.title": "局域网设备",
  "device.count": "在线 {count} 台",
  "device.empty": "正在扫描局域网… 未发现其他设备",
  "device.local": "本机：{name}",
  "device.open": "开始聊天",
  "device.addByIp": "添加设备",
  "device.addPlaceholder": "输入对方 IP，如 192.168.1.10:28101",
  "device.notice.up": "{name} 已上线",
  "device.notice.down": "{name} 已下线",
  "chat.back": "返回设备列表",
  "chat.title": "与 {name} 的会话",
  "chat.empty": "还没有消息，说点什么吧",
  "chat.placeholder": "输入消息，Ctrl+Enter 发送",
  "chat.send": "发送",
  "chat.retry": "重发",
  "chat.connected": "已连接",
  "chat.disconnected": "未连接（发送时自动连接）",
  "chat.error": "发送失败：{reason}",
  "chat.status.sending": "发送中",
  "chat.status.delivered": "已送达",
  "chat.status.failed": "发送失败",
  "chat.attachImage": "发送图片",
  "chat.attachFile": "发送文件",
  "transfer.state.pending": "等待确认",
  "transfer.state.offered": "等待对方确认",
  "transfer.state.transferring": "传输中",
  "transfer.state.done": "已完成",
  "transfer.state.failed": "传输失败",
  "transfer.state.cancelled": "已取消",
  "transfer.state.rejected": "已拒绝",
  "transfer.accept": "接受",
  "transfer.reject": "拒绝",
  "transfer.cancel": "取消",
  "transfer.retry": "重试",
  "transfer.speed": "{speed}",
  "transfer.remaining": "剩余 {time}",
  "transfer.direction.send": "发送",
  "transfer.direction.receive": "接收",
  "transfer.open": "打开文件",
  "transfer.reveal": "打开所在文件夹",
  "transfer.saveAs": "另存为",
  "transfer.progress": "{percent}%",
  "transfer.previewUnavailable": "图片预览不可用",
  "nav.devices": "设备",
  "nav.settings": "设置",
  "chat.copy": "复制",
  "chat.copied": "已复制",
  "chat.pasteFailed": "粘贴图片失败：{reason}",
  "chat.folderNotSupported": "请将文件夹压缩为 zip 后发送（已忽略 {count} 个文件夹）",
  "chat.dropHint": "可直接把文件或图片拖到窗口发送",
  "chat.clearHistory": "清空本会话记录",
  "chat.cleared": "本会话记录已清空",
  "settings.title": "设置",
  "settings.nickname": "设备昵称",
  "settings.deviceId": "设备 ID",
  "settings.downloadDir": "下载目录",
  "settings.discoveryPort": "发现端口（UDP）",
  "settings.tcpPort": "通信端口（TCP）",
  "settings.theme": "主题",
  "settings.theme.light": "浅色",
  "settings.theme.dark": "深色",
  "settings.autoImages": "自动接收图片",
  "settings.autoFiles": "自动接收文件",
  "settings.autostart": "开机自启",
  "settings.save": "保存设置",
  "settings.saved": "设置已保存",
  "settings.restart": "昵称或端口已修改，重启应用后生效",
  "settings.clearHistory": "清空全部历史",
  "settings.deleteFiles": "清空历史时同时删除已接收文件",
  "settings.historyCleared": "历史已清空",
  "settings.historyClearedWithFiles": "历史已清空，并删除 {count} 个文件",
  "settings.portsHint": "端口修改需重启应用；两端发现端口需一致",
} as const;

/** 文案 key（由中文词条表推导，写错 key 会在编译期报错）。 */
export type MessageKey = keyof typeof zhCN;

/** 语言 → 词条表。 */
const catalogs: Record<Locale, Record<MessageKey, string>> = {
  "zh-CN": zhCN,
};

let currentLocale: Locale = "zh-CN";

/** 设置当前语言（设置页在 M4 提供入口）。 */
export function setLocale(locale: Locale): void {
  currentLocale = locale;
}

/** 读取当前语言。 */
export function getLocale(): Locale {
  return currentLocale;
}

/**
 * 取一条文案，支持 `{name}` 形式的占位符。
 *
 * 运行时遇到未知 key 或缺失参数时保留原样（返回 key 或占位符本身），
 * 保证界面不会因为漏词条而空白。
 */
export function t(key: MessageKey, params?: Record<string, string | number>): string {
  const table = catalogs[currentLocale] as Record<string, string>;
  const template = table[key] ?? key;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (placeholder, name: string) => {
    const value = params[name];
    return value === undefined ? placeholder : String(value);
  });
}
