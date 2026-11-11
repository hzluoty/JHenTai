//! 画廊列表页（首页/热门/排行/收藏/关注/历史共用一个 controller）。
//!
//! 对应 Dart 的 `BasePageLogic`：6 个列表页只实现各自的数据源。
//! 数据源抽象成 `GalleryListSource`，`AppController` 负责把它接到 Slint。

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Model};

use jhentai_core::model::GalleryPageInfo;
use jhentai_core::net::Client;
use jhentai_core::net::*;
use jhentai_core::parser;

use crate::{AppWindow, GalleryItem};

/// 列表页的数据源
/// 翻页游标。E-H 的首页/收藏页只认 `next=<gid>` / `prev=<gid>`，
/// 排行页用 `p=<index>`，热门页不可翻页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageCursor {
    First,
    Next(String),
    Prev(String),
    Index(i64),
}

pub trait GalleryListSource {
    fn name(&self) -> &'static str;
    /// 页面 URL；空串表示数据来自本地（历史页）
    fn url(&self, cursor: &PageCursor) -> String;

    /// 站点选择（EH / EX）——与 core 的 `SearchType::base_url(is_ex)` 同构
    fn url_on(&self, cursor: &PageCursor, is_ex: bool) -> String {
        let url = self.url(cursor);
        if is_ex {
            url.replacen(jhentai_core::net::consts::EH_INDEX, jhentai_core::net::consts::EX_INDEX, 1)
        } else {
            url
        }
    }
}

/// 首页 / 热门 / 排行 / 收藏 / 关注 / 历史
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Front,
    Popular,
    Ranklist,
    Favorite,
    Watched,
    History,
}

impl Tab {
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Front,
            1 => Self::Popular,
            2 => Self::Ranklist,
            3 => Self::Favorite,
            4 => Self::Watched,
            _ => Self::History,
        }
    }

    pub fn all() -> [Self; 6] {
        [Self::Front, Self::Popular, Self::Ranklist, Self::Favorite, Self::Watched, Self::History]
    }
}

impl GalleryListSource for Tab {
    fn name(&self) -> &'static str {
        match self {
            Self::Front => "Front Page",
            Self::Popular => "Popular",
            Self::Ranklist => "Ranklist",
            Self::Favorite => "Favorites",
            Self::Watched => "Watched",
            Self::History => "History",
        }
    }

    fn url(&self, cursor: &PageCursor) -> String {
        let suffix = match cursor {
            PageCursor::First => String::new(),
            PageCursor::Next(gid) => format!("?next={gid}"),
            PageCursor::Prev(gid) => format!("?prev={gid}"),
            PageCursor::Index(index) => format!("?p={index}"),
        };
        match self {
            Self::Front => format!("{}{suffix}", consts::EH_INDEX),
            Self::Popular => consts::EH_POPULAR.to_string(),
            Self::Ranklist => match cursor {
                PageCursor::Index(index) => format!("{}?tl=15&p={index}", consts::EH_RANKLIST),
                _ => format!("{}?tl=15", consts::EH_RANKLIST),
            },
            Self::Favorite => format!("{}{suffix}", consts::EH_FAVORITE),
            Self::Watched => consts::EH_WATCHED.to_string(),
            // 历史页读本地数据库
            Self::History => String::new(),
        }
    }
}

/// 分类配色（与 `ui_config.dart` 的 `galleryCategoryColor` 一致）
pub fn category_color(category: &str) -> slint::Color {
    let (r, g, b) = match category {
        "Doujinshi" => (0xfc, 0x4e, 0x4e),
        "Manga" => (0xfc, 0xb4, 0x17),
        "Image Set" => (0x50, 0x50, 0xd7),
        "Game CG" => (0x05, 0xbf, 0x0b),
        "Artist CG" => (0xdd, 0xe5, 0x00),
        "Cosplay" => (0x97, 0x55, 0xf5),
        "Non-H" => (0x68, 0xc9, 0xde),
        "Asian Porn" => (0xfe, 0x93, 0xff),
        "Western" => (0x14, 0xe7, 0x23),
        "Misc" => (0x9e, 0x9e, 0x9e),
        "Private" => (0x00, 0x00, 0x00),
        _ => (0x9e, 0x9e, 0x9e),
    };
    slint::Color::from_rgb_u8(r, g, b)
}

