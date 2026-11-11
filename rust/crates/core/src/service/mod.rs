//! 服务层：登录、搜索、阅读进度、历史、标签翻译等（对应 `lib/src/service/`）。
//!
//! 所有服务都只依赖 core 内的 net/db/config/parser，不碰 UI 与平台 API。

pub mod account;
pub mod block_rule;
pub mod favorite;
pub mod history;
pub mod read_progress;
pub mod reader;
pub mod search;
