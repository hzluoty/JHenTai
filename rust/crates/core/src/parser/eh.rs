//! E-H 页面解析。逐函数对应 Dart 版 `utils/eh_spider_parser.dart`，选择器保持不变。

use scraper::{ElementRef, Html, Selector};

use crate::model::*;

/// 站内常量：站点地址的唯一出处是 `crate::net::consts`，这里只补解析专用值。
pub mod consts {
    pub use crate::net::consts::{EH_INDEX, EX_INDEX};

    pub const EH_509_IMAGE_URL: &str = "https://ehgt.org/g/509.gif";
    pub const EX_509_IMAGE_URL: &str = "https://s.exhentai.org/img/509.gif";

    /// 收藏夹颜色 -> 索引（`ui_config.dart` 的 `favoriteTagIndex`）
    pub const FAVORITE_TAG_INDEX: [(&str, i64); 10] = [
        ("000", 0),
        ("f00", 1),
        ("fa0", 2),
        ("dd0", 3),
        ("080", 4),
        ("9f4", 5),
        ("4bf", 6),
        ("00f", 7),
        ("508", 8),
        ("e8e", 9),
    ];
}

#[derive(Debug)]
pub enum ParseError {
    MissingField(&'static str),
    InvalidValue(String),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "missing field: {field}"),
            Self::InvalidValue(v) => write!(f, "invalid value: {v}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl From<String> for ParseError {
    fn from(s: String) -> Self {
        Self::InvalidValue(s)
    }
}

pub type Result<T> = std::result::Result<T, ParseError>;

pub(crate) fn sel(css: &str) -> Selector {
    Selector::parse(css).expect("valid selector")
}

pub(crate) fn doc(html: &str) -> Html {
    // 与 Dart `parse(data)` 一致：容忍残缺 HTML，不报错。
    Html::parse_document(html)
}

pub(crate) fn text(e: ElementRef) -> String {
    e.text().collect::<Vec<_>>().join("")
}

/// Dart `.text`（html 包）：保留自身与后代节点文本，但**不**做空白归一。
pub(crate) fn attr<'a>(e: ElementRef<'a>, name: &str) -> Option<&'a str> {
    e.value().attr(name)
}

pub(crate) fn first<'a>(root: &'a Html, css: &str) -> Option<ElementRef<'a>> {
    root.select(&sel(css)).next()
}

pub(crate) fn all<'a>(root: &'a Html, css: &str) -> Vec<ElementRef<'a>> {
    root.select(&sel(css)).collect()
}

pub(crate) fn parse_i64(s: &str) -> Option<i64> {
    s.trim().replace(',', "").parse().ok()
}

/// `int.parse(x.split(' ')[0])`
fn leading_int(s: &str) -> Option<i64> {
    s.split(' ').next().and_then(parse_i64)
}

