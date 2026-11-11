//! 其余 E-H 页面/JSON 解析：图片页、种子、归档、统计、标签集、设置页等。
//! 逐函数对应 Dart `eh_spider_parser.dart` 的后半部分。

use scraper::ElementRef;

use super::eh::*;
use crate::model::*;

// ---------------------------------------------------------------- 图片页

/// `imagePage2GalleryUrl`
pub fn image_page_gallery_url(html: &str) -> Option<GalleryUrl> {
    let d = doc(html);
    let href = first(&d, "#i5 > .sb > a").and_then(|a| attr(a, "href"))?;
    GalleryUrl::try_parse(href)
}

/// `imagePage2GalleryImage`
pub fn image_page_image(html: &str) -> Result<GalleryImage> {
    let d = doc(html);
    let Some(img) = first(&d, "#img") else {
        if first(&d, "#pane_images").is_some() {
            return Err(ParseError::InvalidValue("unsupportedImagePageStyle".into()));
        }
        return Err(ParseError::MissingField("#img"));
    };
    let style = attr(img, "style").ok_or(ParseError::MissingField("#img style"))?;
    let url = attr(img, "src").ok_or(ParseError::MissingField("#img src"))?.to_string();
    if url == consts::EH_509_IMAGE_URL || url == consts::EX_509_IMAGE_URL {
        return Err(ParseError::InvalidValue("exceedImageLimits".into()));
    }
    let height = RE_IMG_HEIGHT.captures(style).and_then(|c| c[1].parse().ok());
    let width = RE_IMG_WIDTH.captures(style).and_then(|c| c[1].parse().ok());

    let image_hash = first(&d, "#i6 div a")
        .and_then(|a| attr(a, "href"))
        .and_then(|h| RE_SHAH.captures(h).map(|c| c[1].to_string()))
        .ok_or(ParseError::MissingField("#i6 div a"))?;

    // #i6 a[id] 的父节点 -> 下一个兄弟 -> 里面的 <a>（原始图片链接）
    let original = first(&d, "#i6 a[id]")
        .and_then(|a| a.parent())
        .and_then(ElementRef::wrap)
        .and_then(|p| p.next_siblings().filter_map(ElementRef::wrap).next())
        .and_then(|s| first_el(s, "a"));
    let original_image_url = original.and_then(|a| attr(a, "href").map(str::to_string));
    let wh = original
        .map(text)
        .and_then(|t| RE_ORIGINAL_WH.captures(&t).map(|c| (c[1].to_string(), c[2].to_string())));

    let reload_key = first(&d, "#loadfail")
        .and_then(|e| attr(e, "onclick"))
        .and_then(|oc| RE_RELOAD_KEY.captures(oc).map(|c| c[1].to_string()))
        .ok_or(ParseError::MissingField("#loadfail"))?;

    Ok(GalleryImage {
        url,
        height,
        width,
        original_image_url,
        original_image_width: wh.as_ref().and_then(|(w, _)| w.parse().ok()),
        original_image_height: wh.as_ref().and_then(|(_, h)| h.parse().ok()),
        reload_key: Some(reload_key),
        image_hash: Some(image_hash),
        path: None,
    })
}

// ---------------------------------------------------------------- 评论 / 登录 / 用户

/// `sendComment2ErrorMsg`
pub fn send_comment_error_msg(html: &str) -> Option<String> {
    if html.is_empty() {
        return None;
    }
    first(&doc(html), "p.br").map(text)
}

/// `loginPage2UserInfoOrErrorMsg`：`Ok((ipb_member_id, ipb_pass_hash))` 或 `Err(错误键)`
pub fn login_page_user_info(html: &str, set_cookie: &[String]) -> std::result::Result<(i64, String), String> {
    let with_id = set_cookie.iter().find(|c| c.contains("ipb_member_id"));
    let with_hash = set_cookie.iter().find(|c| c.contains("ipb_pass_hash"));
    match (with_id, with_hash) {
        (Some(id), Some(hash)) => {
            let id_re = regex::Regex::new(r"ipb_member_id=(\d+);").unwrap();
            let hash_re = regex::Regex::new(r"ipb_pass_hash=(\w+);").unwrap();
            let member_id = id_re
                .captures(id)
                .and_then(|c| c[1].parse().ok())
                .ok_or_else(|| "userNameOrPasswordMismatch".to_string())?;
            let pass_hash = hash_re
                .captures(hash)
                .map(|c| c[1].to_string())
                .ok_or_else(|| "userNameOrPasswordMismatch".to_string())?;
            Ok((member_id, pass_hash))
        }
        _ => Err(login_error_msg(html)),
    }
}

