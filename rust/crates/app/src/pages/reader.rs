//! 阅读页 controller：图片页解析 → 原图下载 → 解码 → 全屏展示。
//!
//! 对应 Dart `pages/read/read_page_logic.dart`。首版做「横向单页 + 点击翻页 + 预加载 ±1 +
//! 阅读进度入库」；纵向/双页、缩放、音量键、自动翻页按计划 §2.3 后续补。
//!
//! 跨线程约定：解码结果放进 `ReaderState::pending`（`RgbaImage` 是 `Send`），
//! 再用 `ui_events.request_reader()` 通知 UI 线程转换成 `slint::Image`（`!Send`，只能在 UI 线程造）。

use std::collections::HashMap;
use std::rc::Rc;

use jhentai_core::image::RgbaImage;
use slint::ComponentHandle;
use jhentai_core::model::GalleryThumbnail;
use jhentai_core::service::reader;

use crate::pages::cover_loader::to_slint_image;

/// 阅读器状态（跨线程：只装 `Send` 数据）
#[derive(Debug, Default)]
pub struct ReaderState {
    pub gid: i64,
    pub is_ex: bool,
    pub thumbnails: Vec<GalleryThumbnail>,
    /// 每页状态：0 未请求 / 1 加载中 / 2 已解码
    pub status: Vec<u8>,
    pub index: usize,
    /// 解码完成待转成 `slint::Image` 的位图
    pub pending: HashMap<usize, RgbaImage>,
    /// 本地画廊模式：按磁盘路径阅读（已下载）
    pub local_files: Vec<std::path::PathBuf>,
}

impl ReaderState {
    pub fn new(gid: i64, is_ex: bool, thumbnails: Vec<GalleryThumbnail>, start: usize) -> Self {
        let count = thumbnails.len();
        Self {
            gid,
            is_ex,
            thumbnails,
            status: vec![0; count],
            index: start.min(count.saturating_sub(1)),
            pending: HashMap::new(),
            local_files: Vec::new(),
        }
    }

    /// 切到本地画廊（已下载）：用文件列表替代缩略图
    pub fn reset_local(&mut self, gid: i64, files: Vec<std::path::PathBuf>) {
        let start = crate::pages::reader::ReaderState::load_start(gid);
        self.gid = gid;
        self.is_ex = false;
        self.thumbnails = Vec::new();
        self.local_files = files;
        self.status = vec![0; self.local_files.len()];
        self.index = start.min(self.local_files.len().saturating_sub(1));
        self.pending.clear();
    }

    fn load_start(_gid: i64) -> usize {
        0
    }

    pub fn is_local(&self) -> bool {
        !self.local_files.is_empty()
    }

    pub fn len(&self) -> usize {
        if self.is_local() {
            self.local_files.len()
        } else {
            self.thumbnails.len()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.thumbnails.is_empty()
    }
}

/// UI 线程持有的解码后图片（`slint::Image` 不能跨线程）
#[derive(Default)]
pub struct ReaderImages {
    /// 这批图片属于哪个画廊（防串页）
    pub gid: i64,
    pub images: HashMap<usize, slint::Image>,
}

/// 保留窗口：当前页 ± N（解码图很大，长画廊必须淘汰）
pub const CACHE_WINDOW: usize = 2;

impl ReaderImages {
    /// 只保留 `|page - index| <= window` 的条目。
    ///
    /// 窗口 2 = 当前页 + 前后各一页（正好覆盖预加载），再远的重进会重新解码。
    pub fn retain_around(&mut self, index: usize, window: usize) {
        self.images.retain(|page, _| page.abs_diff(index) <= window);
    }

    pub fn len(&self) -> usize {
        self.images.len()
    }

    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

impl crate::AppController {
    /// 从详情页进入阅读器（UI 线程调用），续读位置来自 `local_config`
    pub fn open_reader(self: &Rc<Self>) {
        let (gid, is_ex, thumbnails) = {
            let guard = self.details.lock().unwrap();
            let Some(detail) = guard.detail.as_ref() else {
                return;
            };
            (detail.gallery_url.gid, !detail.gallery_url.is_eh, detail.thumbnails.clone())
        };
        if thumbnails.is_empty() {
            return;
        }

        let start = self
            .store
            .lock()
            .ok()
            .and_then(|store| jhentai_core::service::read_progress::load(store.as_ref(), gid))
            .map(|i| i.max(0) as usize)
            .unwrap_or(0);

        {
            let mut state = self.reader.lock().unwrap();
            *state = ReaderState::new(gid, is_ex, thumbnails, start);
        }
        {
            let mut images = self.reader_images.borrow_mut();
            images.images.clear();
            images.gid = gid;
        }

        // 阅读时按设置保持屏幕常亮
        if self.settings.borrow().read.keep_screen_awake_when_reading {
            self.platform.set_keep_screen_on(true);
        }
        // 自动翻页：进入阅读页重置计时（启用与否由设置页的开关决定）
        *self.auto_turn.lock().unwrap() = Some(std::time::Instant::now());

        let page = crate::nav::Page::Read { gid };
        self.nav.borrow_mut().push(page.clone());
        self.show_page(&page);
        self.show_reader_page();
    }