static RE_QUERY_NEXT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"next=([\d-]+)").unwrap());
static RE_QUERY_PREV: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"prev=([\d-]+)").unwrap());
pub(crate) static RE_COUNT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"([\d,\\+]+)").unwrap());
pub(crate) static RE_PAGE_NO: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"p(age)?=(\d+)").unwrap());
pub(crate) static RE_COVER: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"width:(\d+)px.*height:(\d+)px.*url\((.*)\)").unwrap()
});
pub(crate) static RE_SIZE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(\d+)").unwrap());
pub(crate) static RE_OFFSET: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"-*(\d+)+px").unwrap());
pub(crate) static RE_BORDER_COLOR: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"border-color:#(\w{3});").unwrap());
pub(crate) static RE_FAV_OFFSET: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"background-position:0px -(\d+)px").unwrap());
pub(crate) static RE_REAL_RATING: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"Average: (\d+.\d+\d+)").unwrap());
pub(crate) static RE_APIKEY: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r#"var apikey = "(\w+)""#).unwrap());
pub(crate) static RE_APIKEY2: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r#"apikey = \"(.*)\""#).unwrap());
pub(crate) static RE_ONCLICK_URL: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"'([^']+)'").unwrap());
pub(crate) static RE_TAG_COLOR: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"color:#(.*?);").unwrap());
pub(crate) static RE_TAG_BG: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"background:radial-gradient\(#.*,#(.*)\)").unwrap()
});
pub(crate) static RE_URL_IN_STYLE: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"url\((.+)\)").unwrap());
pub(crate) static RE_WIDTH: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"width:(\d+)?px").unwrap());
pub(crate) static RE_HEIGHT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"height:(\d+)?px").unwrap());
pub(crate) static RE_OFFSET_PX: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"\) -(\d+)?px ").unwrap());
pub(crate) static RE_ADDED: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"added (\d\d\d\d-\d\d-\d\d \d\d:\d\d)").unwrap());
pub(crate) static RE_SHOWUSER: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"showuser=(\d+)").unwrap());
pub(crate) static RE_POSTED: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"Posted on (.+, .+)( by:)?").unwrap());
pub(crate) static RE_SHAH: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"f_shash=(\w+)").unwrap());
pub(crate) static RE_RELOAD_KEY: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"return nl\('(.*)'\)").unwrap());
pub(crate) static RE_ORIGINAL_WH: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"(\d+) x (\d+)").unwrap());
pub(crate) static RE_IMG_HEIGHT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"height:(\d+)px").unwrap());
pub(crate) static RE_IMG_WIDTH: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"width:(\d+)px").unwrap());
pub(crate) static RE_GP: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"([\d,]+) GP").unwrap());
pub(crate) static RE_CREDIT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"([\d,]+) Credits").unwrap());
pub(crate) static RE_HATH: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"You currently have ([\d,.]+) Hath").unwrap());
pub(crate) static RE_CREDIT2: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"([\d,k ]+)Credits").unwrap());
pub(crate) static RE_GP2: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"([\d,k ]+)GP").unwrap());
pub(crate) static RE_HH_RES: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"'(\w+)'").unwrap());
pub(crate) static RE_TORRENT_COUNT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"\d+").unwrap());
pub(crate) static RE_COPYRIGHT: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"This gallery is unavailable due to a copyright claim by (.*).$").unwrap()
});

fn query_gid(root: &Html, css: &str, re: &regex::Regex) -> Option<String> {
    let href = first(root, css).and_then(|e| attr(e, "href").map(str::to_string));
    re.captures(href.as_deref().unwrap_or("")).map(|c| c[1].to_string())
}

// ---------------------------------------------------------------- 列表页

/// 自动识别三种列表视图（Minimal / Compact+Extended / Thumbnail）。
pub fn gallery_page(html: &str) -> Result<GalleryPageInfo> {
    let d = doc(html);
    if first(&d, ".itg.gltm").is_some() {
        return Ok(parse_minimal_page(&d));
    }
    if first(&d, ".itg.gltc").is_some() {
        return Ok(parse_compact_page(&d));
    }
    if first(&d, ".itg.glte").is_some() {
        return Ok(parse_extended_page(&d));
    }
    if first(&d, ".itg.gld").is_some() {
        return Ok(parse_thumbnail_page(&d));
    }
    Ok(parse_compact_page(&d))
}

fn sort_order(d: &Html) -> Option<FavoriteSortOrder> {
    let text = first(d, ".searchnav > div > select > option[selected]").map(text);
    favorite_sort_order_from_text(text.as_deref())
}

fn total_count(d: &Html) -> Option<GalleryCount> {
    let text = first(d, ".searchtext").map(text)?;
    if text.contains("hundreds of") {
        return Some(GalleryCount { count: None, kind: GalleryCountType::Hundreds });
    }
    if text.contains("thousands of") {
        return Some(GalleryCount { count: None, kind: GalleryCountType::Thousands });
    }
    let count = RE_COUNT.captures(&text)?.get(1)?.as_str().to_string();
    Some(GalleryCount { count: Some(count), kind: GalleryCountType::Accurate })
}

