//! 阅读链路端到端测试（真实站点）：首页 → 详情 → 缩略图 → 图片页 → 原图 → 解码。
//!
//! 默认忽略；`HTTPS_PROXY=... cargo test -p jhentai-core --test live_reader -- --ignored`

use std::sync::{Arc, Mutex};

use jhentai_core::net::*;
use jhentai_core::service::reader;

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
async fn full_read_pipeline() {
    let state: Arc<Mutex<NetState>> = NetState::shared(options());
    let resolver = FrontingResolver::new();
    let client = build_client(&state.lock().unwrap(), &resolver).unwrap();

    // 1. 首页 → 第一个画廊
    let front = get(&client, &state, &resolver, consts::EH_INDEX).await.unwrap();
    let page = jhentai_core::parser::gallery_page(&front.body).unwrap();
    // 优先挑一个多页画廊，顺带覆盖缩略图分页抓取
    let gallery = page
        .galleries
        .iter()
        .find(|g| g.page_count.unwrap_or(0) > 40)
        .or_else(|| page.galleries.first())
        .expect("首页应有画廊");

    // 2. 详情 → 缩略图
    let detail_html = get(&client, &state, &resolver, &gallery.gallery_url.url()).await.unwrap();
    let detail = jhentai_core::parser::detail_page(&detail_html.body).unwrap().detail;
    let thumb = detail.thumbnails.first().expect("详情页应有缩略图");

    // 3. 图片页 → 原图 URL
    let page_url = reader::absolute_page_url(&thumb.href, true);
    let image = reader::resolve_image_page(&client, &state, &resolver, &page_url)
        .await
        .expect("图片页应解析成功");
    assert!(image.url.starts_with("http"), "图片直链: {}", image.url);

    // 3.5 多页缩略图：分页器解析 + 逐页抓取（下载/阅读都依赖它）
    if detail.thumbnails_page_count > 1 {
        let all = reader::fetch_all_thumbnails(
            &client,
            &state,
            &resolver,
            &detail.gallery_url,
            &detail.thumbnails,
            detail.thumbnails_page_count,
        )
        .await
        .expect("多页缩略图应能抓全");
        eprintln!(
            "thumbnails: first page {} -> all {} ({} pages)",
            detail.thumbnails.len(),
            all.len(),
            detail.thumbnails_page_count
        );
        assert!(all.len() > detail.thumbnails.len(), "多页画廊应抓到更多缩略图");
        assert_eq!(all.len() as i64, detail.page_count, "缩略图总数应等于页数");
    }

    // 4. 下载 `#img` 直链 + 解码（带 referer）
    let bitmap = reader::load_image_with_referer(&client, &state, &resolver, &image.url, Some(&page_url), 1080)
        .await
        .expect("图片直链应能下载并解码");
    assert!(bitmap.width > 0 && bitmap.height > 0);
    // `fullimg` 原图需要登录，匿名访问会被 302 到登录页；这里只保证直链可用
    if let Some(original) = image.original_image_url {
        let result = reader::load_image_with_referer(&client, &state, &resolver, &original, Some(&page_url), 1080).await;
        eprintln!("original image (fullimg) result: {:?}", result.map(|b| (b.width, b.height)));
    }
    assert_eq!(bitmap.pixels.len(), (bitmap.width * bitmap.height * 4) as usize);
}
