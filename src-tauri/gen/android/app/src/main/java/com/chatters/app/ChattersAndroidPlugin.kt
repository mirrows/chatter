package com.chatters.app

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.net.wifi.WifiManager
import android.webkit.MimeTypeMap
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

/** SAF 导出参数：源文件绝对路径 + 建议文件名。 */
@InvokeArg
class SaveFileArgs {
  lateinit var path: String
  var fileName: String? = null
}

/** 打开文件参数：源文件绝对路径。 */
@InvokeArg
class OpenFileArgs {
  lateinit var path: String
}

/** 前台服务参数：通知标题与正文。 */
@InvokeArg
class ForegroundArgs {
  lateinit var title: String
  lateinit var body: String
}

/**
 * Chatters 的 Android 原生能力插件（M9）。
 *
 * Rust 侧通过 `tauri::plugin::Builder` 的 `register_android_plugin` 注册本类，
 * 并用 `run_mobile_plugin("<command>", payload)` 调用下列 @Command 方法。
 */
@TauriPlugin
class ChattersAndroidPlugin(private val activity: Activity) : Plugin(activity) {
  private var multicastLock: WifiManager.MulticastLock? = null
  private var pendingSavePath: String? = null

  /** 自检：确认插件已正确注册。 */
  @Command
  fun ping(invoke: Invoke) {
    val result = JSObject()
    result.put("value", "pong")
    invoke.resolve(result)
  }

  /**
   * 持有 Wi-Fi 组播锁。
   *
   * Android 的 Wi-Fi 省电策略会过滤未加入组播组的广播/组播包，
   * 不持有该锁时收不到桌面端发出的 255.255.255.255 发现包。
   */
  @Command
  fun acquireMulticastLock(invoke: Invoke) {
    try {
      if (multicastLock == null) {
        val wifi = activity.applicationContext
          .getSystemService(Context.WIFI_SERVICE) as WifiManager
        val lock = wifi.createMulticastLock("chatters-discovery")
        lock.setReferenceCounted(false)
        lock.acquire()
        multicastLock = lock
      }
      invoke.resolve(JSObject())
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to acquire multicast lock")
    }
  }

  /** 释放组播锁（应用退出或停止发现时）。 */
  @Command
  fun releaseMulticastLock(invoke: Invoke) {
    try {
      multicastLock?.let {
        if (it.isHeld) it.release()
      }
      multicastLock = null
      invoke.resolve(JSObject())
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to release multicast lock")
    }
  }

  /** 用系统查看器打开私有目录里的文件（FileProvider 授权临时读权限）。 */
  @Command
  fun openFile(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(OpenFileArgs::class.java)
      val file = File(args.path)
      if (!file.exists()) {
        invoke.reject("file not found")
        return
      }
      val intent = Intent(Intent.ACTION_VIEW).apply {
        setDataAndType(fileUri(file), mimeTypeFor(file.name))
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      activity.startActivity(intent)
      invoke.resolve(JSObject())
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to open file")
    }
  }

  /** SAF 导出：选择目标位置后把私有目录里的文件复制过去。 */
  @Command
  fun saveFile(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SaveFileArgs::class.java)
      val file = File(args.path)
      if (!file.exists()) {
        invoke.reject("file not found")
        return
      }
      pendingSavePath = args.path
      val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
        addCategory(Intent.CATEGORY_OPENABLE)
        type = mimeTypeFor(file.name)
        putExtra(Intent.EXTRA_TITLE, args.fileName ?: file.name)
      }
      startActivityForResult(invoke, intent, "saveFileResult")
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to start save dialog")
    }
  }

  /** SAF 结果回调：读取用户选择的 Uri 并复制内容。 */
  @ActivityCallback
  fun saveFileResult(invoke: Invoke, result: ActivityResult) {
    val source = pendingSavePath
    pendingSavePath = null
    if (result.resultCode != Activity.RESULT_OK || result.data?.data == null) {
      invoke.reject("cancelled")
      return
    }
    try {
      if (source == null) {
        invoke.reject("missing source path")
        return
      }
      val uri: Uri = result.data!!.data!!
      activity.contentResolver.openOutputStream(uri)?.use { output ->
        File(source).inputStream().use { input -> input.copyTo(output) }
      } ?: throw IllegalStateException("cannot open output stream")
      val ret = JSObject()
      ret.put("path", uri.toString())
      invoke.resolve(ret)
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to write file")
    }
  }

  /** 传输期间启动前台服务（常驻通知）。 */
  @Command
  fun startForegroundService(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ForegroundArgs::class.java)
      TransferService.start(activity.applicationContext, args.title, args.body)
      invoke.resolve(JSObject())
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to start foreground service")
    }
  }

  /** 停止前台服务。 */
  @Command
  fun stopForegroundService(invoke: Invoke) {
    try {
      TransferService.stop(activity.applicationContext)
      invoke.resolve(JSObject())
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to stop foreground service")
    }
  }

  /** 私有目录不在 file_paths 覆盖范围时，先复制到缓存目录再共享。 */
  private fun fileUri(file: File): Uri =
    try {
      FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
    } catch (e: IllegalArgumentException) {
      val cached = File(activity.cacheDir, file.name)
      file.copyTo(cached, overwrite = true)
      FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", cached)
    }

  private fun mimeTypeFor(name: String): String {
    val ext = name.substringAfterLast('.', "").lowercase()
    return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
  }
}