fn page_nav(d: &Html) -> (Option<String>, Option<String>) {
    (query_gid(d, "#uprev", &RE_QUERY_PREV), query_gid(d, "#unext", &RE_QUERY_NEXT))
}

fn parse_minimal_page(d: &Html) -> GalleryPageInfo {
    let galleries = all(d, ".itg.gltm > tbody > tr")
        .into_iter()
        // 去掉广告与表头
        .filter(|tr| tr.children().count() != 1 && first_el(*tr, "th").is_none())
        .map(parse_minimal_gallery)
        .collect();
    let (prev, next) = page_nav(d);
    GalleryPageInfo { total_count: total_count(d), favorite_sort_order: sort_order(d), galleries, prev_gid: prev, next_gid: next }
}

fn parse_compact_page(d: &Html) -> GalleryPageInfo {
    let galleries = all(d, ".itg.gltc > tbody > tr")
        .into_iter()
        .filter(|tr| tr.children().count() != 1 && first_el(*tr, "th").is_none())
        .map(parse_compact_gallery)
        .collect();
    let (prev, next) = page_nav(d);
    GalleryPageInfo { total_count: total_count(d), favorite_sort_order: sort_order(d), galleries, prev_gid: prev, next_gid: next }
}

fn parse_extended_page(d: &Html) -> GalleryPageInfo {
    let galleries = all(d, ".itg.glte > tbody > tr")
        .into_iter()
        .filter(|tr| tr.children().count() != 1)
        .map(parse_extended_gallery)
        .collect();
    let (prev, next) = page_nav(d);
    GalleryPageInfo { total_count: total_count(d), favorite_sort_order: sort_order(d), galleries, prev_gid: prev, next_gid: next }
}

fn parse_thumbnail_page(d: &Html) -> GalleryPageInfo {
    let galleries = all(d, ".itg.gld > div").into_iter().map(parse_thumbnail_gallery).collect();
    let (prev, next) = page_nav(d);
    GalleryPageInfo { total_count: total_count(d), favorite_sort_order: sort_order(d), galleries, prev_gid: prev, next_gid: next }
}

pub(crate) fn first_el<'a>(e: ElementRef<'a>, css: &str) -> Option<ElementRef<'a>> {
    e.select(&sel(css)).next()
}

pub(crate) fn all_el<'a>(e: ElementRef<'a>, css: &str) -> Vec<ElementRef<'a>> {
    e.select(&sel(css)).collect()
}

pub(crate) fn el_text(e: ElementRef, css: &str) -> Option<String> {
    first_el(e, css).map(text)
}

pub(crate) fn el_attr(e: ElementRef, css: &str, name: &str) -> Option<String> {
    first_el(e, css).and_then(|x| attr(x, name).map(str::to_string))
}

/// `.ir` 背景偏移 -> 0..5 星（`_parseGalleryRating`）
pub(crate) fn parse_rating(e: ElementRef) -> f64 {
    let style = el_attr(e, ".ir", "style").unwrap_or_default();
    if style.is_empty() {
        return 0.0;
    }
    let offsets: Vec<&str> = RE_OFFSET.captures_iter(&style).map(|c| c.get(0).unwrap().as_str()).collect();
    if offsets.len() < 2 {
        return 0.0;
    }
    let x: i64 = offsets[0].replace("px", "").parse().unwrap_or(0);
    let y: i64 = offsets[1].replace("px", "").parse().unwrap_or(0);
    let mut value = 5.0;
    value -= -(x as f64) / 16.0;
    value -= if y == -21 { 0.5 } else { 0.0 };
    value
}

fn has_rated(e: ElementRef, css: &str) -> bool {
    el_attr(e, css, "class").map(|c| c.split(' ').count() > 1).unwrap_or(false)
}