pub fn gallery_to_item(gallery: &jhentai_core::model::Gallery) -> GalleryItem {
    GalleryItem {
        gid: gallery.gid() as i32,
        title: gallery.title.clone().into(),
        category: gallery.category.clone().into(),
        // 封面由 Rust 侧按可见范围异步加载后回填
        cover: slint::Image::default(),
        page_count: gallery.page_count.unwrap_or(0) as i32,
        rating: gallery.rating as f32,
        uploader: gallery.uploader.clone().unwrap_or_default().into(),
        publish_time: gallery.publish_time.clone().into(),
        language: gallery.language.clone().unwrap_or_default().into(),
        favorite_color: category_color(&gallery.category),
        has_rated: gallery.has_rated,
    }
}

/// 列表页状态
#[derive(Debug, Default)]
pub struct ListState {
    pub tab: Tab,
    pub page_index: i64,
    pub prev_gid: Option<String>,
    pub next_gid: Option<String>,
    pub total: Option<String>,
    pub galleries: Vec<jhentai_core::model::Gallery>,
    /// 已请求过封面的 gid（图片本身只存在 Slint 模型里，`slint::Image` 不是 Send）
    pub covers: std::collections::HashSet<i64>,
}

/// 把解析结果写进状态（纯函数，便于测试）
pub fn apply_page(state: &mut ListState, page: GalleryPageInfo) {
    state.prev_gid = page.prev_gid;
    state.next_gid = page.next_gid;
    state.total = page.total_count.as_ref().and_then(|c| c.count.clone());
    state.galleries = page.galleries;
}

/// Rust 侧 controller：组合根（窗口 + 网络 + 页栈 + 各页状态）。
///
/// 字段 → owner 模块（新增字段前先看这里，避免继续线性膨胀）：
///
/// | 字段 | owner |
/// |---|---|
/// | `window` | `ui/app.slint` 全部页面 |
/// | `net` / `client` / `resolver` | `core::net`，`pages/settings.rs` 负责热重建 |
/// | `state` | `pages/gallery_list.rs`（列表页） |
/// | `details` / `details_thumbs_*` | `pages/details.rs` |
/// | `reader` / `reader_images` | `pages/reader.rs` |
/// | `downloads` | `pages/downloads.rs` |
/// | `settings` / `platform` / `store` / `store_is_persistent` | `pages/settings.rs` + `store.rs` |
/// | `nav` | `nav.rs` |
/// | `ui_events` / `ui_pump_timer` | `pages/pump.rs` |
/// | `runtime` | 全局 tokio |
///
/// 下一步（M6 剩余）引入云同步/标签翻译时，应把各自状态收进独立 struct 并挂这里。
pub struct AppController {
    pub(crate) window: AppWindow,
    pub(crate) net: Arc<Mutex<NetState>>,
    pub(crate) client: Arc<Mutex<Client>>,
    pub(crate) resolver: FrontingResolver,
    /// 列表页状态
    pub(crate) state: Arc<Mutex<ListState>>,
    /// 详情页状态
    pub(crate) details: Arc<Mutex<crate::pages::details::DetailsState>>,
    /// 页栈：替代 GetX 的 57 条路由
    pub nav: std::cell::RefCell<crate::nav::Navigator>,
    /// 跨线程 UI 事件（封面 / 详情缩略图）
    pub(crate) ui_events: Arc<crate::pages::pump::UiEvents>,
    /// UI 事件泵（页面级定时器都走它）
    pub(crate) ui_pump_timer: slint::Timer,
    /// 平台能力（屏幕常亮/权限/分享等）
    pub(crate) platform: Arc<dyn jhentai_platform::Platform>,
    /// 设置（UI 线程；持久化走 store）
    pub(crate) settings: std::cell::RefCell<crate::pages::settings::SettingsState>,
    /// 下载页状态（跨线程：只装 Send 数据）
    pub(crate) downloads: Arc<Mutex<crate::pages::downloads::DownloadsState>>,
    /// 阅读器状态（跨线程，只装 Send 数据）
    pub(crate) reader: Arc<Mutex<crate::pages::reader::ReaderState>>,
    /// 阅读器已解码的图片（只在 UI 线程读写）
    pub(crate) reader_images: Rc<std::cell::RefCell<crate::pages::reader::ReaderImages>>,
    /// 设置存储（`local_config`）；打不开时降级为内存
    pub(crate) store: Arc<Mutex<Box<dyn jhentai_core::config::ConfigStore + Send>>>,
    /// false 表示设置/阅读进度不会落盘（关于页要提示）
    pub store_is_persistent: bool,
    pub(crate) runtime: tokio::runtime::Runtime,
}

