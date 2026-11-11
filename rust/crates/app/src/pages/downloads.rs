//! 下载页 controller：发起画廊下载、展示进度、本地画廊列表。
//!
//! 状态机在 core（`download::gallery`），这里只做接线：
//! tokio 任务里跑下载，进度用 `Arc<Mutex<..>>` 汇总，UI 泵负责刷新页面。

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;



use jhentai_core::download::gallery::{
    download_gallery, scan_gallery_directories, DownloadOptions, DownloadProgress, GalleryDownloadStatus,
};
use jhentai_core::model::GalleryDetail;

use crate::DownloadItem;

/// 下载页状态（跨线程：只装 Send 数据）
#[derive(Debug, Default)]
pub struct DownloadsState {
    /// gid -> 进度
    pub progress: HashMap<i64, DownloadProgress>,
    /// 正在下载的 gid
    pub active: std::collections::HashSet<i64>,
    /// 下载根目录
    pub download_path: std::path::PathBuf,
    /// 并发数
    pub concurrency: usize,
    /// gid -> 标题（从 metadata 读，本地画廊列表用）
    pub titles: HashMap<i64, String>,
}

impl DownloadsState {
    pub fn sorted(&self) -> Vec<DownloadProgress> {
        let mut list: Vec<DownloadProgress> = self.progress.values().copied().collect();
        list.sort_by_key(|p| (p.status.index(), p.gid));
        list
    }
}

/// 一行下载项的展示数据
pub fn progress_to_item(progress: &DownloadProgress, title: String) -> DownloadItem {
    DownloadItem {
        gid: progress.gid as i32,
        title: title.into(),
        downloaded: progress.downloaded as i32,
        total: progress.total as i32,
        status: match progress.status {
            GalleryDownloadStatus::NotDownloaded => 0,
            GalleryDownloadStatus::Downloading => 1,
            GalleryDownloadStatus::Downloaded => 2,
            GalleryDownloadStatus::Failed => 3,
        },
        ratio: if progress.total > 0 { (progress.downloaded as f32) / (progress.total as f32) } else { 0.0 },
    }
}

impl crate::AppController {
    /// 开始下载当前详情页的画廊（UI 线程调用）
    pub fn start_download(self: &Rc<Self>) {
        let Some(detail) = self.details.lock().ok().and_then(|g| g.detail.clone()) else {
            return;
        };
        self.start_download_of(detail);
    }

    /// 下载指定详情（下载页重试也走这里）
    pub fn start_download_of(self: &Rc<Self>, detail: GalleryDetail) {
        let gid = detail.gallery_url.gid;
        {
            let mut state = self.downloads.lock().unwrap();
            if state.active.contains(&gid) {
                return;
            }
            state.active.insert(gid);
            state.progress.insert(
                gid,
                DownloadProgress { gid, downloaded: 0, total: detail.page_count, status: GalleryDownloadStatus::Downloading },
            );
        }
        self.refresh_downloads_view();

        let client = self.client.lock().unwrap().clone();
        let net = self.net.clone();
        let resolver = self.resolver.clone();
        let downloads = self.downloads.clone();
        let ui_events = self.ui_events.clone();
        let options = {
            let state = self.downloads.lock().unwrap();
            DownloadOptions {
                download_path: state.download_path.clone(),
                concurrency: state.concurrency,
                download_original: false,
            }
        };

        self.runtime.spawn(async move {
            let progress_sink = {
                let downloads = downloads.clone();
                let ui_events = ui_events.clone();
                Arc::new(move |p: DownloadProgress| {
                    if let Ok(mut state) = downloads.lock() {
                        state.progress.insert(p.gid, p);
                    }
                    ui_events.request_downloads();
                })
            };

            let result = download_gallery(&client, &net, &resolver, &options, detail.clone(), progress_sink).await;

            let status = match &result {
                Ok(_) => GalleryDownloadStatus::Downloaded,
                Err(_) => GalleryDownloadStatus::Failed,
            };
            if let Ok(mut state) = downloads.lock() {
                state.active.remove(&gid);
                if let Some(p) = state.progress.get_mut(&gid) {
                    p.status = status;
                }
            }
            ui_events.request_downloads();
        });
    }

    /// 扫描下载目录，把磁盘上已有的画廊列出来（本地画廊）
    pub fn scan_downloads(self: &Rc<Self>) {
        let path = self.downloads.lock().unwrap().download_path.clone();

        let mut state = self.downloads.lock().unwrap();
        for (gid, dir) in scan_gallery_directories(&path) {
            let downloaded = std::fs::read_dir(&dir)
                .map(|entries| {
                    entries
                        .flatten()
                        .filter(|e| {
                            e.file_name()
                                .to_str()
                                .and_then(|n| n.split('.').next())
                                .and_then(|s| s.parse::<i64>().ok())
                                .is_some()
                        })
                        .count() as i64
                })
                .unwrap_or(0);
            // 标题优先取 metadata（与 Dart 版 metadata 兼容）
            if let Some((meta, _)) = jhentai_core::download::gallery::read_metadata(&dir.join("metadata")) {
                state.titles.insert(gid, meta.title);
            }
            let entry = state.progress.entry(gid).or_insert(DownloadProgress {
                gid,
                downloaded,
                total: downloaded,
                status: GalleryDownloadStatus::Downloaded,
            });
            if entry.total == 0 {
                entry.downloaded = downloaded;
                entry.total = downloaded;
                entry.status = GalleryDownloadStatus::Downloaded;
            }
        }
        drop(state);
        self.refresh_downloads_view();
    }