fn cover_from(img: Option<ElementRef>) -> Option<GalleryImage> {
    let img = img?;
    let url = attr(img, "data-src").or_else(|| attr(img, "src"))?.to_string();
    let style = attr(img, "style")?;
    let sizes: Vec<String> = RE_SIZE.captures_iter(style).map(|c| c[1].to_string()).collect();
    if sizes.len() < 2 {
        return None;
    }
    Some(GalleryImage {
        url,
        height: sizes[0].parse().ok(),
        width: sizes[1].parse().ok(),
        ..Default::default()
    })
}

fn favorite_index_by_border(style: Option<String>) -> Option<i64> {
    let style = style?;
    let color = RE_BORDER_COLOR.captures(&style).map(|c| c[1].to_string()).unwrap_or_default();
    favorite_index_of(&color)
}

fn favorite_index_of(color: &str) -> Option<i64> {
    consts::FAVORITE_TAG_INDEX
        .iter()
        .find(|(c, _)| *c == color)
        .map(|(_, i)| *i)
}

fn parse_gallery_tags(e: ElementRef, css: &str) -> TagMap {
    let mut tags = TagMap::new();
    for div in all_el(e, css) {
        let pair = attr(div, "title").unwrap_or_default();
        if pair.is_empty() {
            continue;
        }
        let mut list = pair.split(':');
        let namespace = list.next().unwrap_or("");
        let namespace = if namespace.is_empty() { "temp" } else { namespace };
        let key = list.next().unwrap_or("");
        let style = attr(div, "style").unwrap_or_default();
        let color = RE_TAG_COLOR.captures(style).map(|c| c[1].to_string());
        let bg = RE_TAG_BG.captures(style).map(|c| c[1].to_string());
        tags.push(GalleryTag {
            color: color.as_deref().and_then(parse_hex_color),
            background_color: bg.as_deref().and_then(parse_hex_color),
            tag_data: TagData::new(namespace, key),
            ..Default::default()
        });
    }
    tags
}

fn parse_hex_color(hex: &str) -> Option<u32> {
    u32::from_str_radix(hex, 16).ok().map(|c| 0xFF00_0000 | c)
}

fn parse_minimal_gallery(tr: ElementRef) -> Gallery {
    let cover = cover_from(first_el(tr, ".gl2m > .glthumb > div > img")).unwrap_or_default();
    Gallery {
        gallery_url: GalleryUrl::parse(&el_attr(tr, ".gl3m.glname > a", "href").unwrap_or_default()).unwrap_or_else(|_| {
            GalleryUrl::new(true, 0, "0000000000")
        }),
        title: el_text(tr, ".glink").unwrap_or_default(),
        category: el_text(tr, ".gl1m.glcat > div").unwrap_or_default(),
        cover,
        page_count: None,
        rating: parse_rating(tr),
        has_rated: has_rated(tr, ".gl4m > .ir"),
        favorite_tag_index: favorite_index_by_border(el_attr(tr, ".gl2m > div:nth-child(2) > [id][style]", "style")),
        favorite_tag_name: el_attr(tr, ".gl2m > div:nth-child(2) > [id][style]", "title"),
        language: None,
        uploader: el_text(tr, ".gl5m.glhide > div > a"),
        publish_time: el_text(tr, ".gl2m > div:nth-child(2)").unwrap_or_default(),
        is_expunged: first_el(tr, ".gl2m > div:nth-child(2) > s").is_some(),
        tags: TagMap::new(),
    }
}