fn login_error_msg(html: &str) -> String {
    if html.contains("The captcha was not entered correctly") {
        "needCaptcha".to_string()
    } else {
        "userNameOrPasswordMismatch".to_string()
    }
}

/// `forumPage2UserName`
pub fn forum_page_user_name(html: &str) -> Option<String> {
    first(&doc(html), ".home > b > a").map(text)
}

/// `profilePage2UserInfo`：None 表示 cookie 失效（未登录）
pub fn profile_page_user_info(html: &str) -> Option<(String, String, Option<String>)> {
    let d = doc(html);
    if first(&d, ".pcen").is_some() {
        return None;
    }
    let nick_name = first(&d, "#profilename").map(text)?;
    let user_name = first(&d, ".home > b > a").map(text)?;
    let avatar = first(&d, "#profilename")
        .and_then(|e| e.next_siblings().filter_map(ElementRef::wrap).next())
        .and_then(|e| e.next_siblings().filter_map(ElementRef::wrap).next())
        .and_then(|e| first_el(e, "img"))
        .and_then(|img| attr(img, "src").map(str::to_string));
    Some((user_name, nick_name, avatar))
}

// ---------------------------------------------------------------- 收藏

/// `favoritePopup2FavoriteTagNames`
pub fn favorite_popup_tag_names(html: &str) -> Vec<String> {
    let d = doc(html);
    all(&d, ".nosel > div")
        .into_iter()
        .map(|div| el_text(div, "div:nth-child(5)").unwrap_or_default())
        .collect()
}

/// `favoritePopup2GalleryNote`
pub fn favorite_popup_note(html: &str) -> Result<GalleryNote> {
    let d = doc(html);
    let note = first(&d, "#galpop > div > div:nth-child(3) > textarea")
        .map(text)
        .ok_or(ParseError::MissingField("favorite note"))?;
    Ok(GalleryNote { note })
}

/// `favoritePage2FavoriteTagsAndCounts`：最后一个 `.fp` 不是收藏夹
pub fn favorite_page_tags_and_counts(html: &str) -> (Vec<String>, Vec<i64>) {
    let d = doc(html);
    let mut divs = all(&d, ".nosel > .fp");
    divs.pop();
    let mut names = Vec::new();
    let mut counts = Vec::new();
    for div in divs {
        names.push(el_text(div, "div:last-child").unwrap_or_default());
        counts.push(el_text(div, "div:first-child").and_then(|t| parse_i64(&t)).unwrap_or(0));
    }
    (names, counts)
}

// ---------------------------------------------------------------- 种子

/// `torrentPage2GalleryTorrent`
pub fn torrent_page_torrents(html: &str) -> Vec<GalleryTorrent> {
    let d = doc(html);
    all(&d, "#torrentinfo > div > form")
        .into_iter()
        // 已删除的画廊没有下载链接
        .filter(|form| first_el(*form, "div > table > tbody > tr:nth-child(4) > td > a").is_some())
        .filter_map(|form| {
            let trs = all_el(form, "div > table > tbody > tr");
            let title = el_text(*trs.get(2)?, "td > a")?;
            let torrent_url = el_attr(*trs.get(2)?, "td > a", "href")?;
            let post_span = first_el(*trs.first()?, "td:nth-child(1) > span:nth-child(2)")?;
            let strip = |css: &str, from: usize| -> i64 {
                el_text(*trs.first().unwrap(), css)
                    .and_then(|t| t.get(from..).map(str::to_string))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0)
            };
            Some(GalleryTorrent {
                title,
                post_time: text(post_span),
                size: el_text(*trs.first()?, "td:nth-child(3)").and_then(|s| s.get(6..).map(str::to_string)).unwrap_or_default(),
                seeds: strip("td:nth-child(7)", 7),
                peers: strip("td:nth-child(9)", 7),
                downloads: strip("td:nth-child(11)", 11),
                uploader: el_text(*trs.get(1)?, "td:nth-child(1)")?.get(10..).unwrap_or_default().to_string(),
                magnet_url: format!(
                    "magnet:?xt=urn:btih:{}",
                    torrent_url.split('.').nth(1).and_then(|s| s.split('/').next_back()).unwrap_or_default()
                ),
                outdated: attr(post_span, "style").map(|s| s.contains("color:red")).unwrap_or(false),
                torrent_url,
            })
        })
        .collect()
}

