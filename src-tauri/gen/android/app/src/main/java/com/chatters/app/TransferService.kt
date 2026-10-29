package com.chatters.app

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat

/**
 * 传输期间的前台服务（M9）。
 *
 * Android 会在应用切后台/锁屏后限制后台进程的网络与 CPU，长传输会中断。
 * 传输开始时常驻一条低优先级通知即可让系统把进程视为前台，从而保持 TCP 连接。
 * 服务本身不做传输，只负责“保活 + 展示状态”。
 */
class TransferService : Service() {
  companion object {
    private const val CHANNEL_ID = "chatters-transfer"
    private const val NOTIFICATION_ID = 1001
    private const val EXTRA_TITLE = "title"
    private const val EXTRA_BODY = "body"

    /** 启动（或更新）前台服务。 */
    fun start(context: Context, title: String, body: String) {
      val intent = Intent(context, TransferService::class.java).apply {
        putExtra(EXTRA_TITLE, title)
        putExtra(EXTRA_BODY, body)
      }
      ContextCompat.startForegroundService(context, intent)
    }

    /** 停止前台服务。 */
    fun stop(context: Context) {
      context.stopService(Intent(context, TransferService::class.java))
    }
  }

  override fun onBind(intent: Intent?): IBinder? = null

  override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
    val title = intent?.getStringExtra(EXTRA_TITLE) ?: "Chatters"
    val body = intent?.getStringExtra(EXTRA_BODY) ?: ""
    ensureChannel()

    val notification = NotificationCompat.Builder(this, CHANNEL_ID)
      .setSmallIcon(android.R.drawable.stat_sys_download)
      .setContentTitle(title)
      .setContentText(body)
      .setOngoing(true)
      .setPriority(NotificationCompat.PRIORITY_LOW)
      .build()

    startForeground(NOTIFICATION_ID, notification)
    // 传输由 Rust 侧驱动，服务被系统回收后不需要重建。
    return START_NOT_STICKY
  }

  override fun onDestroy() {
    // 主动摘掉常驻通知，避免用户看到“已完成却仍在通知栏”的残留。
    stopForeground(STOP_FOREGROUND_REMOVE)
    super.onDestroy()
  }

  private fun ensureChannel() {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
      val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
      if (manager.getNotificationChannel(CHANNEL_ID) == null) {
        manager.createNotificationChannel(
          NotificationChannel(CHANNEL_ID, "文件传输", NotificationManager.IMPORTANCE_LOW)
        )
      }
    }
  }
}