fn parse_compact_gallery(tr: ElementRef) -> Gallery {
    let tags = parse_gallery_tags(tr, ".gt");
    let cover = cover_from(first_el(tr, ".gl2c > .glthumb > div > img")).unwrap_or_default();
    let page_count = {
        let divs = all_el(tr, ".gl4c.glhide > div");
        // 收藏页没有页数
        if divs.len() < 2 { None } else { leading_int(&text(divs[1])) }
    };
    Gallery {
        gallery_url: GalleryUrl::parse(&el_attr(tr, ".gl3c.glname > a", "href").unwrap_or_default())
            .unwrap_or_else(|_| GalleryUrl::new(true, 0, "0000000000")),
        title: el_text(tr, ".glink").unwrap_or_default(),
        category: el_text(tr, ".cn").unwrap_or_default(),
        cover,
        page_count,
        rating: parse_rating(tr),
        has_rated: has_rated(tr, ".gl2c > div:nth-child(2) > .ir") || has_rated(tr, ".gl2c .ir"),
        favorite_tag_index: favorite_index_by_border(el_attr(tr, ".gl2c > div:nth-child(2) > [id][style]", "style")),
        favorite_tag_name: el_attr(tr, ".gl2c > div:nth-child(2) > [id][style]", "title"),
        language: tags.get("language").and_then(|v| v.first()).map(|t| t.tag_data.key.clone()),
        uploader: el_text(tr, ".gl4c.glhide > div > a"),
        publish_time: el_text(tr, ".gl2c > div:nth-child(2) > [id]")
            .or_else(|| el_text(tr, ".gl2c [id^=posted_]"))
            .unwrap_or_default(),
        is_expunged: first_el(tr, ".gl2c [id^=posted_] > s").is_some(),
        tags,
    }
}

fn parse_extended_gallery(tr: ElementRef) -> Gallery {
    let tags = parse_gallery_tags(
        tr,
        ".gl2e > div > a > div > div:nth-child(1) > table > tbody > tr > td > div",
    );
    let cover = cover_from(first_el(tr, ".gl1e > div > a > img")).unwrap_or_default();
    let page_count = {
        let divs = all_el(tr, ".gl3e > div");
        if divs.len() < 5 { None } else { leading_int(&text(divs[4])) }
    };
    Gallery {
        gallery_url: GalleryUrl::parse(&el_attr(tr, ".gl1e > div > a", "href").unwrap_or_default())
            .unwrap_or_else(|_| GalleryUrl::new(true, 0, "0000000000")),
        title: el_text(tr, ".glink").unwrap_or_default(),
        category: el_text(tr, ".cn").unwrap_or_default(),
        cover,
        page_count,
        rating: parse_rating(tr),
        has_rated: has_rated(tr, ".gl3e > .ir"),
        favorite_tag_index: favorite_index_by_border(el_attr(tr, ".gl3e > [id][style]", "style")),
        favorite_tag_name: el_attr(tr, ".gl3e > [id][style]", "title"),
        language: tags.get("language").and_then(|v| v.first()).map(|t| t.tag_data.key.clone()),
        uploader: el_text(tr, ".gl3e > div > a"),
        publish_time: el_text(tr, ".gl3e > div[id]").unwrap_or_default(),
        is_expunged: first_el(tr, ".gl3e > div[id] > s").is_some(),
        tags,
    }
}

fn parse_thumbnail_gallery(div: ElementRef) -> Gallery {
    let cover = cover_from(first_el(div, ".gl3t > a > img")).unwrap_or_default();
    let page_count = {
        let divs = all_el(div, ".gl5t > div:nth-child(1) > div");
        if divs.len() < 2 { None } else { leading_int(&text(divs[1])) }
    };
    Gallery {
        gallery_url: GalleryUrl::parse(&el_attr(div, "a", "href").unwrap_or_default())
            .unwrap_or_else(|_| GalleryUrl::new(true, 0, "0000000000")),
        title: el_text(div, ".glink").unwrap_or_default(),
        category: el_text(div, ".cs").unwrap_or_default(),
        cover,
        page_count,
        rating: parse_rating(div),
        has_rated: has_rated(div, ".gl5t > div > .ir"),
        favorite_tag_index: favorite_index_by_border(el_attr(div, ".gl5t > div > [id][style]", "style")),
        favorite_tag_name: el_attr(div, ".gl5t > div > [id][style]", "title"),
        language: None,
        uploader: None,
        publish_time: el_text(div, ".gl5t > div > div[id]").unwrap_or_default(),
        is_expunged: first_el(div, ".gl5t > div > div[id] > s").is_some(),
        tags: TagMap::new(),
    }
}