// ---------------------------------------------------------------- 设置页 / mytags / 统计

/// `settingPage2SiteSetting`
pub fn setting_page_site_setting(html: &str) -> Result<SiteSettingInfo> {
    let d = doc(html);
    let items = all(&d, ".optouter");
    let profiles = all(&d, "#profile_form > select > option")
        .into_iter()
        .filter_map(|e| {
            Some(Profile {
                number: attr(e, "value")?.parse().ok()?,
                name: text(e),
                selected: attr(e, "selected").is_some(),
            })
        })
        .collect();

    let front = items
        .get(8)
        .and_then(|e| first_el(*e, "div > p > label > input[checked=checked]"))
        .and_then(|input| input.parent())
        .and_then(ElementRef::wrap)
        .map(text)
        .unwrap_or_default();

    let front_page_display_type = match front.as_str() {
        " Minimal" => FrontPageDisplayType::Minimal,
        " Minimal+" => FrontPageDisplayType::MinimalPlus,
        " Compact" => FrontPageDisplayType::Compact,
        " Extended" => FrontPageDisplayType::Extended,
        " Thumbnail" => FrontPageDisplayType::Thumbnail,
        _ => FrontPageDisplayType::Minimal,
    };

    let prefer_japanese_title = first(&d, "#tl_1").and_then(|e| attr(e, "checked")).is_some();
    let is_new_thumbnail_size = all(&d, "#tssel > div").len() > 2;
    let thumbnail_size = first(&d, "#tssel > div > label > input[checked=checked]")
        .and_then(|i| i.parent())
        .and_then(ElementRef::wrap)
        .map(text)
        .unwrap_or_default();
    let is_large_thumbnail = if is_new_thumbnail_size { thumbnail_size != " Small" } else { thumbnail_size == " Large" };
    let thumbnail_rows = first(&d, "#trsel > div > label > input[checked=checked]")
        .and_then(|i| i.parent())
        .and_then(ElementRef::wrap)
        .map(text)
        .and_then(|t| parse_i64(&t))
        .unwrap_or(4);

    Ok(SiteSettingInfo { prefer_japanese_title, profiles, front_page_display_type, is_large_thumbnail, thumbnail_rows })
}

