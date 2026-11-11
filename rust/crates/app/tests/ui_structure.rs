//! `ui/app.slint` 结构守卫。
//!
//! Slint 缺一整个页面块**不会**产生编译错误（第 20 轮就因此丢掉过详情页），
//! 所以用测试把「每个页面恰好一块、关键属性仍被引用」钉死。

fn ui_source() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/ui/app.slint")).expect("读取 app.slint")
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

#[test]
fn each_page_block_exists() {
    let src = ui_source();
    // 页面块之外还有同页面条件的零散控件（顶栏按钮等），所以只要求「至少一块」；
    // 整块无重复由 no_duplicated_blocks 的注释标记保证。
    for page in 0..=4 {
        let marker = format!("if page == {page} ");
        assert!(count(&src, &marker) >= 1, "缺少 page {page} 的块");
    }
}

#[test]
fn page_specific_widgets_are_still_wired() {
    let src = ui_source();
    for needle in [
        "tab-titles[current-tab]",
        "search-text;",
        "detail-title;",
        "reader-current;",
        "downloads : Rectangle",
        "setting-is-ex",
        "AboutSlint",
        "save-image()",
        "share-image()",
        "open-settings()",
        "open-downloads()",
    ] {
        assert!(src.contains(needle), "app.slint 缺少接线：{needle}");
    }
}

#[test]
fn no_duplicated_blocks() {
    let src = ui_source();
    assert_eq!(count(&src, "// 设置页（page 4）"), 1, "设置页块重复");
    assert_eq!(count(&src, "// 下载页（page 3）"), 1, "下载页块重复");
    assert_eq!(count(&src, "// 阅读器（page 2）"), 1, "阅读器块重复");
    assert_eq!(count(&src, "// 详情页（page 1）"), 1, "详情页块重复");
    assert_eq!(count(&src, "{"), count(&src, "}"), "app.slint 大括号不配平");
}