/// Ranklist 页：画廊列表 + 总页数 + 前后页号（`ranklistPage2GalleryPageInfo`）
#[derive(Debug, Clone, Default)]
pub struct RanklistPageInfo {
    pub galleries: Vec<Gallery>,
    pub page_count: i64,
    pub prev_page_index: Option<i64>,
    pub next_page_index: Option<i64>,
}

pub fn ranklist_page(html: &str) -> RanklistPageInfo {
    let d = doc(html);
    let galleries = all(&d, ".itg.gltc > tbody > tr")
        .into_iter()
        .filter(|tr| tr.children().count() != 1 && first_el(*tr, "th").is_none())
        .map(parse_compact_gallery)
        .collect();

    let page_count = all(&d, ".ptt > tbody > tr")
        .first()
        .map(|tr| tr.children().filter_map(ElementRef::wrap).collect::<Vec<_>>())
        .filter(|tds| !tds.is_empty())
        .and_then(|tds| tds.get(tds.len().saturating_sub(2)).copied())
        .and_then(|td| first_el(td, "a"))
        .and_then(|a| parse_i64(&text(a)))
        .unwrap_or(0);

    let td_list: Vec<ElementRef> = all(&d, ".ptt > tbody > tr")
        .first()
        .map(|tr| tr.children().filter_map(ElementRef::wrap).collect())
        .unwrap_or_default();

    let next_page_index = td_list
        .last()
        .and_then(|td| first_el(*td, "a"))
        .and_then(|a| attr(a, "href").map(str::to_string))
        .and_then(|href| RE_PAGE_NO.captures(&href).map(|c| c[2].to_string()))
        .and_then(|n| n.parse().ok());

    let prev_page_index = td_list
        .first()
        .and_then(|td| first_el(*td, "a"))
        .and_then(|a| attr(a, "href").map(str::to_string))
        .and_then(|href| RE_PAGE_NO.captures(&href).map(|c| c[2].to_string()))
        .and_then(|n| n.parse().ok());

    RanklistPageInfo { galleries, page_count, prev_page_index, next_page_index }
}

// ---------------------------------------------------------------- 详情页