/// `homePage2ImageLimit`
pub fn home_page_image_limit(html: &str) -> ImageLimitInfo {
    let d = doc(html);
    let is_donator = first(&d, r#".stuffbox > .homebox > form > p > input[value="Reset Quota"]"#).is_some();
    if !is_donator {
        return ImageLimitInfo::default();
    }
    let num = |css: &str| first(&d, css).map(text).and_then(|t| parse_i64(&t));
    ImageLimitInfo {
        is_donator,
        current_consumption: num(".stuffbox > .homebox > p > strong:nth-child(1)"),
        total_limit: num(".stuffbox > .homebox > p > strong:nth-child(3)"),
        reset_cost: num(".stuffbox > .homebox > p:nth-child(3) > strong"),
    }
}

/// `myTagsPage2TagSetNamesAndTagSetsAndApikey`
pub fn my_tags_page(html: &str) -> Result<MyTagsInfo> {
    let d = doc(html);
    let tag_sets = all(&d, "#tagset_outer > div > select > option")
        .into_iter()
        .filter_map(|o| Some((attr(o, "value")?.parse().ok()?, text(o))))
        .collect();
    let tag_set_enable = first(&d, "#tagset_enable[checked]").is_some();
    let tag_set_background_color = first(&d, "#tagcolor").and_then(|e| attr(e, "value")).and_then(argb_from_hex);
    let tags = all(&d, "#usertags_outer > div")
        .into_iter()
        .filter(|div| attr(*div, "id") != Some("usertag_0"))
        .filter_map(|div| {
            let tag_id: i64 = attr(div, "id")?.replace("usertag_", "").parse().ok()?;
            let title = first(&d, &format!("#tagpreview_{tag_id}"))
                .and_then(|e| attr(e, "title"))
                .unwrap_or_default()
                .to_string();
            let mut list = title.split(':');
            let first_part = list.next().unwrap_or("");
            let (namespace, key) = match list.next() {
                Some(k) if !first_part.is_empty() => (first_part, k),
                _ => ("temp", first_part),
            };
            Some(WatchedTag {
                tag_id,
                tag_data: TagData::new(namespace, key),
                watched: first(&d, &format!("#tagwatch_{tag_id}[checked]")).is_some(),
                hidden: first(&d, &format!("#taghide_{tag_id}[checked]")).is_some(),
                background_color: first(&d, &format!("#tagcolor_{tag_id}")).and_then(|e| attr(e, "value")).and_then(argb_from_hex),
                weight: first(&d, &format!("#tagweight_{tag_id}"))
                    .and_then(|e| attr(e, "value"))
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
            })
        })
        .collect();
    let apikey = first(&d, "#outer > script:nth-child(1)")
        .map(text)
        .and_then(|t| RE_APIKEY2.captures(&t).map(|c| c[1].to_string()))
        .ok_or(ParseError::MissingField("mytags apikey"))?;
    Ok(MyTagsInfo { tag_sets, tag_set_enable, tag_set_background_color, tags, apikey })
}

/// `#RRGGBB` / `#AARRGGBB` -> ARGB
pub fn argb_from_hex(hex: &str) -> Option<u32> {
    let hex = hex.trim().trim_start_matches('#');
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(match hex.len() {
        6 => 0xFF00_0000 | v,
        8 => v,
        _ => return None,
    })
}

/// `statPage2GalleryStats`
pub fn stat_page_stats(html: &str) -> Result<GalleryStats> {
    let d = doc(html);
    let rank = first(&d, ".stuffbox > table > tbody");
    let num_in = |tbody: Option<ElementRef>, row: usize, col: usize| -> Option<i64> {
        tbody
            .and_then(|tb| first_el(tb, &format!("tr:nth-child({row}) > td:nth-child({col})")))
            .map(text)
            .and_then(|t| parse_i64(&t))
    };
    let graphs = all(&d, "#graphs > div");
    let stats_of = |idx: usize| -> Vec<VisitStat> {
        graphs
            .get(idx)
            .and_then(|g| first_el(*g, "table > tbody"))
            .map(parse_stats)
            .unwrap_or_default()
    };
    let total_visits = first(&d, ".stuffbox > p > strong")
        .map(text)
        .and_then(|t| parse_i64(&t))
        .ok_or(ParseError::MissingField("totalVisits"))?;

    Ok(GalleryStats {
        total_visits,
        all_time_ranking: num_in(rank, 2, 4),
        all_time_score: num_in(rank, 2, 5),
        year_ranking: num_in(rank, 4, 4),
        year_score: num_in(rank, 4, 5),
        month_ranking: num_in(rank, 6, 4),
        month_score: num_in(rank, 6, 5),
        day_ranking: num_in(rank, 8, 4),
        day_score: num_in(rank, 8, 5),
        yearly_stats: stats_of(2),
        monthly_stats: stats_of(1),
        daily_stats: stats_of(0),
    })
}

fn parse_stats(tbody: ElementRef) -> Vec<VisitStat> {
    let periods: Vec<String> = all_el(tbody, "tr:nth-child(4) > .stdk").into_iter().map(text).collect();
    let visits: Vec<String> = all_el(tbody, "tr:nth-child(6) > .stdv").into_iter().map(text).collect();
    let hits: Vec<String> = all_el(tbody, "tr:nth-child(8) > .stdv").into_iter().map(text).collect();

    let parse_number = |s: &str| -> f64 {
        if let Some(rest) = s.strip_suffix('K') {
            rest.parse::<f64>().map(|v| v * 1000.0).unwrap_or(0.0)
        } else if let Some(rest) = s.strip_suffix('M') {
            rest.parse::<f64>().map(|v| v * 1_000_000.0).unwrap_or(0.0)
        } else {
            s.parse().unwrap_or(0.0)
        }
    };

    let stats: Vec<VisitStat> = periods
        .iter()
        .enumerate()
        .map(|(i, period)| VisitStat {
            period: period.clone(),
            visits: visits.get(i).map(|v| parse_number(v)).unwrap_or(0.0),
            hits: hits.get(i).map(|v| parse_number(v)).unwrap_or(0.0),
        })
        .collect();

    // 去掉开头的空数据
    match stats.iter().position(|s| s.visits > 0.0) {
        Some(begin) => stats[begin..].to_vec(),
        None => Vec::new(),
    }
}

// ---------------------------------------------------------------- 首页事件 / 兑换 / Hath

/// `newsPage2Event`
pub fn news_page_event(html: &str) -> EventInfo {
    let d = doc(html);
    let Some(event_pane) = first(&d, "#eventpane") else {
        return EventInfo::default();
    };
    EventInfo {
        hv_url: first_el(event_pane, "div > a").and_then(|a| attr(a, "href").map(str::to_string)),
        dawn_info: first_el(event_pane, "div > p:nth-child(2)").map(text),
    }
}

/// `exchangePage2Assets` -> (credit, gp)
pub fn exchange_page_assets(html: &str) -> (String, String) {
    let d = doc(html);
    let next_text = |anchor_css: &str| {
        first(&d, anchor_css)
            .and_then(|e| e.parent())
            .and_then(ElementRef::wrap)
            .and_then(|e| e.next_siblings().filter_map(ElementRef::wrap).next())
            .map(text)
            .unwrap_or_default()
    };
    let credit_desc = next_text("#buyform");
    let gp_desc = next_text("#sellform");
    let credit = RE_CREDIT2.captures(&credit_desc).map(|c| c[1].trim().to_string()).unwrap_or_else(|| "-1".to_string());
    let gp = RE_GP2.captures(&gp_desc).map(|c| c[1].trim().to_string()).unwrap_or_else(|| "-1".to_string());
    (credit, gp)
}

/// `hathPage2Hath`
pub fn hath_page_hath(html: &str) -> Option<String> {
    let d = doc(html);
    let ps = all(&d, ".stuffbox > div > p");
    if ps.len() < 4 {
        return None;
    }
    RE_HATH.captures(&text(ps[3])).map(|c| c[1].trim().to_string())
}

// ---------------------------------------------------------------- 归档

/// `unlockArchivePage2DownloadArchivePageUrl`
pub fn unlock_archive_page(html: &str) -> ArchiveUnlockResult {
    if html.starts_with("You do not have enough funds to download this archive. Obtain some Credits or GP and try again.") {
        return ArchiveUnlockResult { success: false, msg: html.to_string(), url: None };
    }
    let d = doc(html);
    ArchiveUnlockResult {
        success: true,
        msg: "success".to_string(),
        url: first(&d, "#continue > a").and_then(|a| attr(a, "href").map(str::to_string)),
    }
}

/// `downloadArchivePage2DownloadUrl`
pub fn download_archive_page_url(html: &str) -> Option<String> {
    first(&doc(html), "#db > p > a").and_then(|a| attr(a, "href").map(str::to_string))
}

/// `archivePage2Archive`
pub fn archive_page(html: &str) -> Result<GalleryArchive> {
    let d = doc(html);
    let balance = first(&d, "#db > p:nth-child(4)").map(text).unwrap_or_default();
    Ok(GalleryArchive {
        gp_count: RE_GP.captures(&balance).and_then(|c| c[1].replace(',', "").parse().ok()),
        credit_count: RE_CREDIT.captures(&balance).and_then(|c| c[1].replace(',', "").parse().ok()),
        original_cost: first(&d, "#db > div > div > div > strong")
            .map(text)
            .ok_or(ParseError::MissingField("originalCost"))?
            .replace(',', ""),
        original_size: first(&d, "#db > div > div > p > strong").map(text).ok_or(ParseError::MissingField("originalSize"))?,
        download_original_hint: first(&d, "#db > div > div > form > div > input")
            .and_then(|e| attr(e, "value"))
            .ok_or(ParseError::MissingField("downloadOriginalHint"))?
            .to_string(),
        resample_cost: first(&d, "#db > div > div:nth-child(3) > div > strong").map(|e| text(e).replace(',', "")),
        resample_size: first(&d, "#db > div > div:nth-child(3) > p > strong").map(text),
        download_resample_hint: first(&d, "#db > div > div:nth-child(3) > form > div > input")
            .and_then(|e| attr(e, "value"))
            .ok_or(ParseError::MissingField("downloadResampleHint"))?
            .to_string(),
    })
}

/// `archivePage2HHInfo`
pub fn archive_page_hh_info(html: &str) -> Result<GalleryHHInfo> {
    let d = doc(html);
    let archives = all(&d, "table > tbody > tr > td")
        .into_iter()
        .filter_map(|td| {
            Some(GalleryHHArchive {
                resolution_desc: el_text(td, "p:nth-child(1)")?,
                resolution: first_el(td, "p:nth-child(1) > a")
                    .and_then(|a| attr(a, "onclick"))
                    .and_then(|oc| RE_HH_RES.captures(oc).map(|c| c[1].to_string())),
                size: el_text(td, "p:nth-child(3)")?,
                cost: el_text(td, "p:nth-child(5)")?,
            })
        })
        .collect();
    let balance = first(&d, "#db > p:nth-child(4)").map(text).unwrap_or_default();
    Ok(GalleryHHInfo {
        gp_count: RE_GP.captures(&balance).and_then(|c| c[1].replace(',', "").parse().ok()),
        credit_count: RE_CREDIT.captures(&balance).and_then(|c| c[1].replace(',', "").parse().ok()),
        archives,
    })
}

/// `downloadHHPage2Result`
pub fn download_hh_page_result(html: &str) -> String {
    first(&doc(html), "#db > p").map(text).unwrap_or_default()
}

// ---------------------------------------------------------------- 杂项

/// `tagSuggestion2TagList`
pub fn tag_suggestion(json: &str) -> Vec<EhRawTag> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(tags) = value.get("tags").and_then(|t| t.as_object()) else {
        return Vec::new();
    };
    tags.values()
        .filter_map(|t| {
            Some(EhRawTag {
                namespace: t.get("ns")?.as_str()?.to_string(),
                key: t.get("tn")?.as_str()?.to_string(),
            })
        })
        .collect()
}

