package com.chatters.app

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // 说明：ChattersAndroidPlugin 由 Rust 侧 register_android_plugin 实例化并注册，
    // 这里不需要（也没有）registerPlugin 调用。
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    applySystemBarInsets()
    requestNotificationPermissionIfNeeded()
  }

  /**
   * 适配不同机型的顶部/底部安全区与软键盘。
   *
   * targetSdk 35+ 在 Android 15 上强制 edge-to-edge，页面会画到状态栏/导航栏下面；
   * 这里把系统栏与输入法（IME）的高度统一转成内容视图的 padding，
   * 页面即可用满剩余空间，键盘弹出时输入框也不会被遮挡。
   */
  private fun applySystemBarInsets() {
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val ime = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
      insets
    }
  }

  /** Android 13+ 需要运行时申请通知权限，否则传输完成/收到文件的提醒不会显示。 */
  private fun requestNotificationPermissionIfNeeded() {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
    val granted = ContextCompat.checkSelfPermission(
      this,
      Manifest.permission.POST_NOTIFICATIONS
    ) == PackageManager.PERMISSION_GRANTED
    if (!granted) {
      ActivityCompat.requestPermissions(
        this,
        arrayOf(Manifest.permission.POST_NOTIFICATIONS),
        NOTIFICATION_PERMISSION_REQUEST
      )
    }
  }

  companion object {
    private const val NOTIFICATION_PERMISSION_REQUEST = 1001
  }
}
