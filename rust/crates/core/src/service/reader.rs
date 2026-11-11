//! 阅读：图片页 → 原图 URL → 字节流。
//!
//! 对应 Dart `pages/read/read_page_logic.dart` 的网络部分。UI 与预加载留给 app 层。

use std::sync::{Arc, Mutex};

use crate::image::{self, RgbaImage};
use crate::model::GalleryImage;
use crate::net::http::{get, get_bytes, Client};
use crate::net::{FrontingResolver, NetState};
use crate::parser::{self, ParseError};

#[derive(Debug)]
pub enum ReadError {
    /// 509 / 配额，需要提示用户
    ExceedLimit,
    /// 页面结构不认识（新版本图片页）
    UnsupportedStyle,
    /// 画廊已被删除 / 不可见
    Deleted(String),
    Network(String),
    Parse(String),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExceedLimit => write!(f, "exceedImageLimits"),
            Self::UnsupportedStyle => write!(f, "unsupportedImagePageStyle"),
            Self::Deleted(msg) => write!(f, "{msg}"),
            Self::Network(e) => write!(f, "network: {e}"),
            Self::Parse(e) => write!(f, "parse: {e}"),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<ParseError> for ReadError {
    fn from(e: ParseError) -> Self {
        match &e {
            ParseError::InvalidValue(v) if v == "exceedImageLimits" => Self::ExceedLimit,
            ParseError::InvalidValue(v) if v == "unsupportedImagePageStyle" => Self::UnsupportedStyle,
            other => Self::Parse(other.to_string()),
        }
    }
}

/// 打开一个图片页，拿到原图 URL（`imagePage2GalleryImage`）
pub async fn resolve_image_page(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    page_url: &str,
) -> Result<GalleryImage, ReadError> {
    let response = get(client, state, resolver, page_url).await.map_err(|e| ReadError::Network(e.to_string()))?;

    // 404 页里带「已删除」提示
    if response.status == 404 {
        return Err(ReadError::Deleted(
            parser::a404_gallery_deleted_hint(&response.body).unwrap_or_else(|| "invisibleHints".into()),
        ));
    }

    parser::image_page_image(&response.body).map_err(ReadError::from)
}

/// 下载并解码一张阅读图（按屏幕宽度降采样）
pub async fn load_image(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    url: &str,
    screen_width: u32,
) -> Result<RgbaImage, ReadError> {
    load_image_with_referer(client, state, resolver, url, None, screen_width).await
}

/// 带 referer 的版本：E-H 图床拒绝无 referer 的直链
pub async fn load_image_with_referer(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    url: &str,
    referer: Option<&str>,
    screen_width: u32,
) -> Result<RgbaImage, ReadError> {
    let (status, bytes) = crate::net::http::get_bytes_with_referer(client, state, resolver, url, referer)
        .await
        .map_err(|e| ReadError::Network(e.to_string()))?;
    if status != 200 {
        return Err(ReadError::Network(format!("status {status}")));
    }
    image::decode_for_reader(&bytes, screen_width).map_err(|e| {
        // 带上长度与头部字节，便于区分「HTML 错误页」和「真损坏图片」
        let head: String = bytes.iter().take(12).map(|b| format!("{b:02x}")).collect();
        ReadError::Parse(format!("{e} ({} bytes, head {head})", bytes.len()))
    })
}

/// 抓全量缩略图：多页画廊（`thumbnails_page_count > 1`）逐页 `?p=N` 抓。
///
/// 抓取循环留在 core（app 只消费结果），对应 Dart 的 `readPageLogic` 预抓逻辑。
pub async fn fetch_all_thumbnails(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    gallery_url: &crate::model::GalleryUrl,
    first_page: &[crate::model::GalleryThumbnail],
    page_count: i64,
) -> Result<Vec<crate::model::GalleryThumbnail>, ReadError> {
    if page_count <= 1 {
        return Ok(first_page.to_vec());
    }

    let mut all: Vec<crate::model::GalleryThumbnail> = first_page.to_vec();
    let base = archive_base(&gallery_url.url());

    for page in 1..page_count {
        let url = format!("{base}?p={page}");
        let response = match crate::net::http::get(client, state, resolver, &url).await {
            Ok(r) => r,
            // 单页失败不整体失败：阅读器缺页比完全打不开好
            Err(_) => continue,
        };
        let thumbs = crate::parser::detail_thumbnails_of(&response.body);
        all.extend(thumbs);
    }
    Ok(all)
}

/// `/g/<gid>/<token>` 形式（去掉结尾斜杠，追加 `?p=N` 时才正确）
fn archive_base(gallery_url: &str) -> String {
    gallery_url.trim_end_matches('/').to_string()
}

/// 图片页 URL：用缩略图的 href（`/s/<token>/<gid>-<n>`）拼成绝对地址
pub fn absolute_page_url(href: &str, is_ex: bool) -> String {
    if href.starts_with("http") {
        return href.to_string();
    }
    let base = if is_ex {
        crate::net::consts::EX_INDEX
    } else {
        crate::net::consts::EH_INDEX
    };
    format!("{base}{href}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_url_keeps_absolute_hrefs() {
        assert_eq!(
            absolute_page_url("https://exhentai.org/s/abc/1-0", true),
            "https://exhentai.org/s/abc/1-0"
        );
        assert_eq!(absolute_page_url("/s/abc/1-0", false), "https://e-hentai.org/s/abc/1-0");
        assert_eq!(absolute_page_url("/s/abc/1-0", true), "https://exhentai.org/s/abc/1-0");
    }

    #[test]
    fn gallery_base_strips_trailing_slash() {
        assert_eq!(super::archive_base("https://e-hentai.org/g/1/aaaaaaaaaa/"), "https://e-hentai.org/g/1/aaaaaaaaaa");
        assert_eq!(super::archive_base("https://e-hentai.org/g/1/aaaaaaaaaa"), "https://e-hentai.org/g/1/aaaaaaaaaa");
    }

    #[test]
    fn parse_errors_map_to_read_errors() {
        let limit: ReadError = ParseError::InvalidValue("exceedImageLimits".into()).into();
        assert!(matches!(limit, ReadError::ExceedLimit));
        let style: ReadError = ParseError::InvalidValue("unsupportedImagePageStyle".into()).into();
        assert!(matches!(style, ReadError::UnsupportedStyle));
        assert_eq!(ReadError::ExceedLimit.to_string(), "exceedImageLimits");
    }
}
