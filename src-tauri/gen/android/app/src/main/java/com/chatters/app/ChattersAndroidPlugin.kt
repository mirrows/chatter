package com.chatters.app

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.net.wifi.WifiManager
import android.provider.OpenableColumns
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

/** 文件复制缓冲：默认 8KB 对大文件太慢，改为 1MB（v3）。 */
private const val COPY_BUFFER = 1 shl 20

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

/** SAF 导入参数：文件选择器返回的 `content://` Uri。 */
@InvokeArg
class ImportFileArgs {
  lateinit var uri: String
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

  /**
   * 取走系统分享内容（v3，Android ACTION_SEND）。
   *
   * 无分享时返回空对象；有分享时返回 `{ kind: "text", text }` 或
   * `{ kind: "file", uri, name, mime }`。
   */
  @Command
  fun takeShare(invoke: Invoke) {
    val pending = ShareInbox.take()
    invoke.resolve(pending ?: JSObject())
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
    if (source == null) {
      invoke.reject("missing source path")
      return
    }
    val uri: Uri = result.data!!.data!!
    // 复制可能耗时较长（大文件），放到后台线程，避免卡住界面（v3）。
    copyInBackground(invoke) {
      activity.contentResolver.openOutputStream(uri)?.use { output ->
        File(source).inputStream().use { input -> input.copyTo(output, COPY_BUFFER) }
      } ?: throw IllegalStateException("cannot open output stream")
      uri.toString()
    }
  }

  /**
   * 把 SAF 选中的 Uri 复制到缓存目录，返回本地绝对路径。
   *
   * Android 的文件选择器只返回 `content://` Uri，Rust 侧无法用 std::fs 读取，
   * 因此发送图片/文件前先由 Kotlin 复制到私有缓存。
   *
   * v3 优化：复制改到后台线程 + 1MB 缓冲。旧实现用默认 8KB 缓冲并在命令线程同步复制，
   * 几百 MB 的文件会明显卡顿、且调用方迟迟拿不到结果。
   */
  @Command
  fun importFile(invoke: Invoke) {
    val uri = try {
      Uri.parse(invoke.parseArgs(ImportFileArgs::class.java).uri)
    } catch (e: Exception) {
      invoke.reject(e.message ?: "failed to parse uri")
      return
    }
    copyInBackground(invoke) {
      val dir = File(activity.cacheDir, "imports").apply { mkdirs() }
      val name = displayName(uri) ?: "import-${System.currentTimeMillis()}"
      val target = uniqueFile(dir, name)
      activity.contentResolver.openInputStream(uri)?.use { input ->
        target.outputStream().use { output -> input.copyTo(output, COPY_BUFFER) }
      } ?: throw IllegalStateException("cannot open input stream")
      target.absolutePath
    }
  }

  /**
   * 在后台线程执行文件复制，完成后回主线程回传结果。
   *
   * @param block 返回需要回传给 Rust 的本地路径。
   */
  private fun copyInBackground(invoke: Invoke, block: () -> String) {
    Thread {
      try {
        val path = block()
        activity.runOnUiThread {
          val ret = JSObject()
          ret.put("path", path)
          invoke.resolve(ret)
        }
      } catch (e: Exception) {
        val message = e.message ?: "copy failed"
        activity.runOnUiThread { invoke.reject(message) }
      }
    }.start()
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

  /** 读取 SAF 文件的显示名（保留扩展名，供图片/文件类型识别）。 */
  private fun displayName(uri: Uri): String? {
    val columns = arrayOf(OpenableColumns.DISPLAY_NAME)
    activity.contentResolver.query(uri, columns, null, null, null)?.use { cursor ->
      if (cursor.moveToFirst()) {
        val index = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME)
        if (index >= 0 && !cursor.isNull(index)) return cursor.getString(index)
      }
    }
    return uri.lastPathSegment?.substringAfterLast('/')
  }

  /** 缓存目录内避免重名覆盖（同名文件追加序号）。 */
  private fun uniqueFile(dir: File, name: String): File {
    var candidate = File(dir, name)
    if (!candidate.exists()) return candidate
    val base = name.substringBeforeLast('.', name)
    val ext = name.substringAfterLast('.', "")
    var index = 1
    while (candidate.exists()) {
      candidate = File(dir, if (ext.isEmpty()) "$base ($index)" else "$base ($index).$ext")
      index += 1
    }
    return candidate
  }

  /** 私有目录不在 file_paths 覆盖范围时，先复制到缓存目录再共享。 */
  private fun fileUri(file: File): Uri =
    try {
      FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
    } catch (e: IllegalArgumentException) {
      val cached = File(activity.cacheDir, file.name)
      file.copyTo(cached, overwrite = true, bufferSize = COPY_BUFFER)
      FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", cached)
    }

  private fun mimeTypeFor(name: String): String {
    val ext = name.substringAfterLast('.', "").lowercase()
    return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
  }
}
