//! 解析器金样测试：用抓取的真实 E-H 页面验证字段级输出。

use jhentai_core::model::EhTagStatus;
use jhentai_core::parser::*;

fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn compact_list_page() {
    let page = gallery_page(&fixture("list_compact.html")).unwrap();
    assert!(page.galleries.len() >= 20, "galleries: {}", page.galleries.len());

    let first = &page.galleries[0];
    assert_eq!(first.gallery_url.gid, 4321294);
    assert_eq!(first.token(), "b8f1467b0e");
    assert_eq!(first.title, "[猿かん] Nami buggy [AI Generated]");
    assert_eq!(first.category, "Misc");
    assert_eq!(first.page_count, Some(196));
    assert_eq!(first.uploader.as_deref(), Some("Quigor"));
    assert_eq!(first.publish_time, "2026-11-11 05:49");
    assert!(!first.is_expunged);
    assert_eq!(first.cover.height, Some(172.0));
    assert_eq!(first.cover.width, Some(250.0));
    assert!(first.cover.url.starts_with("https://ehgt.org/"));

    // 第二个画廊用的懒加载 data-src
    assert_eq!(page.galleries[1].cover.width, Some(250.0));
    assert!(page.galleries[1].cover.url.starts_with("https://ehgt.org/w/"));

    assert_eq!(first.tags.get("other").map(|v| v[0].tag_data.key.as_str()), Some("ai generated"));
    assert!(page.total_count.is_some());
}

#[test]
fn compact_list_rating_and_thumbnails_page() {
    let page = gallery_page(&fixture("list_compact.html")).unwrap();
    // 主页没有任何已评分星级时 rating 为 5；有偏移时按 16px 一格换算
    assert!(page.galleries.iter().all(|g| (0.0..=5.0).contains(&g.rating)));
    assert!(page.next_gid.is_some() || page.prev_gid.is_some(), "分页 gid 应至少有一个");
}

#[test]
fn toplist_page() {
    let info = ranklist_page(&fixture("toplist.html"));
    assert!(info.galleries.len() >= 20);
    assert!(info.page_count >= 2, "page_count: {}", info.page_count);
    assert_eq!(info.next_page_index, Some(1));
    assert!(info.galleries[0].page_count.is_some(), "排行页也应解析页数");
}

#[test]
fn gallery_detail_page() {
    let result = detail_page(&fixture("gallery_detail.html")).unwrap();
    let d = result.detail;

    assert_eq!(d.gallery_url.gid, 4321294);
    assert_eq!(d.gallery_url.token, "b8f1467b0e");
    assert!(!d.raw_title.is_empty());
    assert_eq!(d.category, "Misc");
    assert_eq!(d.page_count, 196);
    assert_eq!(d.language, "Japanese");
    assert_eq!(d.uploader.as_deref(), Some("uploader_anonymized"));
    assert_eq!(d.cover.width, Some(250.0));
    assert!(d.cover.height.is_some());
    assert!(d.archive_page_url.contains("archiver.php"), "archive: {}", d.archive_page_url);
    assert!(d.torrent_page_url.contains("gallerytorrents.php"), "torrent: {}", d.torrent_page_url);
    assert!(d.size.contains("MiB") || d.size.contains("GiB"), "size: {}", d.size);
    assert!(!d.thumbnails.is_empty());
    assert!(d.thumbnails_page_count >= 1);
    assert!(!d.tags.is_empty(), "详情页应解析出标签");

    // 该 fixture 是 AI 生成画廊，只有 other: 一个命名空间
    assert!(d.tags.get("other").is_some());
    assert_eq!(d.tags.get("other").unwrap()[0].tag_status, Some(EhTagStatus::Confidence));
}

#[test]
fn watched_page_has_no_galleries() {
    let page = gallery_page(&fixture("watched.html")).unwrap();
    assert!(page.galleries.is_empty());
    assert!(page.total_count.is_none());
}

#[test]
fn detail_tag_namespace_and_key() {
    let result = detail_page(&fixture("gallery_detail.html")).unwrap();
    let other = result.detail.tags.get("other").expect("other 命名空间");
    assert_eq!(other[0].tag_data.key, "ai generated");
    assert_eq!(other[0].tag_data.full(), "other:ai generated");
}

#[test]
fn image_page_fields() {
    let image = image_page_image(&fixture("image_page.html")).expect("图片页应解析成功");
    assert!(image.url.starts_with("https://"), "直链: {}", image.url);
    assert!(image.url.contains(".webp") || image.url.contains(".jpg") || image.url.contains(".png"));
    assert_eq!(image.width, Some(800.0));
    assert_eq!(image.height, Some(547.0));
    assert!(image.reload_key.is_some());
    assert_eq!(image.image_hash.as_deref().map(str::len), Some(40), "sha1 hash");
    // fullimg 原图链接（匿名访问会 302 到登录页，仅用于「下载原图」）
    assert!(image.original_image_url.as_deref().unwrap_or_default().contains("/fullimg/"));

    let gallery = image_page_gallery_url(&fixture("image_page.html")).expect("图片页应有画廊链接");
    assert_eq!(gallery.gid, 4321294);
}

#[test]
fn multi_page_detail_fixture_is_parsed_once() {
    // 单页画廊：thumbnails_page_count 为 1，避免阅读器走多页补全分支
    let result = detail_page(&fixture("gallery_detail.html")).unwrap();
    assert_eq!(result.detail.thumbnails_page_count, 1);
    assert!(!result.detail.thumbnails.is_empty());
}
