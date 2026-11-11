//! 搜索页 controller：关键字搜索、搜索历史、清空回到浏览模式。
//!
//! 参数组装在 core（`service::search`），这里只做 UI 状态与历史读写。

use std::rc::Rc;

use jhentai_core::net::GalleryQuery;
use jhentai_core::service::search as search_service;


impl crate::AppController {
    /// 执行关键字搜索（UI 线程）：写入条件、记录历史、刷新列表
    pub fn start_search(self: &Rc<Self>, keyword: String) {
        let keyword = keyword.trim().to_string();
        if keyword.is_empty() {
            return;
        }

        {
            let mut state = self.state.lock().unwrap();
            // 搜索只在「画廊」搜索类型下有效，收藏/关注页不参与
            state.query = Some(GalleryQuery { keyword: Some(keyword.clone()), ..Default::default() });
            state.page_index = 0;
            state.prev_gid = None;
            state.next_gid = None;
            state.covers.clear();
        }

        if let Ok(mut store) = self.store.lock() {
            search_service::add_search_history(store.as_mut(), &keyword);
        }
        self.refresh_search_ui();
        self.load_first_page();
    }

    /// 清空搜索，回到当前 tab 的浏览模式
    pub fn clear_search(self: &Rc<Self>) {
        {
            let mut state = self.state.lock().unwrap();
            state.query = None;
            state.page_index = 0;
            state.prev_gid = None;
            state.next_gid = None;
            state.covers.clear();
        }
        self.window.set_search_text("".into());
        self.refresh_search_ui();
        self.load_first_page();
    }

    /// 点历史记录直接搜索
    pub fn search_history(self: &Rc<Self>, index: i32) {
        let keyword = {
            let state = self.state.lock().unwrap();
            state.history.get(index.max(0) as usize).cloned()
        };
        if let Some(keyword) = keyword {
            self.start_search(keyword);
        }
    }

    /// 把搜索状态（条件、历史）同步到 UI
    pub fn refresh_search_ui(self: &Rc<Self>) {
        let (keyword, history, searching) = {
            let state = self.state.lock().unwrap();
            let keyword = state.query.as_ref().and_then(|q| q.keyword.clone()).unwrap_or_default();
            (keyword, state.history.clone(), state.query.is_some())
        };
        self.window.set_search_text(keyword.into());
        self.window.set_searching(searching);
        self.window.set_search_history(Rc::new(slint::VecModel::from(
            history.into_iter().map(slint::SharedString::from).collect::<Vec<_>>(),
        )).into());
    }

    /// 启动时加载搜索历史
    pub fn load_search_history(self: &Rc<Self>) {
        let history = self
            .store
            .lock()
            .ok()
            .map(|store| search_service::load_search_history(store.as_ref()))
            .unwrap_or_default();
        self.state.lock().unwrap().history = history;
        self.refresh_search_ui();
    }
}

#[cfg(test)]
mod tests {
    use jhentai_core::net::GalleryQuery;

    #[test]
    fn query_keyword_round_trip() {
        let query = GalleryQuery { keyword: Some("azur lane".into()), ..Default::default() };
        let url = jhentai_core::service::search::search_url(
            jhentai_core::service::search::SearchType::Gallery,
            false,
            &query,
            0,
        );
        assert_eq!(url, "https://e-hentai.org/?f_search=azur%20lane");
        let page2 = jhentai_core::service::search::search_url(
            jhentai_core::service::search::SearchType::Gallery,
            false,
            &query,
            1,
        );
        assert!(page2.ends_with("&page=1"), "{page2}");
    }
}
