use serde::{Deserialize, Serialize};

use super::gallery::{GalleryImage, GalleryUrl};
use super::tag::TagMap;

/// 下载目录里 `metadata` JSON 的结构（与 Dart `GalleryMetadata.toJson` 一致）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryMetadata {
    #[serde(rename = "galleryUrl")]
    pub gallery_url: GalleryUrl,
    pub title: String,
    #[serde(rename = "japaneseTitle")]
    pub japanese_title: String,
    pub category: String,
    pub cover: GalleryImage,
    #[serde(rename = "pageCount")]
    pub page_count: i64,
    pub rating: f64,
    pub language: String,
    pub uploader: Option<String>,
    #[serde(rename = "publishTime")]
    pub publish_time: String,
    #[serde(rename = "isExpunged")]
    pub is_expunged: bool,
    pub size: String,
    #[serde(rename = "torrentCount")]
    pub torrent_count: i64,
    pub tags: TagMap,
}

/// E-H API `gdata` 里 `filesize` 的字节数格式化（`byte_util.dart`）。
pub fn byte_to_string(bytes: f64) -> String {
    if bytes < 1024.0 {
        return format!("{}B", bytes as i64);
    }
    let kb = bytes / 1024.0;
    if kb < 1024.0 {
        return format!("{kb:.2}KB");
    }
    let mb = kb / 1024.0;
    if mb < 1024.0 {
        return format!("{mb:.2}MB");
    }
    format!("{:.2}GB", mb / 1024.0)
}

#[cfg(test)]
mod tests {
    #[test]
    fn byte_to_string_matches_dart() {
        assert_eq!(super::byte_to_string(512.0), "512B");
        assert_eq!(super::byte_to_string(2048.0), "2.00KB");
        assert_eq!(super::byte_to_string(5.0 * 1024.0 * 1024.0), "5.00MB");
    }
}
