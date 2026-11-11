//! 真实站点冒烟测试。默认忽略；用环境变量开关：
//! `LIVE_EH=1 HTTPS_PROXY=http://127.0.0.1:10808 cargo test --test live_network -- --ignored`

use jhentai_core::net::*;

fn options() -> ClientOptions {
    ClientOptions {
        connect_timeout: Some(std::time::Duration::from_secs(20)),
        receive_timeout: Some(std::time::Duration::from_secs(30)),
        proxy: std::env::var("HTTPS_PROXY").ok(),
        ..Default::default()
    }
}

#[tokio::test]
#[ignore = "需要真实网络"]
async fn fetch_front_page_and_parse() {
    let state = NetState::shared(options());
    let resolver = FrontingResolver::new();
    let client = build_client(&state.lock().unwrap(), &resolver).unwrap();

    let response = get(&client, &state, &resolver, "https://e-hentai.org/").await.unwrap();
    assert_eq!(response.status, 200);
    assert!(response.body.contains("E-Hentai Galleries"));

    let page = jhentai_core::parser::gallery_page(&response.body).unwrap();
    assert!(page.galleries.len() > 10);
    // 首页是 .itg gltc 紧凑视图，标签与封面都要解析出来
    assert!(page.galleries[0].cover.url.starts_with("http"));
}

#[tokio::test]
#[ignore = "需要真实网络"]
async fn detail_page_from_front_page_link() {
    let state = NetState::shared(options());
    let resolver = FrontingResolver::new();
    let client = build_client(&state.lock().unwrap(), &resolver).unwrap();

    let front = get(&client, &state, &resolver, "https://e-hentai.org/").await.unwrap();
    let page = jhentai_core::parser::gallery_page(&front.body).unwrap();
    let url = page.galleries[0].gallery_url.url();

    let detail = get(&client, &state, &resolver, &url).await.unwrap();
    assert_eq!(detail.status, 200);
    let parsed = jhentai_core::parser::detail_page(&detail.body).unwrap();
    assert_eq!(parsed.detail.gallery_url.gid, page.galleries[0].gid());
    assert!(parsed.detail.page_count > 0);
    assert!(!parsed.detail.tags.is_empty());
}
