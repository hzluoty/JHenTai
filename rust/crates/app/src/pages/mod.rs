//! 页面 controller。每个页面 = `.slint` 组件 + 这里的 controller（替代 GetX `*_logic`）。

pub mod gallery_list;

pub use gallery_list::AppController;