impl AppController {
    /// `data_dir`：`getExternalFilesDir(null)`，里面是 `db.sqlite`
    pub fn new(
        options: ClientOptions,
        data_dir: Option<std::path::PathBuf>,
        platform: Arc<dyn jhentai_platform::Platform>,
    ) -> Result<Self, String> {
        let window = AppWindow::new().map_err(|e| e.to_string())?;
        let opened = crate::store::open(data_dir.as_deref());
        // 下载目录与 Dart 版一致：<externalFilesDir>/download
        let download_dir = data_dir.clone().unwrap_or_else(|| std::path::PathBuf::from("download")).join("download");
        let net = NetState::shared(options);
        let (resolver, client) = {
            let guard = net.lock().map_err(|e| e.to_string())?;
            let resolver = FrontingResolver::with_provider(guard.ip_provider.clone());
            let client = build_client(&guard, &resolver).map_err(|e| e.to_string())?;
            (resolver, client)
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            window,
            net,
            client: Arc::new(Mutex::new(client)),
            resolver,
            state: Arc::new(Mutex::new(ListState::default())),
            details: Arc::new(Mutex::new(crate::pages::details::DetailsState::default())),
            nav: std::cell::RefCell::new(crate::nav::Navigator::new()),
            ui_events: Arc::new(crate::pages::pump::UiEvents::default()),
            ui_pump_timer: slint::Timer::default(),
            platform,
            settings: std::cell::RefCell::new(crate::pages::settings::SettingsState::default()),
            downloads: Arc::new(Mutex::new(crate::pages::downloads::DownloadsState {
                download_path: download_dir.clone(),
                concurrency: 3,
                ..Default::default()
            })),
            reader: Arc::new(Mutex::new(crate::pages::reader::ReaderState::default())),
            reader_images: Rc::new(std::cell::RefCell::new(crate::pages::reader::ReaderImages::default())),
            store: Arc::new(Mutex::new(opened.store)),
            store_is_persistent: opened.persistent,
            runtime,
        })
    }

    pub fn window(&self) -> &AppWindow {
        &self.window
    }

    /// 绑定 UI 回调。网络结果用 `invoke_from_event_loop` 回推 UI 线程。
    pub fn wire_callbacks(self: &Rc<Self>) {
        crate::pages::pump::start(&self.ui_pump_timer, self, std::time::Duration::from_millis(120));

        // 点击卡片压栈到详情页（M4 接详情 UI，这里先保证交互不静默）
        let weak = Rc::downgrade(self);
        self.window.on_open_gallery(move |index| {
            if let Some(this) = weak.upgrade() {
                let target = this
                    .state
                    .lock()
                    .ok()
                    .and_then(|g| g.galleries.get(index as usize).map(|gallery| (gallery.gid(), gallery.token().to_string(), gallery.gallery_url.is_eh)));
                if let Some((gid, token, is_eh)) = target {
                    this.open_details(gid, token, !is_eh);
                }
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_reader_step(move |delta| {
            if let Some(this) = weak.upgrade() {
                this.reader_step(delta as i64);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_open_settings(move || {
            if let Some(this) = weak.upgrade() {
                let page = crate::nav::Page::Settings;
                this.nav.borrow_mut().push(page.clone());
                this.show_page(&page);
                this.apply_settings_to_ui();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_cycle_language(move || {
            if let Some(this) = weak.upgrade() {
                this.cycle_language();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_set_account_user(move |text| {
            if let Some(this) = weak.upgrade() {
                this.window.set_account_user(text);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_set_account_password(move |text| {
            if let Some(this) = weak.upgrade() {
                this.window.set_account_password(text);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_login(move |user, password| {
            if let Some(this) = weak.upgrade() {
                this.login(user.to_string(), password.to_string());
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_logout(move || {
            if let Some(this) = weak.upgrade() {
                this.logout();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_import_cookies(move |raw| {
            if let Some(this) = weak.upgrade() {
                this.import_cookies(raw.to_string());
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_toggle_site(move || {
            if let Some(this) = weak.upgrade() {
                this.toggle_site();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_set_proxy(move |address| {
            if let Some(this) = weak.upgrade() {
                this.set_proxy(address.to_string());
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_toggle_domain_fronting(move || {
            if let Some(this) = weak.upgrade() {
                this.toggle_domain_fronting();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_cycle_read_direction(move || {
            if let Some(this) = weak.upgrade() {
                this.cycle_read_direction();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_toggle_keep_awake(move || {
            if let Some(this) = weak.upgrade() {
                this.toggle_keep_awake();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_bump_download_concurrency(move || {
            if let Some(this) = weak.upgrade() {
                this.bump_download_concurrency();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_toggle_download_original(move || {
            if let Some(this) = weak.upgrade() {
                this.toggle_download_original();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_set_pin(move |pin| {
            if let Some(this) = weak.upgrade() {
                let pin = pin.trim().to_string();
                this.set_pin(if pin.is_empty() { None } else { Some(pin) });
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_unlock(move |input| {
            if let Some(this) = weak.upgrade() {
                this.unlock(input.as_str());
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_download(move || {
            if let Some(this) = weak.upgrade() {
                this.start_download();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_open_downloads(move || {
            if let Some(this) = weak.upgrade() {
                this.scan_downloads();
                let page = crate::nav::Page::Downloads;
                this.nav.borrow_mut().push(page.clone());
                this.show_page(&page);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_open_local_gallery(move |gid| {
            if let Some(this) = weak.upgrade() {
                this.open_local_gallery(gid as i64);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_open_reader(move || {
            if let Some(this) = weak.upgrade() {
                this.open_reader();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_back(move || {
            if let Some(this) = weak.upgrade() {
                this.go_back();
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_refresh(move || {
            if let Some(this) = weak.upgrade() {
                this.load_first_page();
            }
        });

        // 翻页用 E-H 返回的 next/prev gid；排行页没有 gid，退回页码
        let weak = Rc::downgrade(self);
        self.window.on_next_page(move || {
            if let Some(this) = weak.upgrade() {
                let cursor = {
                    let guard = this.state.lock().unwrap();
                    match &guard.next_gid {
                        Some(gid) => PageCursor::Next(gid.clone()),
                        None => PageCursor::Index(guard.page_index + 1),
                    }
                };
                this.load(cursor);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_prev_page(move || {
            if let Some(this) = weak.upgrade() {
                let cursor = {
                    let guard = this.state.lock().unwrap();
                    match &guard.prev_gid {
                        Some(gid) => PageCursor::Prev(gid.clone()),
                        None => PageCursor::Index((guard.page_index - 1).max(0)),
                    }
                };
                this.load(cursor);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_select_tab(move |index| {
            if let Some(this) = weak.upgrade() {
                this.window.set_current_tab(index);
                {
                    let mut guard = this.state.lock().unwrap();
                    guard.tab = Tab::from_index(index);
                    guard.page_index = 0;
                    guard.prev_gid = None;
                    guard.next_gid = None;
                    guard.covers.clear();
                }
                this.load_first_page();
            }
        });
    }

    /// 加载第一页
    pub fn load_first_page(self: &Rc<Self>) {
        self.load(PageCursor::First);
    }

    /// 按游标加载（异步，结果回 UI 线程）
    pub fn load(self: &Rc<Self>, cursor: PageCursor) {
        let tab = self.state.lock().map(|g| g.tab).unwrap_or_default();
        // 站点设置：选了 EX 就用 EX 域名（cookie 里没 igneous 时 EX 会是空白页）
        let want_ex = self.settings.borrow().eh.is_ex();
        // 没登录就用 EX 会拿到空白页（缺 igneous）：退回 EH 并提示
        let logged_in = self.net.lock().map(|n| n.cookies.has_logged_in()).unwrap_or(false);
        let is_ex = want_ex && logged_in;
        if want_ex && !logged_in {
            self.window.set_error_text(crate::i18n::t(self.settings.borrow().lang, "exNeedsLogin").into());
        }
        let url = tab.url_on(&cursor, is_ex);
        if url.is_empty() {
            self.window.set_galleries(Rc::new(slint::VecModel::from(Vec::<GalleryItem>::new())).into());
            self.window.set_error_text("History comes from the local database (M2)".into());
            self.window.set_loading(false);
            return;
        }

        self.window.set_loading(true);
        self.window.set_error_text("".into());

        let net = self.net.clone();
        let client = self.client.lock().unwrap().clone();
        let resolver = self.resolver.clone();
        let state = self.state.clone();
        let window_weak = self.window.as_weak();
        let ui_events = self.ui_events.clone();
        let cursor_for_state = cursor.clone();

        self.runtime.spawn(async move {
            let parsed = match get(&client, &net, &resolver, &url).await {
                Ok(response) => parser::gallery_page(&response.body).map_err(RequestError::Parse),
                Err(e) => Err(e),
            };

            let _ = slint::invoke_from_event_loop(move || {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                window.set_loading(false);
                match parsed {
                    Ok(page) => {
                        let items: Vec<GalleryItem> = page.galleries.iter().map(gallery_to_item).collect();
                        let count = items.len();
                        let (can_next, can_prev, shown_index) = {
                            let mut guard = state.lock().unwrap();
                            // 游标翻页没有页码，用 +1 近似（只影响页码显示）
                            guard.page_index = match &cursor_for_state {
                                PageCursor::Index(index) => *index,
                                PageCursor::First => 0,
                                _ => guard.page_index + 1,
                            };
                            guard.covers.clear();
                            apply_page(&mut guard, page);
                            (guard.next_gid.is_some(), guard.prev_gid.is_some() || guard.page_index > 0, guard.page_index)
                        };
                        window.set_galleries(Rc::new(slint::VecModel::from(items)).into());
                        window.set_page_index(shown_index as i32);
                        window.set_can_go_next(can_next);
                        window.set_can_go_prev(can_prev);
                        window.set_total_text(format!("{count} items").into());

                        // 封面加载由 UI 线程上的定时器接手（Rc 不能跨线程）
                        ui_events.request_covers();
                    }
                    Err(e) => {
                        window.set_galleries(Rc::new(slint::VecModel::from(Vec::<GalleryItem>::new())).into());
                        window.set_error_text(format!("{e}").into());
                    }
                }
            });
        });
    }

    /// 为当前列表里还没有封面的条目下载并解码缩略图，逐行回填到模型。
    ///
    /// 只按可见数量（前 `COVER_BATCH` 条）请求，滚动到底再触发下一批；
    /// 解码在 tokio 线程池，`slint::Image` 只在 UI 线程构造。
    pub fn load_covers(self: &Rc<Self>) {
        const COVER_BATCH: usize = 24;
        const MAX_WIDTH: u32 = crate::pages::cover_loader::COVER_WIDTH;

        let pending: Vec<(usize, i64, String)> = {
            let mut guard = self.state.lock().unwrap();
            let picked: Vec<(usize, i64, String)> = guard
                .galleries
                .iter()
                .enumerate()
                .filter(|(_, g)| !guard.covers.contains(&g.gid()))
                .take(COVER_BATCH)
                .map(|(index, g)| (index, g.gid(), g.cover.url.clone()))
                .collect();
            for (_, gid, _) in &picked {
                guard.covers.insert(*gid);
            }
            picked
        };
        if pending.is_empty() {
            return;
        }

        for (_index, gid, url) in pending {
            if url.is_empty() {
                continue;
            }
            let net = self.net.clone();
            let client = self.client.lock().unwrap().clone();
            let resolver = self.resolver.clone();
            let state = self.state.clone();
            let window_weak = self.window.as_weak();

            self.runtime.spawn(async move {
                // 失败要回滚标记，否则这一条永远不再重试
                let failure = |state: &Arc<Mutex<ListState>>| {
                    state.lock().unwrap().covers.remove(&gid);
                };

                let Some(decoded) =
                    crate::pages::cover_loader::fetch_image(&client, &net, &resolver, &url, MAX_WIDTH).await
                else {
                    failure(&state);
                    return;
                };

                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    let image = crate::pages::cover_loader::to_slint_image(decoded);
                    // 回填：按 gid 找行，切页/切 tab 后旧结果不会画到错误的行上
                    let values = window.get_galleries();
                    let target = (0..Model::row_count(&values))
                        .find(|row| Model::row_data(&values, *row).map(|item| item.gid).unwrap_or_default() == gid as i32);
                    if let Some(row) = target {
                        if let Some(mut updated) = Model::row_data(&values, row) {
                            updated.cover = image;
                            Model::set_row_data(&values, row, updated);
                        }
                    }
                });
            });
        }
    }

    pub fn run(&self) -> Result<(), slint::PlatformError> {
        self.window.run()
    }

    pub fn show(&self) -> Result<(), slint::PlatformError> {
        self.window.show()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_urls() {
        assert_eq!(Tab::Front.url(&PageCursor::First), "https://e-hentai.org");
        assert_eq!(Tab::Front.url(&PageCursor::Next("1234".into())), "https://e-hentai.org?next=1234");
        assert_eq!(Tab::Front.url(&PageCursor::Prev("99".into())), "https://e-hentai.org?prev=99");
        assert_eq!(Tab::Front.url(&PageCursor::Index(2)), "https://e-hentai.org?p=2");
        assert_eq!(Tab::Ranklist.url(&PageCursor::Index(1)), "https://e-hentai.org/toplist.php?tl=15&p=1");
        assert_eq!(Tab::Popular.url(&PageCursor::Next("1".into())), "https://e-hentai.org/popular", "热门页不可翻页");
        assert!(Tab::History.url(&PageCursor::First).is_empty());
    }

    #[test]
    fn tab_from_index_roundtrip() {
        for (i, tab) in Tab::all().iter().enumerate() {
            assert_eq!(Tab::from_index(i as i32), *tab);
        }
    }

    #[test]
    fn category_colors_match_dart() {
        let misc = category_color("Misc");
        assert_eq!((misc.red(), misc.green(), misc.blue()), (158, 158, 158));
        let unknown = category_color("Other");
        assert_eq!((unknown.red(), unknown.green(), unknown.blue()), (158, 158, 158));
    }

    #[test]
    fn apply_page_copies_navigation() {
        let mut state = ListState::default();
        let page = GalleryPageInfo { prev_gid: Some("1".into()), next_gid: Some("2".into()), ..Default::default() };
        apply_page(&mut state, page);
        assert_eq!(state.prev_gid.as_deref(), Some("1"));
        assert_eq!(state.next_gid.as_deref(), Some("2"));
        assert!(state.galleries.is_empty());
    }
}
