//! 桌面入口（开发用）。Android 走 `android_main`（见 src/android.rs）。

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use jhentai_platform::Platform;
    let platform = std::sync::Arc::new(jhentai_platform::host::HostPlatform);
    let mut options = jhentai_core::net::ClientOptions::default();
    if let Ok(proxy) = std::env::var("HTTPS_PROXY").or_else(|_| std::env::var("HTTP_PROXY")) {
        options.proxy = Some(proxy);
    }
    options.enable_domain_fronting = false;

    let data_dir = platform.external_data_path();
    let controller = std::rc::Rc::new(jhentai::AppController::new(options, Some(data_dir), platform.clone())?);
    controller.wire_callbacks();
    controller.load_settings();
    controller.load_search_history();
    controller.load_block_rules();
    controller.load_tag_translation();
    controller.restore_account();
    if controller.is_locked() {
        controller.window().set_locked(true);
    }
    controller.window().set_current_tab(0);
    controller.load_first_page();

    eprintln!("JHenTai {} ({})", platform.app_version(), platform.device_info());
    controller.run()?;
    Ok(())
}