/// `a404Page2GalleryDeletedHint`：i18n 键，或 `copyRightHints` + 版权方
pub fn a404_gallery_deleted_hint(html: &str) -> Option<String> {
    let d = doc(html);
    let first_text = first(&d, ".d > p")
        .and_then(|p| p.children().next())
        .and_then(|n| n.value().as_text().cloned())?;
    let hint = first_text.trim();
    if hint.is_empty() {
        return None;
    }
    if hint.contains("This gallery has been removed") {
        return Some("invisibleHints".to_string());
    }
    RE_COPYRIGHT.captures(hint).map(|c| format!("copyRightHints{}", &c[1]))
}

/// `galleryRatingResponse2RatingInfo` -> (rating_usr, rating_cnt, rating_avg)
pub fn rating_response(json: &str) -> Option<(f64, i64, f64)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let f = |k: &str| v.get(k).and_then(|x| x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok())));
    Some((f("rating_usr")?, v.get("rating_cnt")?.as_i64()?, f("rating_avg")?))
}

/// `voteTagResponse2ErrorMessage`
pub fn vote_tag_response_error(json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json).ok()?.get("error")?.as_str().map(str::to_string)
}

/// `votingCommentResponse2Score`
pub fn voting_comment_response_score(json: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(json).ok()?.get("comment_score")?.as_i64()
}

