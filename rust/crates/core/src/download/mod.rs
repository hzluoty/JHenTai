//! 下载：画廊、归档、本地画廊。
//!
//! 磁盘布局必须与 Dart 版逐字兼容（覆盖安装后还要能读旧文件）：
//!
//! ```text
//! <downloadPath>/<gid> - <sanitizedTitle>/
//!     1.jpg 2.png ...            # serialNo 从 1 开始，扩展名取自 URL
//!     metadata                   # {"gallery": {...}, "images": "<json 字符串>"}
//!     ComicInfo.xml              # 下载完成时写
//! ```

pub mod comic_info;
pub mod gallery;
pub mod path_resolver;

pub use gallery::{DownloadProgress, GalleryDownload, GalleryDownloadStatus};
pub use path_resolver::*;
