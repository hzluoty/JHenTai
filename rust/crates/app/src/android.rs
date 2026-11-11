//! Android 入口：由 Kotlin 壳的 NativeActivity 加载 `libjhentai.so` 后调用 `android_main`。

use std::rc::Rc;

use jhentai_platform::Platform;

/// Kotlin 壳的返回键回调：能出栈时消费按键，把出栈请求交给 UI 线程。
#[no_mangle]
pub extern "system" fn Java_top_jtmonster_jhentai_MainActivity_nativeOnBack(
    _env: jni::JNIEnv,
    _class: jni::objects::JClass,
) -> jni::sys::jboolean {
    if crate::nav::NAV_DEPTH.load(std::sync::atomic::Ordering::Relaxed) > 1 {
        crate::nav::BACK_REQUESTED.store(true, std::sync::atomic::Ordering::Relaxed);
        1
    } else {
        0
    }
}

/// Slint 的 Android 后端入口（`android-activity` crate 约定）
#[no_mangle]
pub fn android_main(app: slint::android::AndroidApp) {
    if let Err(e) = run(app) {
        // 没有 logcat 桥时至少能在 RustStdoutStderr 里看到
        eprintln!("jhentai: {e}");
    }
}

fn run(app: slint::android::AndroidApp) -> Result<(), Box<dyn std::error::Error>> {
    // 缓存 JavaVM 供 platform 层回调 Bridge 使用（ndk_context 也由 android-activity 初始化）
    unsafe {
        let raw = app.vm_as_ptr();
        if !raw.is_null() {
            if let Ok(vm) = jni::JavaVM::from_raw(raw.cast()) {
                jhentai_platform::android::init(vm);
            }
        }
    }

    slint::android::init(app).map_err(|e| format!("slint android init: {e}"))?;

    let platform = std::sync::Arc::new(jhentai_platform::android::AndroidPlatform);
    let mut options = jhentai_core::net::ClientOptions::default();
    options.enable_domain_fronting = true;
    // 系统代理由 ConnectivityManager/settings 读，reqwest 不自动读
    options.proxy = jhentai_core::net::normalize_proxy(platform.system_proxy());

    let data_dir = platform.external_data_path();
    let controller = Rc::new(crate::AppController::new(options, Some(data_dir), platform.clone())?);
    controller.wire_callbacks();
    controller.load_settings();
    controller.restore_account();
    if controller.is_locked() {
        controller.window().set_locked(true);
    }
    controller.load_first_page();
    controller.run()?;
    Ok(())
}