/// `addTagSetResponse2Result`：True 表示没有触顶
pub fn add_tag_set_response_ok(html: &str) -> bool {
    !html.contains("No more tags can be added to this tagset")
}

/// `githubReleasePage2LatestVersion`
pub fn github_release_latest_version(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.get(0)?.get("tag_name")?.as_str().map(str::to_string)
}

/// `latestReleaseResponse2Tag`：Location 头最后一段
pub fn latest_release_tag_from_location(location: &str) -> Option<String> {
    location.split('/').next_back().map(str::to_string)
}

// ---------------------------------------------------------------- 元数据 JSON

/// `galleryMetadataJson2GalleryMetadata` / `...Metadatas`
pub fn gallery_metadata(json: &str) -> Result<Vec<GalleryMetadata>> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| ParseError::InvalidValue(e.to_string()))?;
    let gmetadata = value
        .get("gmetadata")
        .and_then(|g| g.as_array())
        .filter(|a| !a.is_empty())
        .ok_or_else(|| ParseError::InvalidValue("getMetaDataFailed".into()))?;

    gmetadata
        .iter()
        .map(|map| {
            let s = |k: &str| map.get(k).and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let mut tags_map = TagMap::new();
            if let Some(arr) = map.get("tags").and_then(|t| t.as_array()) {
                for raw in arr.iter().filter_map(|t| t.as_str()) {
                    let mut list = raw.split(':');
                    let first = list.next().unwrap_or_default();
                    tags_map.push(match list.next() {
                        Some(key) => GalleryTag::new(TagData::new(first, key)),
                        None => GalleryTag::new(TagData::new("temp", first)),
                    });
                }
            }
            let language = tags_map
                .get("language")
                .and_then(|v| v.iter().find(|t| t.tag_data.key != "translated"))
                .map(|t| t.tag_data.key.clone())
                .unwrap_or_else(|| "Japanese".to_string());
            let posted: i64 = map.get("posted").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0);
            let filesize = map
                .get("filesize")
                .and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())))
                .unwrap_or(0.0);
            let uploader = s("uploader");
            Ok(GalleryMetadata {
                gallery_url: GalleryUrl::new(true, map.get("gid").and_then(|v| v.as_i64()).unwrap_or(0), s("token")),
                title: s("title"),
                japanese_title: s("title_jpn"),
                category: s("category"),
                cover: GalleryImage::url_only(s("thumb")),
                page_count: map.get("filecount").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0),
                rating: map.get("rating").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0.0),
                language,
                uploader: if uploader == "(Disowned)" { None } else { Some(uploader) },
                publish_time: format_timestamp(posted),
                is_expunged: map.get("expunged").and_then(|v| v.as_bool()).unwrap_or(false),
                size: byte_to_string(filesize),
                torrent_count: map.get("torrentcount").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0),
                tags: tags_map,
            })
        })
        .collect()
}

