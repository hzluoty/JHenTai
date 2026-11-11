/*
 * 薄壳：只做 JNI 装载与 Activity 生命周期转发，不放业务逻辑（计划 §0）。
 * Slint 的 android-activity 后端要求 Activity 继承 NativeActivity。
 */
package top.jtmonster.jhentai

import android.app.NativeActivity
import android.os.Bundle
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

    companion object {
        init {
            System.loadLibrary("jhentai")
        }
        // shortcut: 音量键翻页在 M4 阅读器落地时再接（dispatchKeyEvent + JNI 回调），
        // 现在不声明无实现的 external 函数，避免按键触发 UnsatisfiedLinkError。
    }
}
