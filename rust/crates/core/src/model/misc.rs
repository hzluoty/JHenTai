use serde::{Deserialize, Serialize};

use super::gallery::{FavoriteSortOrder, GalleryUrl};
use super::tag::TagData;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryNote {
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchiveUnlockResult {
    pub success: bool,
    pub msg: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryArchive {
    #[serde(rename = "gpCount")]
    pub gp_count: Option<i64>,
    #[serde(rename = "creditCount")]
    pub credit_count: Option<i64>,
    #[serde(rename = "originalCost")]
    pub original_cost: String,
    #[serde(rename = "originalSize")]
    pub original_size: String,
    #[serde(rename = "downloadOriginalHint")]
    pub download_original_hint: String,
    #[serde(rename = "resampleCost")]
    pub resample_cost: Option<String>,
    #[serde(rename = "resampleSize")]
    pub resample_size: Option<String>,
    #[serde(rename = "downloadResampleHint")]
    pub download_resample_hint: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryHHArchive {
    #[serde(rename = "resolutionDesc")]
    pub resolution_desc: String,
    pub resolution: Option<String>,
    pub size: String,
    pub cost: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryHHInfo {
    #[serde(rename = "gpCount")]
    pub gp_count: Option<i64>,
    #[serde(rename = "creditCount")]
    pub credit_count: Option<i64>,
    pub archives: Vec<GalleryHHArchive>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryTorrent {
    pub title: String,
    #[serde(rename = "postTime")]
    pub post_time: String,
    pub size: String,
    pub seeds: i64,
    pub peers: i64,
    pub downloads: i64,
    pub uploader: String,
    #[serde(rename = "torrentUrl")]
    pub torrent_url: String,
    #[serde(rename = "magnetUrl")]
    pub magnet_url: String,
    pub outdated: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VisitStat {
    pub period: String,
    pub visits: f64,
    pub hits: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryStats {
    #[serde(rename = "totalVisits")]
    pub total_visits: i64,
    #[serde(rename = "allTimeRanking")]
    pub all_time_ranking: Option<i64>,
    #[serde(rename = "allTimeScore")]
    pub all_time_score: Option<i64>,
    #[serde(rename = "yearRanking")]
    pub year_ranking: Option<i64>,
    #[serde(rename = "yearScore")]
    pub year_score: Option<i64>,
    #[serde(rename = "monthRanking")]
    pub month_ranking: Option<i64>,
    #[serde(rename = "monthScore")]
    pub month_score: Option<i64>,
    #[serde(rename = "dayRanking")]
    pub day_ranking: Option<i64>,
    #[serde(rename = "dayScore")]
    pub day_score: Option<i64>,
    #[serde(rename = "yearlyStats")]
    pub yearly_stats: Vec<VisitStat>,
    #[serde(rename = "monthlyStats")]
    pub monthly_stats: Vec<VisitStat>,
    #[serde(rename = "dailyStats")]
    pub daily_stats: Vec<VisitStat>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WatchedTag {
    #[serde(rename = "tagId")]
    pub tag_id: i64,
    #[serde(rename = "tagData")]
    pub tag_data: TagData,
    pub watched: bool,
    pub hidden: bool,
    #[serde(rename = "backgroundColor")]
    pub background_color: Option<u32>,
    pub weight: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    pub number: i64,
    pub name: String,
    pub selected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrontPageDisplayType {
    Minimal,
    MinimalPlus,
    Compact,
    Extended,
    Thumbnail,
}

impl Default for FrontPageDisplayType {
    fn default() -> Self {
        Self::Minimal
    }
}

#[derive(Debug, Clone, Default)]
pub struct SiteSettingInfo {
    pub prefer_japanese_title: bool,
    pub profiles: Vec<Profile>,
    pub front_page_display_type: FrontPageDisplayType,
    pub is_large_thumbnail: bool,
    pub thumbnail_rows: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ImageLimitInfo {
    pub is_donator: bool,
    pub current_consumption: Option<i64>,
    pub total_limit: Option<i64>,
    pub reset_cost: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct EventInfo {
    pub dawn_info: Option<String>,
    pub hv_url: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct MyTagsInfo {
    pub tag_sets: Vec<(i64, String)>,
    pub tag_set_enable: bool,
    pub tag_set_background_color: Option<u32>,
    pub tags: Vec<WatchedTag>,
    pub apikey: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryHistoryModel {
    #[serde(rename = "galleryUrl")]
    pub gallery_url: GalleryUrl,
    pub title: String,
    pub category: String,
    #[serde(rename = "coverUrl")]
    pub cover_url: String,
    #[serde(rename = "pageCount")]
    pub page_count: i64,
    pub rating: f64,
    pub language: String,
    pub uploader: String,
    #[serde(rename = "publishTime")]
    pub publish_time: String,
    #[serde(rename = "isExpunged")]
    pub is_expunged: bool,
    pub tags: Vec<String>,
}

/// 排序下拉框里能读出的两种顺序。
pub fn favorite_sort_order_from_text(text: Option<&str>) -> Option<FavoriteSortOrder> {
    match text {
        Some("Published Time") => Some(FavoriteSortOrder::PublishedTime),
        Some("Favorited Time") => Some(FavoriteSortOrder::FavoritedTime),
        _ => None,
    }
}
