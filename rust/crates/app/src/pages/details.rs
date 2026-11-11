//! 详情页 controller：解析画廊详情、缩略图、评论，并压栈/出栈。
//!
//! 对应 Dart 的 `pages/details/details_page_logic.dart`。UI 在 `ui/app.slint` 的 DetailsPage。

use std::rc::Rc;

use slint::{ComponentHandle, Model};

use jhentai_core::model::GalleryDetail;
use jhentai_core::net::consts;
use jhentai_core::parser;

use crate::{AppWindow, ThumbnailItem};

/// 详情页状态
#[derive(Debug, Default)]
pub struct DetailsState {
    pub gid: i64,
    pub detail: Option<GalleryDetail>,
    /// 已加载的缩略图 hash，避免重复下载
    pub loaded_thumbnails: std::collections::HashSet<String>,
}

/// 详情 → Slint 头部展示用的字段
pub fn apply_detail(window: &AppWindow, detail: &GalleryDetail) {
    window.set_detail_title(detail.raw_title.clone().into());
    window.set_detail_japanese_title(detail.japanese_title.clone().unwrap_or_default().into());
    window.set_detail_category(detail.category.clone().into());
    window.set_detail_category_color(crate::pages::gallery_list::category_color(&detail.category));
    window.set_detail_pages(detail.page_count as i32);
    window.set_detail_rating(detail.rating as f32);
    window.set_detail_real_rating(detail.real_rating as f32);
    window.set_detail_rating_count(detail.rating_count as i32);
    window.set_detail_uploader(detail.uploader.clone().unwrap_or_default().into());
    window.set_detail_publish_time(detail.publish_time.clone().into());
    window.set_detail_size(detail.size.clone().into());
    window.set_detail_language(detail.language.clone().into());
    window.set_detail_is_favorite(detail.is_favorite());
    window.set_detail_expunged(detail.is_expunged);

    // 标签按 namespace 分组展示："namespace: key key"
    let mut tag_lines: Vec<slint::SharedString> = Vec::new();
    for (namespace, tags) in detail.tags.iter() {
        let keys: Vec<String> = tags.iter().map(|t| t.tag_data.key.clone()).collect();
        tag_lines.push(format!("{namespace}: {}", keys.join(", ")).into());
    }
    window.set_detail_tags(Rc::new(slint::VecModel::from(tag_lines)).into());
}

/// 缩略图 → Slint 列表项
pub fn thumbnails_to_items(detail: &GalleryDetail) -> Vec<ThumbnailItem> {
    detail
        .thumbnails
        .iter()
        .enumerate()
        .map(|(index, thumb)| ThumbnailItem {
            index: index as i32,
            thumb: slint::Image::default(),
            href: thumb.href.clone().into(),
            offset: thumb.offset.unwrap_or_default() as f32,
            is_large: thumb.is_large,
        })
        .collect()
}

pub fn comments_to_items(detail: &GalleryDetail) -> Vec<slint::SharedString> {
    detail
        .comments
        .iter()
        .map(|c| {
            // 评论正文是 HTML，UI 只做纯文本预览（富文本在 M6 用 StyledText）
            let text = strip_html(&c.content);
            format!(
                "{} · {} · {}",
                c.username.clone().unwrap_or_else(|| "anonymous".into()),
                c.time,
                text
            )
            .into()
        })
        .collect()
}

