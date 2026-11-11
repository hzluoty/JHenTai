//! 平台层：`trait Platform`，Android 用 JNI 实现，host 用 stub。
//!
//! core 不直接依赖 Android/JNI（计划 §4.5 的护栏）：需要平台能力时由 app
//! 通过本 crate 提供的 trait 注入。

use std::path::PathBuf;

pub mod host;
#[cfg(target_os = "android")]
pub mod android;

/// 平台能力。所有方法都必须能在 Java 主线程之外调用（必要时代码里 runOnUiThread）。
pub trait Platform: Send + Sync {
    /// `getExternalFilesDir(null)` 等价的私有外部目录
    fn external_data_path(&self) -> PathBuf;

    /// 屏幕常亮 / 取消
    fn set_keep_screen_on(&self, on: bool);

    /// 防截屏
    fn set_secure_screen(&self, on: bool);

    /// 阅读时调亮度：0.0..=1.0，None 恢复系统默认
    fn set_screen_brightness(&self, brightness: Option<f32>);

    /// 电量百分比
    fn battery_level(&self) -> Option<u8>;

    /// 剪贴板
    fn set_clipboard(&self, text: &str);
    fn get_clipboard(&self) -> Option<String>;

    /// 用外部应用打开链接
    fn open_url(&self, url: &str);

    /// 系统代理（`ConnectivityManager.getDefaultProxy`）
    fn system_proxy(&self) -> Option<String>;

    /// 保存图片到相册（MediaStore）；返回是否成功
    fn save_image_to_gallery(&self, path: &PathBuf, mime: &str) -> bool;

    /// 分享文件（FileProvider）
    fn share_file(&self, path: &PathBuf, mime: &str);

    /// 版本号与设备信息
    fn app_version(&self) -> String;
    fn device_info(&self) -> String;

    /// 是否已获得全部文件访问权限（Android 11+ 的 MANAGE_EXTERNAL_STORAGE）
    fn has_all_files_access(&self) -> bool;

    /// 请求全部文件访问权限（跳系统设置页）
    fn request_all_files_access(&self);
}

/// 用于测试与桌面预览的空实现。
#[derive(Debug, Default, Clone, Copy)]
pub struct NullPlatform;

impl Platform for NullPlatform {
    fn external_data_path(&self) -> PathBuf {
        std::env::temp_dir()
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
    fn open_url(&self, _url: &str) {}
    fn system_proxy(&self) -> Option<String> {
        None
    }
    fn save_image_to_gallery(&self, _path: &PathBuf, _mime: &str) -> bool {
        false
    }
    fn share_file(&self, _path: &PathBuf, _mime: &str) {}
    fn app_version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
    fn device_info(&self) -> String {
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
    }
    fn has_all_files_access(&self) -> bool {
        true
    }
    fn request_all_files_access(&self) {}
}
