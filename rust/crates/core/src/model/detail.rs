use serde::{Deserialize, Serialize};

use super::gallery::{GalleryImage, GalleryThumbnail, GalleryUrl};
use super::tag::TagMap;

/// 评论 HTML 片段：Dart 版存 `Element`，Rust 版存规范化 HTML 字符串。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryComment {
    pub id: i64,
    pub username: Option<String>,
    #[serde(rename = "userId")]
    pub user_id: Option<i64>,
    pub score: String,
    #[serde(rename = "scoreDetails")]
    pub score_details: Vec<String>,
    pub content: String,
    pub time: String,
    #[serde(rename = "lastEditTime")]
    pub last_edit_time: Option<String>,
    #[serde(rename = "fromMe")]
    pub from_me: bool,
    #[serde(rename = "votedUp")]
    pub voted_up: bool,
    #[serde(rename = "votedDown")]
    pub voted_down: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChildGallery {
    #[serde(rename = "galleryUrl")]
    pub gallery_url: GalleryUrl,
    pub title: String,
    #[serde(rename = "updateTime")]
    pub update_time: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryDetail {
    #[serde(rename = "galleryUrl")]
    pub gallery_url: GalleryUrl,
    #[serde(rename = "rawTitle")]
    pub raw_title: String,
    #[serde(rename = "japaneseTitle")]
    pub japanese_title: Option<String>,
    pub category: String,
    pub cover: GalleryImage,
    #[serde(rename = "pageCount")]
    pub page_count: i64,
    pub rating: f64,
    #[serde(rename = "realRating")]
    pub real_rating: f64,
    #[serde(rename = "hasRated")]
    pub has_rated: bool,
    #[serde(rename = "ratingCount")]
    pub rating_count: i64,
    #[serde(rename = "favoriteTagIndex")]
    pub favorite_tag_index: Option<i64>,
    #[serde(rename = "favoriteTagName")]
    pub favorite_tag_name: Option<String>,
    #[serde(rename = "favoriteCount")]
    pub favorite_count: i64,
    pub language: String,
    pub uploader: Option<String>,
    #[serde(rename = "publishTime")]
    pub publish_time: String,
    #[serde(rename = "isExpunged")]
    pub is_expunged: bool,
    pub tags: TagMap,
    pub size: String,
    #[serde(rename = "torrentCount")]
    pub torrent_count: String,
    #[serde(rename = "torrentPageUrl")]
    pub torrent_page_url: String,
    #[serde(rename = "archivePageUrl")]
    pub archive_page_url: String,
    #[serde(rename = "parentGalleryUrl")]
    pub parent_gallery_url: Option<GalleryUrl>,
    #[serde(rename = "childrenGalleries")]
    pub children_galleries: Vec<ChildGallery>,
    pub comments: Vec<GalleryComment>,
    pub thumbnails: Vec<GalleryThumbnail>,
    #[serde(rename = "thumbnailsPageCount")]
    pub thumbnails_page_count: i64,
}

impl GalleryDetail {
    pub fn is_favorite(&self) -> bool {
        self.favorite_tag_index.is_some() || self.favorite_tag_name.is_some()
    }

    /// 新版本画廊 = 子画廊里最后一个。
    pub fn new_version_gallery_url(&self) -> Option<&GalleryUrl> {
        self.children_galleries.last().map(|c| &c.gallery_url)
    }
}

/// 详情页 `galleryUrl` + `apikey`
#[derive(Debug, Clone)]
pub struct DetailPageResult {
    pub detail: GalleryDetail,
    pub apikey: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DetailPageInfo {
    #[serde(rename = "imageNoFrom")]
    pub image_no_from: i64,
    #[serde(rename = "imageNoTo")]
    pub image_no_to: i64,
    #[serde(rename = "imageCount")]
    pub image_count: i64,
    #[serde(rename = "currentPageNo")]
    pub current_page_no: i64,
    #[serde(rename = "pageCount")]
    pub page_count: i64,
    pub thumbnails: Vec<GalleryThumbnail>,
}

impl DetailPageInfo {
    /// 20 40 50 100 200 400
    pub fn thumbnails_count_per_page(&self) -> i64 {
        if self.current_page_no != self.page_count {
            self.thumbnails.len() as i64
        } else if self.page_count != 1 {
            (self.image_no_from - 1) / self.current_page_no
        } else {
            [20, 40, 50, 100, 200, 400]
                .into_iter()
                .find(|n| *n >= self.image_count)
                .unwrap_or(400)
        }
    }
}
