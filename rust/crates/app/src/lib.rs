//! JHenTai Rust 应用层：Slint UI + controller。
//!
//! Android 上编译为 `libjhentai.so`，由 Kotlin 壳（NativeActivity 子类）加载；
//! 桌面上编译为普通 bin，用于快速迭代（platform 用 host stub）。

pub mod nav;
#[cfg(target_os = "android")]
pub mod android;
pub mod pages;
pub mod store;

slint::include_modules!();

pub use pages::AppController;
