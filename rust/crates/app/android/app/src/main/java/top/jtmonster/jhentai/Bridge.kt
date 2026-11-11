/*
 * JNI 桥：把平台能力暴露给 Rust。所有方法都在 Java 主线程被调用（Rust 侧负责切线程）。
 */
package top.jtmonster.jhentai

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings

object Bridge {
    private lateinit var appContext: Context

    fun init(context: Context) {
        appContext = context.applicationContext
    }

    @JvmStatic
    fun externalFilesDir(): String = appContext.getExternalFilesDir(null)!!.absolutePath

    // shortcut: 屏幕常亮/防截屏/亮度由 Rust 侧通过 android-activity 的 set_window_flags
    // 与 WindowManager.LayoutParams 直接做，这里只在需要 Java 侧 Activity 引用时才实现。
    @JvmStatic
    fun keepScreenOn(on: Boolean) = Unit

    @JvmStatic
    fun setSecureFlag(on: Boolean) = Unit

    @JvmStatic
    fun setBrightness(value: Float) = Unit

    @JvmStatic
    fun batteryLevel(): Int = -1

    @JvmStatic
    fun setClipboard(text: String) {
        val manager = appContext.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        manager.setPrimaryClip(ClipData.newPlainText("jhentai", text))
    }

    @JvmStatic
    fun getClipboard(): String {
        val manager = appContext.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
        return manager.primaryClip?.getItemAt(0)?.coerceToText(appContext)?.toString() ?: ""
    }

    @JvmStatic
    fun openUrl(url: String) {
        val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        appContext.startActivity(intent)
    }

    /** Android 11+ 需要 MANAGE_EXTERNAL_STORAGE 才能读下载目录之外的文件 */
    @JvmStatic
    fun hasAllFilesAccess(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) android.os.Environment.isExternalStorageManager() else true

    @JvmStatic
    fun requestAllFilesAccess() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val intent = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION)
            intent.data = Uri.parse("package:" + appContext.packageName)
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            appContext.startActivity(intent)
        }
    }

    @JvmStatic
    fun appVersion(): String = BuildConfig.VERSION_NAME

    @JvmStatic
    fun deviceInfo(): String = "${Build.MANUFACTURER} ${Build.MODEL} / Android ${Build.VERSION.RELEASE}"

    /** 系统代理：`adb shell settings put global http_proxy host:port` 写入的那一项 */
    @JvmStatic
    fun systemProxy(): String {
        val proxy = Settings.Global.getString(appContext.contentResolver, "http_proxy") ?: ""
        val exclusion = Settings.Global.getString(appContext.contentResolver, "http_proxy_exclusion_list") ?: ""
        if (proxy.isBlank() || exclusion.contains("e-hentai.org")) return ""
        return proxy
    }

    /** API 29+ 走 MediaStore；更低版本写入 Pictures 目录后通知媒体扫描 */
    @JvmStatic
    fun saveImage(path: String, mime: String): Boolean {
        return try {
            val file = java.io.File(path)
            if (!file.exists()) return false
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                val values = android.content.ContentValues().apply {
                    put(android.provider.MediaStore.Images.Media.DISPLAY_NAME, file.name)
                    put(android.provider.MediaStore.Images.Media.MIME_TYPE, mime)
                    put(android.provider.MediaStore.Images.Media.RELATIVE_PATH, "Pictures/JHenTai")
                }
                val uri = appContext.contentResolver.insert(
                    android.provider.MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values
                ) ?: return false
                appContext.contentResolver.openOutputStream(uri)?.use { out ->
                    file.inputStream().use { it.copyTo(out) }
                } ?: return false
            } else {
                val dir = java.io.File(
                    android.os.Environment.getExternalStoragePublicDirectory(android.os.Environment.DIRECTORY_PICTURES),
                    "JHenTai"
                )
                dir.mkdirs()
                val target = java.io.File(dir, file.name)
                file.copyTo(target, overwrite = true)
                android.media.MediaScannerConnection.scanFile(
                    appContext, arrayOf(target.absolutePath), arrayOf(mime), null
                )
            }
            true
        } catch (e: Exception) {
            false
        }
    }

    @JvmStatic
    fun shareFile(path: String, mime: String) {
        try {
            val authority = appContext.packageName + ".fileprovider"
            val uri = androidx.core.content.FileProvider.getUriForFile(appContext, authority, java.io.File(path))
            val intent = Intent(Intent.ACTION_SEND).apply {
                type = mime
                putExtra(Intent.EXTRA_STREAM, uri)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            val chooser = Intent.createChooser(intent, null).apply { addFlags(Intent.FLAG_ACTIVITY_NEW_TASK) }
            appContext.startActivity(chooser)
        } catch (e: Exception) {
            // 没有 FileProvider 或无可分享应用时静默失败，不崩溃
        }
    }
}
