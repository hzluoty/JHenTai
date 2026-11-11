//! Android 入口：由 Kotlin 壳的 NativeActivity 加载 `libjhentai.so` 后调用 `android_main`。

use std::rc::Rc;

use jhentai_platform::Platform;

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

    let platform = jhentai_platform::android::AndroidPlatform;
    let mut options = jhentai_core::net::ClientOptions::default();
    options.enable_domain_fronting = true;
    // 系统代理由 ConnectivityManager/settings 读，reqwest 不自动读
    options.proxy = platform
        .system_proxy()
        .map(|proxy| if proxy.starts_with("http") { proxy } else { format!("http://{proxy}") });

    let controller = Rc::new(crate::AppController::new(options)?);
    controller.wire_callbacks();
    controller.load_page(0);
    controller.run()?;
    Ok(())
}
