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
pub trait GalleryListSource {
    fn name(&self) -> &'static str;
    /// 页面 URL（首页/热门/排行/收藏…）；空串表示数据来自本地
    fn url(&self, page_index: i64) -> String;
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

    fn url(&self, page_index: i64) -> String {
        match self {
            Self::Front => {
                if page_index == 0 {
                    consts::EH_INDEX.to_string()
                } else {
                    format!("{}?page={page_index}", consts::EH_INDEX)
                }
            }
            Self::Popular => consts::EH_POPULAR.to_string(),
            Self::Ranklist => format!("{}?tl=15&p={page_index}", consts::EH_RANKLIST),
            Self::Favorite => {
                if page_index == 0 {
                    consts::EH_FAVORITE.to_string()
                } else {
                    format!("{}?page={page_index}", consts::EH_FAVORITE)
                }
            }
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

/// 把解码后的位图转成 Slint 图片
pub fn to_slint_image(image: jhentai_core::image::RgbaImage) -> slint::Image {
    slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &image.pixels,
        image.width,
        image.height,
    ))
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

/// Rust 侧 controller：持有 Slint window 与网络状态
pub struct AppController {
    window: AppWindow,
    net: Arc<Mutex<NetState>>,
    client: Client,
    resolver: FrontingResolver,
    state: Arc<Mutex<ListState>>,
    /// 页栈：替代 GetX 的 57 条路由
    pub nav: std::cell::RefCell<crate::nav::Navigator>,
    /// 有新列表需要加载封面时置位，由 UI 线程上的定时器消费
    covers_wanted: Arc<std::sync::atomic::AtomicBool>,
    covers_timer: slint::Timer,
    runtime: tokio::runtime::Runtime,
}

impl AppController {
    pub fn new(options: ClientOptions) -> Result<Self, String> {
        let window = AppWindow::new().map_err(|e| e.to_string())?;
        let net = NetState::shared(options);
        let resolver = FrontingResolver::new();
        let client = {
            let guard = net.lock().map_err(|e| e.to_string())?;
            build_client(&guard, &resolver).map_err(|e| e.to_string())?
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .map_err(|e| e.to_string())?;

        Ok(Self {
            window,
            net,
            client,
            resolver,
            state: Arc::new(Mutex::new(ListState::default())),
            nav: std::cell::RefCell::new(crate::nav::Navigator::new()),
            covers_wanted: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            covers_timer: slint::Timer::default(),
            runtime,
        })
    }

    pub fn window(&self) -> &AppWindow {
        &self.window
    }

    /// 绑定 UI 回调。网络结果用 `invoke_from_event_loop` 回推 UI 线程。
    pub fn wire_callbacks(self: &Rc<Self>) {
        // 封面加载泵：UI 线程上轮询标志位，Rc 不跨线程
        {
            let weak = Rc::downgrade(self);
            self.covers_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(150), move || {
                if let Some(this) = weak.upgrade() {
                    if this.covers_wanted.swap(false, std::sync::atomic::Ordering::Relaxed) {
                        this.load_covers();
                    }
                }
            });
        }

        // 点击卡片压栈到详情页（M4 接详情 UI，这里先保证交互不静默）
        let weak = Rc::downgrade(self);
        self.window.on_open_gallery(move |index| {
            if let Some(this) = weak.upgrade() {
                let page = this.state.lock().map(|g| g.galleries.get(index as usize).map(|gallery| (gallery.gid(), gallery.token().to_string()))).ok().flatten();
                if let Some((gid, token)) = page {
                    this.nav.borrow_mut().push(crate::nav::Page::Details { gid, token });
                }
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_refresh(move || {
            if let Some(this) = weak.upgrade() {
                let page = this.state.lock().unwrap().page_index;
                this.load_page(page);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_next_page(move || {
            if let Some(this) = weak.upgrade() {
                let page = this.state.lock().unwrap().page_index + 1;
                this.load_page(page);
            }
        });

        let weak = Rc::downgrade(self);
        self.window.on_prev_page(move || {
            if let Some(this) = weak.upgrade() {
                let page = (this.state.lock().unwrap().page_index - 1).max(0);
                this.load_page(page);
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
                }
                this.load_page(0);
            }
        });
    }

    /// 加载某一页（异步，结果回 UI 线程）
    pub fn load_page(self: &Rc<Self>, page_index: i64) {
        let tab = self.state.lock().map(|g| g.tab).unwrap_or_default();
        let url = tab.url(page_index);
        if url.is_empty() {
            self.window.set_galleries(Rc::new(slint::VecModel::from(Vec::<GalleryItem>::new())).into());
            self.window.set_error_text("History comes from the local database (M2)".into());
            self.window.set_loading(false);
            return;
        }

        self.window.set_loading(true);
        self.window.set_error_text("".into());

        let net = self.net.clone();
        let client = self.client.clone();
        let resolver = self.resolver.clone();
        let state = self.state.clone();
        let window_weak = self.window.as_weak();
        let covers_wanted = self.covers_wanted.clone();

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
                        let (can_next, can_prev) = {
                            let mut guard = state.lock().unwrap();
                            guard.page_index = page_index;
                            guard.covers.clear();
                            apply_page(&mut guard, page);
                            (guard.next_gid.is_some(), guard.prev_gid.is_some() || page_index > 0)
                        };
                        window.set_galleries(Rc::new(slint::VecModel::from(items)).into());
                        window.set_page_index(page_index as i32);
                        window.set_can_go_next(can_next);
                        window.set_can_go_prev(can_prev);
                        window.set_total_text(format!("{count} items").into());

                        // 封面加载由 UI 线程上的定时器接手（Rc 不能跨线程）
                        covers_wanted.store(true, std::sync::atomic::Ordering::Relaxed);
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
        const MAX_WIDTH: u32 = 128;

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

        for (index, _gid, url) in pending {
            if url.is_empty() {
                continue;
            }
            let net = self.net.clone();
            let client = self.client.clone();
            let resolver = self.resolver.clone();
            let window_weak = self.window.as_weak();

            self.runtime.spawn(async move {
                let Ok((status, bytes)) = get_bytes(&client, &net, &resolver, &url).await else {
                    return;
                };
                if status != 200 {
                    return;
                }
                // 解码放在阻塞线程池，避免卡住 UI 之外的 async worker
                let decoded = tokio::task::spawn_blocking(move || jhentai_core::image::decode(&bytes, MAX_WIDTH)).await.ok().and_then(|r| r.ok());
                let Some(decoded) = decoded else {
                    return;
                };

                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    let image = to_slint_image(decoded);
                    // 回填：索引不变（该批加载期间列表不重排）
                    let values = window.get_galleries();
                    if let Some(row) = Model::row_data(&values, index) {
                        let mut updated = row;
                        updated.cover = image;
                        Model::set_row_data(&values, index, updated);
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
        assert_eq!(Tab::Front.url(0), "https://e-hentai.org");
        assert_eq!(Tab::Front.url(2), "https://e-hentai.org?page=2");
        assert_eq!(Tab::Ranklist.url(1), "https://e-hentai.org/toplist.php?tl=15&p=1");
        assert!(Tab::History.url(0).is_empty());
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
