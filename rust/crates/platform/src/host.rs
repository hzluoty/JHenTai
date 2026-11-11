//! host（Windows/Linux）上的实现，用于桌面预览与测试。

use std::path::PathBuf;

use crate::{NullPlatform, Platform};

#[derive(Debug, Default, Clone, Copy)]
pub struct HostPlatform;

impl Platform for HostPlatform {
    fn external_data_path(&self) -> PathBuf {
        let base = std::env::var_os("JHENTAI_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs_data_dir().join("JHenTaiRust"));
        let _ = std::fs::create_dir_all(&base);
        base
    }

    fn open_url(&self, url: &str) {
        // host 上直接打印，桌面调试够用
        eprintln!("open_url: {url}");
    }

    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn device_info(&self) -> String {
        NullPlatform.device_info()
    }

    fn has_all_files_access(&self) -> bool {
        true
    }

    fn set_keep_screen_on(&self, _on: bool) {}
    fn set_secure_screen(&self, _on: bool) {}
    fn set_screen_brightness(&self, _brightness: Option<f32>) {}
    fn battery_level(&self) -> Option<u8> {
        None
    }
    fn set_clipboard(&self, _text: &str) {}
    fn get_clipboard(&self) -> Option<String> {
        None
    }
    fn system_proxy(&self) -> Option<String> {
        std::env::var("HTTPS_PROXY").ok().or_else(|| std::env::var("HTTP_PROXY").ok())
    }
    fn save_image_to_gallery(&self, _path: &PathBuf, _mime: &str) -> bool {
        false
    }
    fn share_file(&self, _path: &PathBuf, _mime: &str) {}
    fn request_all_files_access(&self) {}
}

fn dirs_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(dir);
    }
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(dir);
    }
    PathBuf::from(".")
}
