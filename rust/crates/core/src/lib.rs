//! E-Hentai / EXHentai 核心：模型、解析、网络、数据库、设置、服务。
//!
//! 只依赖纯 Rust 库，可在 host 上跑全部测试（计划 §4.2 的 crate 划分标准）。

pub mod config;
pub mod db;
pub mod download;
pub mod image;
pub mod model;
pub mod net;
pub mod parser;
pub mod service;

pub use model::*;
