//! 详情页下半部分：子画廊、评论、缩略图、评论时间。

use scraper::{ElementRef, Html, Node};

use super::datetime::format_eh_datetime;
use super::eh::*;
use crate::model::*;

/// `#gnd` 里的子画廊（新版画廊）。
pub(super) fn children_galleries(d: &Html) -> Vec<ChildGallery> {
    let Some(gn) = super::eh::first(d, "#gnd") else {
        return Vec::new();
    };
    let nodes: Vec<_> = gn.children().collect();
    let mut result = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        let Some(el) = scraper::ElementRef::wrap(*node) else { continue };
        if el.value().name() != "a" {
            continue;
        }
        let href = el.value().attr("href").unwrap_or_default();
        let Some(url) = GalleryUrl::try_parse(href) else { continue };
        let title: String = el.text().collect();
        let update_time = match nodes.get(i + 1) {
            Some(noderef) => match noderef.value() {
                Node::Text(t) => RE_ADDED.captures(t).map(|c| c[1].to_string()).unwrap_or_default(),
                _ => String::new(),
            },
            None => String::new(),
        };
        result.push(ChildGallery { gallery_url: url, title, update_time });
    }
    result
}

/// `Posted on 10 March 2022, 03:49[ by: xxx]` -> `2022-03-10 03:49`
pub(super) fn parse_posted_local_time(e: ElementRef) -> String {
    let desc = el_text(e, ".c2 > .c3").unwrap_or_default();
    let captured = RE_POSTED
        .captures(&desc)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    format_eh_datetime(&captured).unwrap_or(captured)
}

pub(super) fn parse_posted_edited_time(e: ElementRef) -> Option<String> {
    let raw = el_text(e, ".c8 > strong")?;
    Some(format_eh_datetime(&raw).unwrap_or(raw))
}

pub(super) fn parse_comments(elements: &[ElementRef]) -> Vec<GalleryComment> {
    elements
        .iter()
        .map(|e| GalleryComment {
            id: el_attr(*e, ".c6", "id")
                .and_then(|s| s.split('_').nth(1).and_then(parse_i64))
                .unwrap_or(0),
            username: el_text(*e, ".c2 > .c3 > a"),
            user_id: el_attr(*e, ".c2 > .c3 > a:nth-child(3)", "href")
                .and_then(|href| RE_SHOWUSER.captures(&href).and_then(|c| c[1].parse().ok())),
            score: el_text(*e, ".c2 > .c5.nosel > span").unwrap_or_default(),
            score_details: el_text(*e, ".c7")
                .map(|t| t.split(',').map(|d| d.trim().to_string()).collect())
                .unwrap_or_default(),
            content: first_el(*e, ".c6").map(|el| el.html()).unwrap_or_default(),
            time: parse_posted_local_time(*e),
            last_edit_time: parse_posted_edited_time(*e),
            from_me: el_text(*e, ".c2 > .c4.nosel > a").as_deref() == Some("Edit"),
            voted_up: first_el(*e, r#".c2 > .c4.nosel > a[style="color:blue"]:nth-child(1)"#).is_some(),
            voted_down: first_el(*e, r#".c2 > .c4.nosel > a[style="color:blue"]:nth-child(3)"#).is_some(),
        })
        .collect()
}

pub(super) fn detail_thumbnails(d: &Html) -> Vec<GalleryThumbnail> {
    let Some(root) = super::eh::first(d, "#gdt") else {
        return Vec::new();
    };
    if root.value().classes().next().is_some() {
        return new_thumbnails(root);
    }
    let small = all_el(root, ".gdtm");
    if !small.is_empty() {
        return old_small_thumbnails(&small);
    }
    old_large_thumbnails(&all_el(root, ".gdtl"))
}

fn new_thumbnails(root: ElementRef) -> Vec<GalleryThumbnail> {
    all_el(root, "a")
        .into_iter()
        .map(|element| {
            let href = element.value().attr("href").unwrap_or_default().to_string();
            let div = first_el(element, "div[style]");
            let origin_image_hash = div.and_then(|d| d.value().attr("data-orghash").map(str::to_string));
            let style = div.and_then(|d| d.value().attr("style")).unwrap_or_default().to_string();
            let mut thumb_url = RE_URL_IN_STYLE
                .captures(&style)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if thumb_url.starts_with('/') {
                // 相对路径：按 href 属于 EH 还是 EX 补全
                let base = match GalleryImagePageUrl::parse(&href) {
                    Ok(u) if u.is_eh => consts::EH_INDEX,
                    _ => consts::EX_INDEX,
                };
                thumb_url = format!("{base}{thumb_url}");
            }
            let offset = RE_OFFSET_PX
                .captures(&style)
                .and_then(|c| c.get(1))
                .and_then(|m| m.as_str().parse::<f64>().ok());
            GalleryThumbnail {
                href,
                thumb_url,
                is_large: offset.is_none(),
                thumb_width: Some(cap_f64(&style, &RE_WIDTH).unwrap_or(0.0)),
                thumb_height: Some(cap_f64(&style, &RE_HEIGHT).unwrap_or(0.0) - 1.0),
                offset,
                origin_image_hash,
            }
        })
        .collect()
}

fn cap_f64(style: &str, re: &regex::Regex) -> Option<f64> {
    re.captures(style)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok())
}

fn old_small_thumbnails(elements: &[ElementRef]) -> Vec<GalleryThumbnail> {
    elements
        .iter()
        .map(|element| {
            let style = el_attr(*element, "div", "style").unwrap_or_default();
            GalleryThumbnail {
                href: el_attr(*element, "div > a", "href").unwrap_or_default(),
                thumb_url: RE_URL_IN_STYLE
                    .captures(&style)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default(),
                is_large: false,
                thumb_width: Some(cap_f64(&style, &RE_WIDTH).unwrap_or(0.0)),
                thumb_height: Some(cap_f64(&style, &RE_HEIGHT).unwrap_or(0.0) - 1.0),
                offset: Some(cap_f64(&style, &RE_OFFSET_PX).unwrap_or(0.0)),
                origin_image_hash: None,
            }
        })
        .collect()
}

fn old_large_thumbnails(elements: &[ElementRef]) -> Vec<GalleryThumbnail> {
    elements
        .iter()
        .map(|element| {
            let thumb_url = el_attr(*element, "a > img", "src").unwrap_or_default();
            let parts: Vec<&str> = thumb_url.split('-').collect();
            GalleryThumbnail {
                href: el_attr(*element, "a", "href").unwrap_or_default(),
                is_large: true,
                thumb_width: parts.get(2).and_then(|s| s.parse().ok()),
                thumb_height: parts.get(3).and_then(|s| s.parse().ok()),
                thumb_url,
                offset: None,
                origin_image_hash: None,
            }
        })
        .collect()
}

pub(super) fn thumbnails_page_count(d: &Html) -> i64 {
    // 分页器结构：<table class="ptt"><tr><td>&lt;</td><td><a>1</a></td>…<td>&gt;</td></tr></table>
    // 倒数第二格是最后一页的页码（Dart 版同样取 len-2）。
    let cells: Vec<ElementRef> = super::eh::all(d, ".ptt tr")
        .first()
        .map(|tr| tr.children().filter_map(ElementRef::wrap).collect())
        .unwrap_or_default();
    cells
        .get(cells.len().saturating_sub(2))
        .and_then(|td| first_el(*td, "a"))
        .and_then(|a| parse_i64(&a.text().collect::<Vec<_>>().join("")))
        .unwrap_or(1)
}
