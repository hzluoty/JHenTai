//! 搜索服务：URL 组装、搜索历史、快速搜索、标签联想。
//!
//! 对应 `service/search_service.dart` 与 `model/search_config.dart` 的查询部分。

use crate::config::{keys, ConfigStore};
use crate::net::{GalleryQuery, RanklistType};
use crate::net::{consts, urlencode};

/// 搜索类型（决定落在哪个页面）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchType {
    Gallery,
    Popular,
    Favorite,
    Watched,
    History,
}

impl SearchType {
    pub fn base_url(&self, is_ex: bool) -> &'static str {
        match self {
            Self::Gallery => {
                if is_ex {
                    consts::EX_INDEX
                } else {
                    consts::EH_INDEX
                }
            }
            Self::Popular => {
                if is_ex {
                    consts::EX_POPULAR
                } else {
                    consts::EH_POPULAR
                }
            }
            Self::Favorite => {
                if is_ex {
                    consts::EX_FAVORITE
                } else {
                    consts::EH_FAVORITE
                }
            }
            Self::Watched => {
                if is_ex {
                    consts::EX_WATCHED
                } else {
                    consts::EH_WATCHED
                }
            }
            Self::History => "",
        }
    }
}

/// 组装搜索 URL（第一页不带 `page` 参数，与 Dart 版一致）
pub fn search_url(search_type: SearchType, is_ex: bool, query: &GalleryQuery, page_index: i64) -> String {
    let base = search_type.base_url(is_ex);
    if base.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    let query_string = query.to_query_string();
    if !query_string.is_empty() {
        parts.push(query_string);
    }
    if page_index > 0 {
        parts.push(format!("page={page_index}"));
    }

    if parts.is_empty() {
        base.to_string()
    } else {
        // E-H 对 `/?f_search=` 才不 302，`?f_search=` 会被重定向
        format!("{}/?{}", base.trim_end_matches('/'), parts.join("&"))
    }
}

/// 排行页 URL
pub fn ranklist_url(is_ex: bool, kind: RanklistType, page_index: i64) -> String {
    format!(
        "{}/toplist.php?tl={}&p={page_index}",
        if is_ex { consts::EX_INDEX } else { consts::EH_INDEX },
        kind.tl()
    )
}

// ---------------------------------------------------------------- 历史 / 快速搜索

/// 搜索历史最多保留条数
pub const MAX_HISTORY: usize = 50;

/// 追加一条搜索历史（最新的在最前，去重）
pub fn add_search_history(store: &mut dyn ConfigStore, keyword: &str) {
    let keyword = keyword.trim();
    if keyword.is_empty() {
        return;
    }
    let mut history = load_search_history(store);
    history.retain(|k| k != keyword);
    history.insert(0, keyword.to_string());
    history.truncate(MAX_HISTORY);
    if let Ok(json) = serde_json::to_string(&history) {
        crate::config::write_string(store, keys::SEARCH_HISTORY, "", &json);
    }
}

pub fn load_search_history(store: &dyn ConfigStore) -> Vec<String> {
    crate::config::read_string(store, keys::SEARCH_HISTORY, "")
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn clear_search_history(store: &mut dyn ConfigStore) {
    crate::config::write_string(store, keys::SEARCH_HISTORY, "", "[]");
}

/// 快速搜索：`(名称, 查询串)` 列表
pub fn load_quick_search(store: &dyn ConfigStore) -> Vec<(String, String)> {
    crate::config::read_string(store, keys::QUICK_SEARCH, "")
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn save_quick_search(store: &mut dyn ConfigStore, items: &[(String, String)]) {
    if let Ok(json) = serde_json::to_string(items) {
        crate::config::write_string(store, keys::QUICK_SEARCH, "", &json);
    }
}

/// 标签联想 URL（E-H 的 tag suggest API）
pub fn tag_suggestion_url(keyword: &str) -> String {
    format!("https://e-hentai.org/tag/suggest.php?q={}", urlencode(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemoryConfigStore;

    #[test]
    fn search_url_matches_dart_shape() {
        let query = GalleryQuery { keyword: Some("azur lane".into()), ..Default::default() };
        assert_eq!(search_url(SearchType::Gallery, false, &query, 0), "https://e-hentai.org/?f_search=azur%20lane");
        assert!(search_url(SearchType::Gallery, true, &query, 3).ends_with("&page=3"));
        assert!(search_url(SearchType::Gallery, true, &query, 0).starts_with("https://exhentai.org"));
        assert_eq!(search_url(SearchType::Popular, false, &GalleryQuery::default(), 0), "https://e-hentai.org/popular");
        assert!(search_url(SearchType::History, false, &query, 0).is_empty());
    }

    #[test]
    fn ranklist_url_uses_tl_codes() {
        assert_eq!(ranklist_url(false, RanklistType::Day, 2), "https://e-hentai.org/toplist.php?tl=15&p=2");
        assert_eq!(ranklist_url(true, RanklistType::AllTime, 0), "https://exhentai.org/toplist.php?tl=11&p=0");
    }

    #[test]
    fn search_history_dedupes_and_caps() {
        let mut store = MemoryConfigStore::new();
        for i in 0..60 {
            add_search_history(&mut store, &format!("kw{i}"));
        }
        add_search_history(&mut store, "kw30");
        let history = load_search_history(&store);
        assert_eq!(history.len(), MAX_HISTORY);
        assert_eq!(history[0], "kw30", "重复关键词提到最前");
        clear_search_history(&mut store);
        assert!(load_search_history(&store).is_empty());
    }

    #[test]
    fn quick_search_roundtrip() {
        let mut store = MemoryConfigStore::new();
        assert!(load_quick_search(&store).is_empty());
        save_quick_search(&mut store, &[("tag A".into(), "f_search=tag+a".into())]);
        assert_eq!(load_quick_search(&store).len(), 1);
    }

    #[test]
    fn tag_suggestion_url_encodes() {
        assert_eq!(tag_suggestion_url("sole female"), "https://e-hentai.org/tag/suggest.php?q=sole%20female");
    }
}
