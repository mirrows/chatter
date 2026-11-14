package com.chatters.app

import app.tauri.plugin.JSObject

/**
 * 系统分享待处理内容（v3，Android ACTION_SEND）。
 *
 * MainActivity 收到分享时写入，Rust 通过插件命令 `takeShare` 取走并清空；
 * 只保留最近一次分享（够用且避免内存泄漏）。
 */
object ShareInbox {
  @Volatile
  private var pending: JSObject? = null

  /** 记录一次分享（覆盖上一次未处理的分享）。 */
  @Synchronized
  fun set(payload: JSObject) {
    pending = payload
  }

  /** 取走待处理分享；无则返回 null。 */
  @Synchronized
  fun take(): JSObject? {
    val current = pending
    pending = null
    return current
  }
}
