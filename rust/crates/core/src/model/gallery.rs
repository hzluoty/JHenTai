use serde::{Deserialize, Serialize};

use super::tag::TagMap;

/// E-H / EX 画廊 URL。token 恒为 10 位。
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GalleryUrl {
    #[serde(rename = "isEH")]
    pub is_eh: bool,
    pub gid: i64,
    pub token: String,
}

impl GalleryUrl {
    pub fn new(is_eh: bool, gid: i64, token: impl Into<String>) -> Self {
        Self { is_eh, gid, token: token.into() }
    }

    /// 与 Dart 版同一正则；返回 None 表示不是画廊链接。
    pub fn try_parse(url: &str) -> Option<Self> {
        let caps = GALLERY_URL_RE.captures(url)?;
        let e = caps.get(1)?.as_str();
        let gid = caps.get(2)?.as_str().parse().ok()?;
        let token = caps.get(3)?.as_str().to_string();
        Some(Self { is_eh: e == "-", gid, token })
    }

    pub fn parse(url: &str) -> Result<Self, String> {
        Self::try_parse(url).ok_or_else(|| format!("parse gallery url failed: {url}"))
    }

    pub fn url(&self) -> String {
        let host = if self.is_eh { "e-hentai.org" } else { "exhentai.org" };
        format!("https://{host}/g/{}/{}", self.gid, self.token)
    }
}

static GALLERY_URL_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"https://e([-x])hentai\.org/g/(\d+)/([a-z0-9]{10})").unwrap());

/// 图片页 URL：`https://e-hentai.org/s/<token>/<gid>-<pageNo>`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GalleryImagePageUrl {
    #[serde(rename = "isEH")]
    pub is_eh: bool,
    pub image_token: String,
    pub gid: i64,
    #[serde(rename = "pageNo")]
    pub page_no: i64,
}

impl GalleryImagePageUrl {
    pub fn try_parse(url: &str) -> Option<Self> {
        let caps = IMAGE_PAGE_URL_RE.captures(url)?;
        Some(Self {
            is_eh: caps.get(1)?.as_str() == "-",
            image_token: caps.get(2)?.as_str().to_string(),
            gid: caps.get(3)?.as_str().parse().ok()?,
            page_no: caps.get(4)?.as_str().parse().ok()?,
        })
    }

    pub fn parse(url: &str) -> Result<Self, String> {
        Self::try_parse(url).ok_or_else(|| format!("parse gallery image page url failed: {url}"))
    }

    pub fn url(&self) -> String {
        let host = if self.is_eh { "e-hentai.org" } else { "exhentai.org" };
        format!("https://{host}/s/{}/{}-{}", self.image_token, self.gid, self.page_no)
    }
}

static IMAGE_PAGE_URL_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"https://e([-x])hentai\.org/s/([a-z0-9]{10})/(\d+)-(\d+)").unwrap()
});

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryImage {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(rename = "originalImageUrl", skip_serializing_if = "Option::is_none")]
    pub original_image_url: Option<String>,
    #[serde(rename = "originalImageHeight", skip_serializing_if = "Option::is_none")]
    pub original_image_height: Option<f64>,
    #[serde(rename = "originalImageWidth", skip_serializing_if = "Option::is_none")]
    pub original_image_width: Option<f64>,
    #[serde(rename = "reloadKey", skip_serializing_if = "Option::is_none")]
    pub reload_key: Option<String>,
    #[serde(rename = "imageHash", skip_serializing_if = "Option::is_none")]
    pub image_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

impl GalleryImage {
    pub fn url_only(url: impl Into<String>) -> Self {
        Self { url: url.into(), ..Default::default() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Gallery {
    #[serde(rename = "galleryUrl")]
    pub gallery_url: GalleryUrl,
    pub title: String,
    pub category: String,
    pub cover: GalleryImage,
    #[serde(rename = "pageCount")]
    pub page_count: Option<i64>,
    pub rating: f64,
    #[serde(rename = "hasRated")]
    pub has_rated: bool,
    #[serde(rename = "favoriteTagIndex")]
    pub favorite_tag_index: Option<i64>,
    #[serde(rename = "favoriteTagName")]
    pub favorite_tag_name: Option<String>,
    pub language: Option<String>,
    pub uploader: Option<String>,
    #[serde(rename = "publishTime")]
    pub publish_time: String,
    #[serde(rename = "isExpunged")]
    pub is_expunged: bool,
    pub tags: TagMap,
}

impl Gallery {
    pub fn gid(&self) -> i64 {
        self.gallery_url.gid
    }

    pub fn token(&self) -> &str {
        &self.gallery_url.token
    }

    pub fn is_favorite(&self) -> bool {
        self.favorite_tag_index.is_some() || self.favorite_tag_name.is_some()
    }

    /// Dart `Gallery.toJson` 的字段子集（金样比对用）。
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "gid": self.gid(),
            "token": self.token(),
            "title": self.title,
            "category": self.category,
            "cover": self.cover,
            "pageCount": self.page_count,
            "rating": self.rating,
            "hasRated": self.has_rated,
            "isFavorite": self.is_favorite(),
            "favoriteTagIndex": self.favorite_tag_index,
            "favoriteTagName": self.favorite_tag_name,
            "galleryUrl": self.gallery_url.url(),
            "language": self.language,
            "uploader": self.uploader,
            "publishTime": self.publish_time,
            "isExpunged": self.is_expunged,
            "tags": self.tags,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FavoriteSortOrder {
    FavoritedTime,
    PublishedTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryCountType {
    /// 1,465,200 / about 232,805 / 50,000+
    Accurate,
    /// hundreds of
    Hundreds,
    /// thousands of
    Thousands,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GalleryCount {
    pub count: Option<String>,
    #[serde(rename = "type")]
    pub kind: GalleryCountType,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryPageInfo {
    #[serde(rename = "totalCount")]
    pub total_count: Option<GalleryCount>,
    #[serde(rename = "favoriteSortOrder")]
    pub favorite_sort_order: Option<FavoriteSortOrder>,
    pub galleries: Vec<Gallery>,
    #[serde(rename = "prevGid")]
    pub prev_gid: Option<String>,
    #[serde(rename = "nextGid")]
    pub next_gid: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryThumbnail {
    pub href: String,
    #[serde(rename = "isLarge")]
    pub is_large: bool,
    #[serde(rename = "thumbUrl")]
    pub thumb_url: String,
    #[serde(rename = "thumbHeight")]
    pub thumb_height: Option<f64>,
    #[serde(rename = "thumbWidth")]
    pub thumb_width: Option<f64>,
    #[serde(rename = "offSet")]
    pub offset: Option<f64>,
    #[serde(rename = "originImageHash")]
    pub origin_image_hash: Option<String>,
}

impl GalleryThumbnail {
    /// 用户开 MPV 时把 href 换成普通一页一图链接。
    pub fn replaced_mpv_href(&self, page_no: i64) -> String {
        let Some(hash) = self.origin_image_hash.as_deref() else {
            return self.href.clone();
        };
        let Some(caps) = MPV_RE.captures(&self.href) else {
            return self.href.clone();
        };
        let prefix = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
        let gid = caps.get(2).map(|m| m.as_str()).unwrap_or_default();
        let head: String = hash.chars().take(10).collect();
        format!("{prefix}/s/{head}/{gid}-{page_no}")
    }
}

static MPV_RE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(.+?)/mpv/(.+?)/").unwrap());
