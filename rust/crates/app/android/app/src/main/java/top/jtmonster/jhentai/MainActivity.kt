/*
 * 薄壳：只做 JNI 装载与 Activity 生命周期转发，不放业务逻辑（计划 §0）。
 * Slint 的 android-activity 后端要求 Activity 继承 NativeActivity。
 */
package top.jtmonster.jhentai

import android.app.NativeActivity
import android.os.Bundle
import android.view.KeyEvent
import android.view.View

class MainActivity : NativeActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Bridge.init(this)
    }

    override fun onResume() {
        super.onResume()
        // 沉浸式：状态栏/导航栏透明（对应 Flutter 版的 SystemUiOverlayStyle）
        window.decorView.systemUiVisibility = (
            View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                or View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
            )
    }

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        // 音量键翻页：Rust 只在阅读页且设置开启时返回 true
        if (event.keyCode == KeyEvent.KEYCODE_VOLUME_UP || event.keyCode == KeyEvent.KEYCODE_VOLUME_DOWN) {
            if (event.action == KeyEvent.ACTION_DOWN && nativeOnVolumeKey(event.keyCode == KeyEvent.KEYCODE_VOLUME_UP)) {
                return true
            }
        }
        // 返回键先给 Rust 的页栈；栈底时交回系统（退出应用）
        if (event.keyCode == KeyEvent.KEYCODE_BACK) {
            if (event.action == KeyEvent.ACTION_UP && nativeOnBack()) return true
            // 按下事件也吞掉，避免漏给原生后端
            // 按下事件一律吞掉，避免漏给原生后端
            if (event.action == KeyEvent.ACTION_DOWN) return true
        }
        return super.dispatchKeyEvent(event)
    }

    companion object {
        init {
            System.loadLibrary("jhentai")
        }
        /** 返回 true 表示 Rust 侧已出栈，按键被消费 */
        @JvmStatic
        external fun nativeOnBack(): Boolean

        /** 返回 true 表示 Rust 侧已消费该音量键（阅读页翻页） */
        @JvmStatic
        external fun nativeOnVolumeKey(up: Boolean): Boolean


        // shortcut: 音量键翻页在 M4 阅读器落地时再接（dispatchKeyEvent + JNI 回调）
    }
}