/// 极简 HTML 去标签（评论预览用）
pub fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl crate::AppController {
    /// 打开详情页（UI 线程调用）：压栈 + 拉取页面
    pub fn open_details(self: &Rc<Self>, gid: i64, token: String, is_ex: bool) {
        self.nav.borrow_mut().push(crate::nav::Page::Details { gid, token: token.clone() });

        self.show_page(&crate::nav::Page::Details { gid, token: token.clone() });
        let window = &self.window;
        window.set_detail_loading(true);
        window.set_detail_error("".into());
        window.set_detail_thumbnails(Rc::new(slint::VecModel::from(Vec::<ThumbnailItem>::new())).into());
        window.set_detail_comments(Rc::new(slint::VecModel::from(Vec::<slint::SharedString>::new())).into());

        let base = if is_ex { consts::EX_INDEX } else { consts::EH_INDEX };
        let url = format!("{base}/g/{gid}/{token}/");

        // 先登记当前 gid，之后所有回填都以它校验
        {
            let mut guard = self.details.lock().unwrap();
            guard.gid = gid;
            guard.detail = None;
            guard.loaded_thumbnails.clear();
        }

        let client = self.client.clone();
        let net = self.net.clone();
        let resolver = self.resolver.clone();
        let window_weak = self.window.as_weak();
        let details = self.details.clone();
        let ui_events = self.ui_events.clone();
        let more_thumbs_wanted = self.more_thumbs_wanted.clone();

        self.runtime.spawn(async move {
            let parsed = match jhentai_core::net::get(&client, &net, &resolver, &url).await {
                Ok(response) => parser::detail_page(&response.body).map_err(jhentai_core::net::RequestError::Parse),
                Err(e) => Err(e),
            };

            let _ = slint::invoke_from_event_loop(move || {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                // 用户可能已经切到别的画廊：只接受当前 gid 的响应
                if details.lock().map(|g| g.gid).unwrap_or_default() != gid {
                    return;
                }
                window.set_detail_loading(false);
                match parsed {
                    Ok(result) => {
                        let detail = result.detail;
                        apply_detail(&window, &detail);
                        window.set_detail_thumbnails(
                            Rc::new(slint::VecModel::from(thumbnails_to_items(&detail))).into(),
                        );
                        window.set_detail_comments(Rc::new(slint::VecModel::from(comments_to_items(&detail))).into());
                        window.set_detail_thumbnail_pages(detail.thumbnails_page_count as i32);

                        let need_more = detail.thumbnails_page_count > 1;
                        {
                            let mut guard = details.lock().unwrap();
                            guard.gid = detail.gallery_url.gid;
                            guard.loaded_thumbnails.clear();
                            guard.detail = Some(detail.clone());
                        }
                        ui_events.request_details_thumbs();

                        // 多页缩略图画廊：置标志，由 UI 泵发起补全（Rc 不能进闭包）
                        if need_more {
                            more_thumbs_wanted.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    Err(e) => window.set_detail_error(format!("{e}").into()),
                }
            });
        });
    }

    /// 后台抓全量缩略图（多页画廊），完成后替换详情页列表。
    ///
    /// 由 UI 泵在 `more_thumbs_wanted` 置位后调用（`Rc` 只能活在 UI 线程）。
    pub fn fetch_more_thumbnails(self: &Rc<Self>) {
        let Some(detail) = self.details.lock().ok().and_then(|g| g.detail.clone()) else {
            return;
        };
        if detail.thumbnails_page_count <= 1 {
            return;
        }
        let client = self.client.clone();
        let net = self.net.clone();
        let resolver = self.resolver.clone();
        let details = self.details.clone();
        let window_weak = self.window.as_weak();
        let ui_events = self.ui_events.clone();
        let more_thumbs_wanted = self.more_thumbs_wanted.clone();
        let gid = detail.gallery_url.gid;

        self.runtime.spawn(async move {
            let all = jhentai_core::service::reader::fetch_all_thumbnails(
                &client,
                &net,
                &resolver,
                &detail.gallery_url,
                &detail.thumbnails,
                detail.thumbnails_page_count,
            )
            .await;
            let Ok(all) = all else { return };

            let _ = slint::invoke_from_event_loop(move || {
                let Some(window) = window_weak.upgrade() else {
                    return;
                };
                let mut guard = details.lock().unwrap();
                // 用户可能已经切走：gid 不匹配就丢弃
                if guard.gid != gid {
                    return;
                }
                if let Some(current) = guard.detail.as_mut() {
                    current.thumbnails = all;
                }
                let items = guard.detail.as_ref().map(thumbnails_to_items).unwrap_or_default();
                guard.loaded_thumbnails.clear();
                drop(guard);

                window.set_detail_thumbnails(Rc::new(slint::VecModel::from(items)).into());
                ui_events.request_details_thumbs();
            });
        });
    }

    /// 详情页缩略图懒加载（UI 线程调用）：每次取一批未加载的
    pub fn load_detail_thumbnails(self: &Rc<Self>) {
        const BATCH: usize = 12;

        let pending: Vec<(usize, String, String)> = {
            let mut guard = self.details.lock().unwrap();
            let Some(detail) = guard.detail.as_ref() else {
                return;
            };
            let picked: Vec<(usize, String, String)> = detail
                .thumbnails
                .iter()
                .enumerate()
                .filter(|(_, t)| !guard.loaded_thumbnails.contains(&t.href))
                .take(BATCH)
                .map(|(index, t)| (index, t.href.clone(), t.thumb_url.clone()))
                .collect();
            for (_, href, _) in &picked {
                guard.loaded_thumbnails.insert(href.clone());
            }
            picked
        };
        if pending.is_empty() {
            return;
        }

        for (_index, href, url) in pending {
            let client = self.client.clone();
            let net = self.net.clone();
            let resolver = self.resolver.clone();
            let details = self.details.clone();
            let window_weak = self.window.as_weak();

            self.runtime.spawn(async move {
                let decoded = crate::pages::cover_loader::fetch_image(
                    &client,
                    &net,
                    &resolver,
                    &url,
                    crate::pages::cover_loader::THUMBNAIL_WIDTH,
                )
                .await;
                let Some(decoded) = decoded else {
                    // 失败回滚，下次还会再试
                    details.lock().unwrap().loaded_thumbnails.remove(&href);
                    return;
                };
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(window) = window_weak.upgrade() else {
                        return;
                    };
                    let image = crate::pages::cover_loader::to_slint_image(decoded);
                    let values = window.get_detail_thumbnails();
                    // 按 href 找行：期间可能已经换成别的画廊
                    let target = (0..Model::row_count(&values)).find(|row| {
                        Model::row_data(&values, *row).map(|item| item.href == href.as_str()).unwrap_or(false)
                    });
                    if let Some(row) = target {
                        if let Some(mut updated) = Model::row_data(&values, row) {
                            updated.thumb = image;
                            Model::set_row_data(&values, row, updated);
                        }
                    }
                });
            });
        }
    }

    /// 返回上一页：只有真的出栈成功才切 UI，且按栈顶全变体映射
    pub fn go_back(self: &Rc<Self>) {
        if !self.nav.borrow_mut().back() {
            return;
        }
        let current = self.nav.borrow().current().clone();
        // 离开详情页时清掉 gid，避免在途响应继续拉缩略图
        if !matches!(current, crate::nav::Page::Details { .. }) {
            self.details.lock().unwrap().gid = 0;
        }
        self.show_page(&current);
    }

    /// 页栈 → UI 的单一映射（push / go_back 都走它，避免两处漂移）
    pub fn show_page(self: &Rc<Self>, page: &crate::nav::Page) {
        match page {
            crate::nav::Page::GalleryList => self.window.set_page(0),
            crate::nav::Page::Details { .. } => self.window.set_page(1),
            crate::nav::Page::Read { .. } => self.window.set_page(2),
            // shortcut: 下载/设置页的 UI 在 M5/M6 接入，先回到列表避免白屏
            crate::nav::Page::Downloads | crate::nav::Page::Settings => self.window.set_page(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_keeps_text() {
        assert_eq!(strip_html("<p>hello <b>world</b></p>"), "hello world");
        assert_eq!(strip_html("a<br/>b"), "a b");
        assert_eq!(strip_html(""), "");
    }
}
