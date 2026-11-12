//! 搜索链路端到端（真实站点）：搜索 → 解析游标 → 用 next= 翻页 → 结果必须不同。
//!
//! 这条测试专门覆盖「搜索翻页是 gid 制，`page=N` 会被 E-H 忽略」这个坑：
//! 只断言第一页长得对是发现不了的。
//!
//! 默认忽略；`HTTPS_PROXY=... cargo test -p jhentai-core --test live_search -- --ignored`

use std::sync::{Arc, Mutex};

use jhentai_core::net::*;
use jhentai_core::service::search::{search_url, SearchType};

fn options() -> ClientOptions {
    ClientOptions {
        connect_timeout: Some(std::time::Duration::from_secs(20)),
        receive_timeout: Some(std::time::Duration::from_secs(40)),
        proxy: std::env::var("HTTPS_PROXY").ok(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "需要真实网络"]
async fn search_pagination_uses_gid_cursor() {
    let state: Arc<Mutex<NetState>> = NetState::shared(options());
    let resolver = FrontingResolver::new();
    let client = build_client(&state.lock().unwrap(), &resolver).unwrap();

    // 第一页
    let query = GalleryQuery { keyword: Some("azur".into()), ..Default::default() };
    let first_url = search_url(SearchType::Gallery, false, &query, 0);
    assert!(first_url.contains("/?f_search="), "第一页 URL: {first_url}");

    let first = get(&client, &state, &resolver, &first_url).await.unwrap();
    let first_page = jhentai_core::parser::gallery_page(&first.body).unwrap();
    assert!(!first_page.galleries.is_empty(), "搜索应有结果");
    let next_gid = first_page.next_gid.clone().expect("搜索结果页应给出 next gid");
    let first_gid = first_page.galleries[0].gid();

    // 第二页：必须用 next=<gid>，而不是 page=1
    let mut page2_query = query.clone();
    page2_query.next_gid = Some(next_gid.clone());
    let second_url = search_url(SearchType::Gallery, false, &page2_query, 0);
    assert!(second_url.contains(&format!("next={next_gid}")), "第二页 URL: {second_url}");

    let second = get(&client, &state, &resolver, &second_url).await.unwrap();
    let second_page = jhentai_core::parser::gallery_page(&second.body).unwrap();
    assert!(!second_page.galleries.is_empty(), "第二页应有结果");
    assert_ne!(
        second_page.galleries[0].gid(),
        first_gid,
        "gid 游标翻页必须换页（page=N 会被忽略而重载第一页）"
    );

    // 对照：page=N 形式确实会被忽略（同一个第一页）
    let mut page_query = query.clone();
    page_query.page_at_least = None;
    let ignored_url = format!("{first_url}&page=1");
    let ignored = get(&client, &state, &resolver, &ignored_url).await.unwrap();
    let ignored_page = jhentai_core::parser::gallery_page(&ignored.body).unwrap();
    assert_eq!(
        ignored_page.galleries[0].gid(),
        first_gid,
        "E-H 忽略 page 参数，这正是不能用页码翻页的原因"
    );
}

/// 标签联想走 `api.php` 的 JSON POST（form 编码会被拒）
#[tokio::test]
#[ignore = "需要真实网络"]
async fn tag_suggest_returns_tags() {
    let state: Arc<Mutex<NetState>> = NetState::shared(options());
    let resolver = FrontingResolver::new();
    let client = build_client(&state.lock().unwrap(), &resolver).unwrap();

    let tags = jhentai_core::service::search::tag_suggest(&client, &state, &resolver, "sole")
        .await
        .expect("联想接口应返回数据");
    assert!(!tags.is_empty(), "sole 应有联想结果");
    assert!(tags.iter().any(|t| t.namespace == "female" && t.key == "sole female"));
}

/// 标签翻译库能下载并解析（7MB JSON，超时放宽）
#[tokio::test]
#[ignore = "需要真实网络"]
async fn tag_translation_database_downloads() {
    let state: Arc<Mutex<NetState>> = NetState::shared(options());
    let resolver = FrontingResolver::new();
    let client = build_client(&state.lock().unwrap(), &resolver).unwrap();

    let db = jhentai_core::service::tag_translation::fetch(&client, &state, &resolver)
        .await
        .expect("翻译库应能下载");
    assert!(db.namespace_name("female").is_some(), "应有 female 命名空间");
    assert!(db.tag_name("female", "sole female").is_some(), "应有 sole female 的翻译");
    eprintln!(
        "namespaces={} female tags={}",
        db.namespaces.len(),
        db.tags.get("female").map(|m| m.len()).unwrap_or(0)
    );
}

/// 翻译库入库耗时（10 万+ 行必须在事务里，否则模拟器上跑几分钟）
#[tokio::test]
#[ignore = "需要真实网络"]
async fn tag_translation_store_is_fast() {
    let state: Arc<Mutex<NetState>> = NetState::shared(options());
    let resolver = FrontingResolver::new();
    let client = build_client(&state.lock().unwrap(), &resolver).unwrap();
    let db = jhentai_core::service::tag_translation::fetch(&client, &state, &resolver)
        .await
        .expect("翻译库应能下载");

    let mut path = std::env::temp_dir();
    path.push(format!("jhentai-tagstore-{}.sqlite", std::process::id()));
    let conn = jhentai_core::db::open(&path).unwrap();
    let start = std::time::Instant::now();
    jhentai_core::service::tag_translation::store(&conn, &db).expect("入库应成功");
    let elapsed = start.elapsed();

    let count: i64 = conn.query_row("SELECT COUNT(*) FROM tag", [], |r| r.get(0)).unwrap();
    eprintln!("stored {count} tags in {elapsed:?}");
    assert!(count > 40_000, "应写入 4 万+ 行，实际 {count}");
    assert!(elapsed < std::time::Duration::from_secs(20), "入库太慢：{elapsed:?}");

    drop(conn);
    let _ = std::fs::remove_file(path);
}