    /// 把下载状态渲染到 UI（UI 线程）
    pub fn refresh_downloads_view(self: &Rc<Self>) {
        let items: Vec<DownloadItem> = {
            let state = self.downloads.lock().unwrap();
            state
                .sorted()
                .iter()
                .map(|p| {
                    let title = state
                        .titles
                        .get(&p.gid)
                        .cloned()
                        .or_else(|| {
                            self.details
                                .lock()
                                .ok()
                                .and_then(|g| g.detail.as_ref().filter(|d| d.gallery_url.gid == p.gid).map(|d| d.raw_title.clone()))
                        })
                        .unwrap_or_else(|| format!("gid {}", p.gid));
                    progress_to_item(p, title)
                })
                .collect()
        };
        self.window.set_downloads(Rc::new(slint::VecModel::from(items)).into());
    }

    /// 从下载页打开本地画廊（离线阅读）
    pub fn open_local_gallery(self: &Rc<Self>, gid: i64) {
        let dir = {
            let state = self.downloads.lock().unwrap();
            scan_gallery_directories(&state.download_path).into_iter().find(|(g, _)| *g == gid).map(|(_, d)| d)
        };
        let Some(dir) = dir else {
            return;
        };
        let metadata_path = dir.join("metadata");
        let Some((meta, _images)) = jhentai_core::download::gallery::read_metadata(&metadata_path) else {
            return;
        };
        let count = meta.page_count;
        let files = jhentai_core::download::gallery::offline_images(&dir, count);
        if files.is_empty() {
            return;
        }

        {
            let mut reader = self.reader.lock().unwrap();
            reader.reset_local(gid, files.clone());
        }
        self.reader_images.borrow_mut().images.clear();
        let page = crate::nav::Page::Read { gid };
        self.nav.borrow_mut().push(page.clone());
        self.show_page(&page);
        self.show_local_reader_page();
    }

    /// 本地阅读：按文件路径解码（不走网络）
    pub fn show_local_reader_page(self: &Rc<Self>) {
        let (index, total) = {
            let state = self.reader.lock().unwrap();
            (state.index, state.local_files.len())
        };
        self.window.set_reader_index(index as i32);
        self.window.set_reader_total(total as i32);
        for offset in [-1i64, 0, 1] {
            let target = index as i64 + offset;
            if target >= 0 && (target as usize) < total {
                self.load_local_reader_page(target as usize);
            }
        }
    }

    /// 解码本地图片文件
    pub fn load_local_reader_page(self: &Rc<Self>, index: usize) {
        let path = {
            let mut state = self.reader.lock().unwrap();
            if index >= state.local_files.len() || state.status.get(index).copied().unwrap_or(0) != 0 {
                return;
            }
            if let Some(slot) = state.status.get_mut(index) {
                *slot = 1;
            }
            state.local_files[index].clone()
        };

        let state = self.reader.clone();
        let ui_events = self.ui_events.clone();
        self.runtime.spawn(async move {
            let decoded = tokio::task::spawn_blocking(move || {
                std::fs::read(&path).ok().and_then(|bytes| jhentai_core::image::decode_for_reader(&bytes, 1080).ok())
            })
            .await
            .ok()
            .flatten();

            let Some(bitmap) = decoded else {
                if let Ok(mut guard) = state.lock() {
                    if let Some(slot) = guard.status.get_mut(index) {
                        *slot = 0;
                    }
                }
                return;
            };
            if let Ok(mut guard) = state.lock() {
                guard.status[index] = 2;
                guard.pending.insert(index, bitmap);
            }
            ui_events.request_reader();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_puts_downloading_first() {
        let mut state = DownloadsState::default();
        state.progress.insert(1, DownloadProgress { gid: 1, downloaded: 1, total: 2, status: GalleryDownloadStatus::Downloaded });
        state.progress.insert(2, DownloadProgress { gid: 2, downloaded: 0, total: 2, status: GalleryDownloadStatus::Downloading });
        let sorted = state.sorted();
        assert_eq!(sorted[0].gid, 2, "下载中的排前面");
        assert_eq!(sorted[1].gid, 1);
    }

    #[test]
    fn progress_to_item_maps_status_and_ratio() {
        let progress = DownloadProgress { gid: 7, downloaded: 3, total: 4, status: GalleryDownloadStatus::Downloading };
        let item = progress_to_item(&progress, "t".into());
        assert_eq!((item.gid, item.downloaded, item.total, item.status), (7, 3, 4, 1));
        assert!((item.ratio - 0.75).abs() < f32::EPSILON);

        let empty = progress_to_item(&DownloadProgress { gid: 1, downloaded: 0, total: 0, status: GalleryDownloadStatus::Failed }, String::new());
        assert_eq!(empty.ratio, 0.0, "total 为 0 时不能除零");
    }
}