/// `galleryMetadataJson2FileSizeAndPosted`
pub fn gallery_metadata_filesize_and_posted(json: &str) -> Result<(i64, i64)> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| ParseError::InvalidValue(e.to_string()))?;
    let map = value
        .get("gmetadata")
        .and_then(|g| g.as_array())
        .and_then(|a| a.first())
        .ok_or_else(|| ParseError::InvalidValue("getMetaDataFailed".into()))?;
    Ok((
        map.get("filesize").and_then(|v| v.as_i64()).unwrap_or(0),
        map.get("posted").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(0),
    ))
}

/// Dart `DateTime.fromMillisecondsSinceEpoch(s*1000, isUtc: true).toString()` 的等价形式
pub fn format_timestamp(seconds: i64) -> String {
    let secs = seconds.rem_euclid(86_400);
    let days = seconds.div_euclid(86_400);
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let (y, mo, d) = super::datetime::civil_from_days(days);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}.000Z")
}

#[cfg(test)]
mod tests {
    #[test]
    fn timestamp_matches_dart() {
        assert_eq!(super::format_timestamp(0), "1970-01-01 00:00:00.000Z");
        assert_eq!(super::format_timestamp(1_700_000_000), "2023-11-14 22:13:20.000Z");
    }

    #[test]
    fn tag_suggestion_parses() {
        let tags = super::tag_suggestion(r#"{"tags":{"1":{"ns":"female","tn":"nakadashi"}}}"#);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].namespace, "female");
        assert_eq!(tags[0].key, "nakadashi");
    }

    #[test]
    fn metadata_json_parses() {
        let json = r#"{"gmetadata":[{"gid":123,"token":"aaaaaaaaaa","title":"t","title_jpn":"","category":"Misc","thumb":"https://x/1.jpg","filecount":"10","rating":"4.5","uploader":"u","posted":"1700000000","expunged":false,"filesize":2097152,"torrentcount":"1","tags":["female:sole female","language:english"]}]}"#;
        let metas = super::gallery_metadata(json).unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].page_count, 10);
        assert_eq!(metas[0].size, "2.00MB");
        assert_eq!(metas[0].language, "english");
        assert_eq!(metas[0].tags.len(), 2);
    }
}
