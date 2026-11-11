//! 页面 controller。每个页面 = `.slint` 组件 + 这里的 controller（替代 GetX `*_logic`）。

pub mod cover_loader;
pub mod details;
pub mod pump;
pub mod reader;
pub mod search;
pub mod settings;
pub mod downloads;
pub mod gallery_list;

pub use gallery_list::AppController;