    /// 渲染当前页并预加载前后各一页
    pub fn show_reader_page(self: &Rc<Self>) {
        let (index, total) = {
            let state = self.reader.lock().unwrap();
            (state.index, state.len())
        };
        self.window.set_reader_index(index as i32);
        self.window.set_reader_total(total as i32);

        let local = self.reader.lock().map(|s| s.is_local()).unwrap_or(false);
        for offset in [-1i64, 0, 1] {
            let target = index as i64 + offset;
            if target >= 0 && (target as usize) < total {
                if local {
                    self.load_local_reader_page(target as usize);
                } else {
                    self.load_reader_page(target as usize);
                }
            }
        }
    }

    /// 加载某一页（图片页 → 原图 → 解码）；已请求/已完成的直接跳过
    pub fn load_reader_page(self: &Rc<Self>, index: usize) {
        let (href, is_ex, gid) = {
            let mut state = self.reader.lock().unwrap();
            if index >= state.len() || state.status[index] != 0 {
                return;
            }
            state.status[index] = 1;
            (state.thumbnails[index].href.clone(), state.is_ex, state.gid)
        };

        let client = self.client.lock().unwrap().clone();
        let net = self.net.clone();
        let resolver = self.resolver.clone();
        let state = self.reader.clone();
        let window_weak = self.window.as_weak();
        let ui_events = self.ui_events.clone();
        let page_url = reader::absolute_page_url(&href, is_ex);

        self.runtime.spawn(async move {
            let image_url = match reader::resolve_image_page(&client, &net, &resolver, &page_url).await {
                // 用 `#img` 的直链（CDN）；`fullimg` 原图需要登录，留给 M5 的「下载原图」
                Ok(image) => image.url,
                Err(e) => {
                    if let Ok(mut guard) = state.lock() {
                        if guard.gid == gid && index < guard.status.len() {
                            guard.status[index] = 0;
                        }
                    }
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = window_weak.upgrade() {
                            window.set_reader_error(format!("{e}").into());
                        }
                    });
                    return;
                }
            };

            // 图床校验 referer，用图片页地址作为来源
            match reader::load_image_with_referer(&client, &net, &resolver, &image_url, Some(&page_url), 1080).await {
                Ok(bitmap) => {
                    let mut guard = state.lock().unwrap();
                    // 用户可能已经换了画廊或列表变短：校验身份+边界再写回
                    if guard.gid != gid || index >= guard.status.len() {
                        return;
                    }
                    guard.status[index] = 2;
                    guard.pending.insert(index, bitmap);
                    drop(guard);
                    // UI 线程把位图转成 slint::Image 并显示
                    ui_events.request_reader();
                }
                Err(e) => {
                    if let Ok(mut guard) = state.lock() {
                        if guard.gid == gid && index < guard.status.len() {
                            guard.status[index] = 0;
                        }
                    }
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = window_weak.upgrade() {
                            window.set_reader_error(format!("{e}").into());
                        }
                    });
                }
            }
        });
    }

    /// UI 线程：把待转换的位图变成图片，并在当前位置时显示
    pub fn drain_reader_pending(self: &Rc<Self>) {
        let pending: Vec<(usize, RgbaImage)> = {
            let mut state = self.reader.lock().unwrap();
            state.pending.drain().collect()
        };
        if pending.is_empty() {
            return;
        }

        let (index, gid) = self.reader.lock().map(|s| (s.index, s.gid)).unwrap_or((0, 0));
        {
            let mut images = self.reader_images.borrow_mut();
            if images.gid != gid {
                images.images.clear();
                images.gid = gid;
            }
            for (page, bitmap) in pending {
                images.images.insert(page, to_slint_image(bitmap));
            }
            images.retain_around(index, CACHE_WINDOW);
        }

        if let Some(image) = self.reader_images.borrow().images.get(&index).cloned() {
            self.window.set_reader_error("".into());
            self.window.set_reader_current(image);
        }
    }

    /// 翻页（UI 线程调用）
    pub fn reader_step(self: &Rc<Self>, delta: i64) {
        let index = {
            let mut state = self.reader.lock().unwrap();
            if state.is_empty() {
                return;
            }
            let next = (state.index as i64 + delta).clamp(0, state.len() as i64 - 1) as usize;
            state.index = next;
            next
        };

        // 已解码的直接显示，未解码的置空并加载（避免闪一下上一页）
        let cached = self.reader_images.borrow().images.get(&index).cloned();
        self.reader_images.borrow_mut().retain_around(index, CACHE_WINDOW);
        match cached {
            Some(image) => self.window.set_reader_current(image),
            None => self.window.set_reader_current(slint::Image::default()),
        }

        self.window.set_reader_index(index as i32);
        self.show_reader_page();
        self.save_read_progress(index);
    }

    /// 当前页的 RGBA 像素（UI 线程只做拷贝，编码交给阻塞线程池）
    fn current_image_pixels(self: &Rc<Self>) -> Option<(Vec<u8>, u32, u32)> {
        let index = self.reader.lock().ok()?.index;
        let image = self.reader_images.borrow().images.get(&index).cloned()?;
        let buffer = image.to_rgba8()?;
        let (width, height) = (buffer.width(), buffer.height());
        let pixels: Vec<u8> = buffer.as_slice().iter().flat_map(|px| [px.r, px.g, px.b, px.a]).collect();
        Some((pixels, width, height))
    }

    /// 保存当前页到相册。
    ///
    /// PNG 编码是大图 CPU 活（~2160px 宽），放 `spawn_blocking`，UI 线程只做
    /// `slint::Image → RgbaImage` 的拷贝。
    pub fn save_current_image(self: &Rc<Self>) {
        let (bytes, width, height) = match self.current_image_pixels() {
            Some(v) => v,
            None => {
                self.window.set_reader_error("no image to save".into());
                return;
            }
        };
        let platform = self.platform.clone();
        let cache_dir = self.cache_dir.lock().unwrap().clone();
        let window_weak = self.window.as_weak();
        let index = self.reader.lock().map(|s| s.index).unwrap_or(0);

        self.runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let path = cache_dir.join(format!("reader-{}.png", index));
                jhentai_core::image::RgbaImage::from_rgba(width, height, bytes)
                    .and_then(|image| image.save_png(&path).ok().map(|_| path))
                    .map(|path| platform.save_image_to_gallery(&path, "image/png"))
            })
            .await
            .ok()
            .flatten();

            let message = if result == Some(true) { "saved to gallery" } else { "save failed" };
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(window) = window_weak.upgrade() {
                    window.set_reader_error(message.into());
                }
            });
        });
    }

    /// 分享当前页（本地文件直接分享，在线页先落盘）
    pub fn share_current_image(self: &Rc<Self>) {
        if let Some(path) = self.reader.lock().ok().and_then(|s| s.local_files.get(s.index).cloned()) {
            self.platform.share_file(&path, "image/png");
            return;
        }
        // 在线页：编码落盘后再分享
        let Some((bytes, width, height)) = self.current_image_pixels() else {
            self.window.set_reader_error("no image to share".into());
            return;
        };
        let platform = self.platform.clone();
        let cache_dir = self.cache_dir.lock().unwrap().clone();
        let window_weak = self.window.as_weak();
        let index = self.reader.lock().map(|s| s.index).unwrap_or(0);

        self.runtime.spawn(async move {
            let path = tokio::task::spawn_blocking(move || {
                let path = cache_dir.join(format!("reader-{}.png", index));
                jhentai_core::image::RgbaImage::from_rgba(width, height, bytes)
                    .and_then(|image| image.save_png(&path).ok().map(|_| path))
            })
            .await
            .ok()
            .flatten();

            match path {
                Some(path) => platform.share_file(&path, "image/png"),
                None => {
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(window) = window_weak.upgrade() {
                            window.set_reader_error("share failed".into());
                        }
                    });
                }
            }
        });
    }

    /// 阅读进度写库（`local_config` 的 `readIndexRecord`）
    pub fn save_read_progress(self: &Rc<Self>, index: usize) {
        let gid = self.reader.lock().map(|s| s.gid).unwrap_or_default();
        if gid == 0 {
            return;
        }
        if let Ok(mut store) = self.store.lock() {
            jhentai_core::service::read_progress::save(store.as_mut(), gid, index as i64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thumbs(n: usize) -> Vec<GalleryThumbnail> {
        (0..n).map(|i| GalleryThumbnail { href: format!("/s/abc/1-{i}"), ..Default::default() }).collect()
    }

    #[test]
    fn reader_state_clamps_start() {
        let state = ReaderState::new(1, false, thumbs(3), 99);
        assert_eq!(state.index, 2);
        assert_eq!(state.status.len(), 3);
        assert!(state.status.iter().all(|s| *s == 0));
    }

    #[test]
    fn reader_state_handles_empty() {
        let state = ReaderState::new(1, false, vec![], 0);
        assert!(state.is_empty());
        assert_eq!(state.index, 0, "空画廊不能让 index 下溢");
    }

    #[test]
    fn cache_keeps_only_window() {
        // 用同样语义的 HashMap 验证淘汰规则（slint::Image 不适合在测试里构造）
        let mut pages: HashMap<usize, usize> = (0..10).map(|i| (i, i)).collect();
        let index = 5;
        pages.retain(|page, _| page.abs_diff(index) <= CACHE_WINDOW);
        assert_eq!(pages.len(), CACHE_WINDOW * 2 + 1);
        assert!(pages.contains_key(&3) && pages.contains_key(&7));
        assert!(!pages.contains_key(&2) && !pages.contains_key(&8));
    }

    #[test]
    fn absolute_uses_ex_host_when_needed() {
        assert_eq!(reader::absolute_page_url("/s/abc/1-0", true), "https://exhentai.org/s/abc/1-0");
        assert_eq!(reader::absolute_page_url("/s/abc/1-0", false), "https://e-hentai.org/s/abc/1-0");
    }
}