/// `detailPage2GalleryAndDetailAndApikey`
pub fn detail_page(html: &str) -> Result<DetailPageResult> {
    let d = doc(html);
    let missing = |f: &'static str| ParseError::MissingField(f);

    let gallery_href = first(&d, "#gd5 > p > a").and_then(|a| attr(a, "href").map(str::to_string)).ok_or(missing("#gd5 > p > a"))?;
    let gallery_url = GalleryUrl::parse(gallery_href.split('?').next().unwrap_or("")).map_err(ParseError::InvalidValue)?;

    let raw_title = first(&d, "#gn").map(text).ok_or(missing("#gn"))?;
    let japanese_title = first(&d, "#gj").map(text).unwrap_or_default();
    let cover_style = first(&d, "#gd1 > div").and_then(|e| attr(e, "style").map(str::to_string)).unwrap_or_default();
    let cover = match RE_COVER.captures(&cover_style) {
        Some(c) => GalleryImage { url: c[3].to_string(), height: c[2].parse().ok(), width: c[1].parse().ok(), ..Default::default() },
        None => return Err(ParseError::InvalidValue(format!("cover style: {cover_style}"))),
    };

    let category = first(&d, "#gdc > .cs").map(text).ok_or(missing("#gdc > .cs"))?;
    // Dart 取 `#gdd ... tr:nth-child(5)` 的数字；现网页面里该行是 File Size，
    // 页数在 Length 行，所以先按 Label 找 "Length"，找不到再退回固定行号。
    let page_count = gdd_value(&d, "Length")
        .and_then(|t| leading_int(&t))
        .ok_or(missing("page count"))?;
    let rating = first(&d, "#grt2").map(parse_rating).ok_or(missing("#grt2"))?;
    let favorite_tag_index = parse_favorite_tag_index_by_offset(&d);
    let favorite_tag_name = first(&d, "#fav > .i")
        .filter(|e| attr(*e, "style").is_some())
        .and_then(|_| first(&d, "#favoritelink").map(text));
    let language = parent_language(&d);
    let uploader = first(&d, "#gdn > a").map(text);
    let publish_time = first(&d, "#gdd > table > tbody > tr > .gdt2").map(text).unwrap_or_default();
    let is_expunged = gdd_value(&d, "Visible").map(|v| v.contains("Expunged")).unwrap_or(false);

    let detail = GalleryDetail {
        gallery_url,
        raw_title,
        japanese_title: if japanese_title.is_empty() { None } else { Some(japanese_title) },
        category,
        cover,
        page_count,
        rating,
        real_rating: parse_real_rating(&d),
        has_rated: first(&d, "#rating_image").and_then(|e| attr(e, "class")).map(|c| c.split(' ').count() > 1).unwrap_or(false),
        favorite_tag_index,
        favorite_tag_name,
        language,
        uploader,
        publish_time,
        is_expunged,
        rating_count: first(&d, "#rating_count").map(text).and_then(|t| parse_i64(&t)).unwrap_or(0),
        size: gdd_value(&d, "File Size").unwrap_or_default(),
        favorite_count: parse_favorite_count(&d),
        torrent_count: first(&d, "#gd5").and_then(|gd5| gd5.children().filter_map(ElementRef::wrap).nth(2)).and_then(|td| first_el(td, "a")).map(text).and_then(|t| RE_TORRENT_COUNT.find(&t).map(|m| m.as_str().to_string())).unwrap_or_else(|| "0".to_string()),
        torrent_page_url: onclick_url(&d, "#gd5", 2),
        archive_page_url: onclick_url(&d, "#gd5", 1),
        parent_gallery_url: first(&d, "#gdd > table > tbody > tr:nth-child(1) > .gdt2 > a").and_then(|a| attr(a, "href")).and_then(GalleryUrl::try_parse),
        children_galleries: super::eh_detail::children_galleries(&d),
        comments: super::eh_detail::parse_comments(&all(&d, "#cdiv > .c1")),
        thumbnails: super::eh_detail::detail_thumbnails(&d),
        thumbnails_page_count: super::eh_detail::thumbnails_page_count(&d),
        tags: core_tags(&d),
    };

    // Dart: .gm 的 previousElementSibling 的 previousElementSibling（含 apikey 的 <script>）
    let script = first(&d, ".gm")
        .and_then(|e| e.prev_siblings().filter_map(ElementRef::wrap).next())
        .and_then(|e| e.prev_siblings().filter_map(ElementRef::wrap).next())
        .map(text)
        .unwrap_or_default();
    let apikey = RE_APIKEY.captures(&script).map(|c| c[1].to_string()).unwrap_or_default();

    Ok(DetailPageResult { detail, apikey })
}

/// `#gdd > table` 里标签为 `label` 的那一行的值单元格
fn gdd_value(d: &Html, label: &str) -> Option<String> {
    all(d, "#gdd > table tr")
        .into_iter()
        .find(|tr| tr.children().filter_map(ElementRef::wrap).next().map(text).map(|t| t.trim().trim_end_matches(':') == label).unwrap_or(false))
        .and_then(|tr| tr.children().filter_map(ElementRef::wrap).nth(1))
        .map(|td| text(td).trim().to_string())
}

fn parent_language(d: &Html) -> String {
    gdd_value(d, "Language").unwrap_or_default()
}

fn onclick_url(d: &Html, css: &str, child_index: usize) -> String {
    first(d, css)
        .and_then(|gd5| gd5.children().filter_map(ElementRef::wrap).nth(child_index))
        .and_then(|td| first_el(td, "a"))
        .and_then(|a| attr(a, "onclick").map(str::to_string))
        .and_then(|onclick| RE_ONCLICK_URL.captures(&onclick).map(|c| c[1].to_string()))
        .unwrap_or_default()
}

/// `_detailPageDocument2Tags`：id 形如 `language:english`
fn core_tags(d: &Html) -> TagMap {
    let mut tags = TagMap::new();
    for div in all(d, "#taglist td:nth-of-type(1) > div[id]").into_iter().chain(all(d, "#taglist td:not(:first-child) > div[id]")) {
        let pair = attr(div, "id").unwrap_or_default();
        if pair.is_empty() {
            continue;
        }
        let list: Vec<&str> = pair.split(':').collect();
        // Dart: list[0].split('_')[1]（这里 id 形如 td_language:english，取 '_' 后的部分作 namespace）
        let namespace = if list.len() == 2 && !list[0].is_empty() {
            list[0].split_once('_').map(|(_, ns)| ns).unwrap_or("temp").to_string()
        } else {
            "temp".to_string()
        };
        let key = if list.len() == 1 {
            // Dart: list[0].substring(3) —— 去掉 `td_` 前缀
            list[0].get(3..).unwrap_or(list[0]).replace('_', " ")
        } else {
            list[1].replace('_', " ")
        };
        let tag_status = match attr(div, "class") {
            Some("gtl") => EhTagStatus::Skepticism,
            Some("gtw") => EhTagStatus::Incorrect,
            _ => EhTagStatus::Confidence,
        };
        let vote_status = match first_el(div, "a").and_then(|a| attr(a, "class")) {
            Some("tup") => EhTagVoteStatus::Up,
            Some("tdn") => EhTagVoteStatus::Down,
            _ => EhTagVoteStatus::None,
        };
        tags.push(GalleryTag { tag_data: TagData::new(namespace, key), tag_status: Some(tag_status), vote_status: Some(vote_status), ..Default::default() });
    }
    tags
}

fn parse_favorite_tag_index_by_offset(d: &Html) -> Option<i64> {
    let style = first(d, "#fav > .i").and_then(|e| attr(e, "style"))?;
    let offset: i64 = RE_FAV_OFFSET.captures(style)?.get(1)?.as_str().parse().ok()?;
    Some((offset - 2) / 19)
}

fn parse_real_rating(d: &Html) -> f64 {
    let raw = first(d, "#rating_label").map(text).unwrap_or_default();
    RE_REAL_RATING.captures(&raw).and_then(|c| c[1].parse().ok()).unwrap_or(0.0)
}

fn parse_favorite_count(d: &Html) -> i64 {
    match first(d, "#favcount").map(text) {
        None => 0,
        Some(c) if c == "Never" => 0,
        Some(c) if c == "Once" => 1,
        Some(c) => c.split(' ').next().and_then(parse_i64).unwrap_or(0),
    }
}

/// `detailPage2Thumbnails`
pub fn detail_thumbnails_of(html: &str) -> Vec<GalleryThumbnail> {
    super::eh_detail::detail_thumbnails(&doc(html))
}

/// `detailPage2RangeAndThumbnails`
pub fn detail_page_info(html: &str) -> Result<DetailPageInfo> {
    let d = doc(html);
    let desc = first(&d, ".gtb > .gpc").map(text).unwrap_or_default().replace(',', "");
    let re = regex::Regex::new(r"Showing (\d+) - (\d+) of (\d+) images").unwrap();
    let caps = re.captures(&desc).ok_or(ParseError::InvalidValue(desc.clone()))?;
    Ok(DetailPageInfo {
        image_no_from: caps[1].parse::<i64>().unwrap_or(0) - 1,
        image_no_to: caps[2].parse::<i64>().unwrap_or(0) - 1,
        image_count: caps[3].parse::<i64>().unwrap_or(0),
        current_page_no: first(&d, ".ptds > a").map(text).and_then(|t| parse_i64(&t)).unwrap_or(0),
        page_count: super::eh_detail::thumbnails_page_count(&d),
        thumbnails: super::eh_detail::detail_thumbnails(&d),
    })
}

/// `detailPage2Comments`
pub fn detail_comments(html: &str) -> Vec<GalleryComment> {
    let d = doc(html);
    super::eh_detail::parse_comments(&all(&d, "#cdiv > .c1"))
}
