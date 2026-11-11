//! 画廊下载引擎：任务队列 + 并发下载 + 断点续传 + metadata/ComicInfo 落盘。
//!
//! 对应 Dart 的 `service/gallery_download/*`（7 个文件）。状态机与磁盘布局保持一致，
//! 但不移植 dart 的 executor 轮询——这里用 tokio + Semaphore，进度通过回调推给 UI。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::download::comic_info::build_comic_info_xml;
use crate::download::path_resolver::{
    compute_sanitized_gallery_title, gallery_directory, image_path, COMIC_INFO_FILE_NAME, METADATA_FILE_NAME,
};
use crate::model::{GalleryDetail, GalleryImage, GalleryMetadata};
use crate::net::http::{get_bytes_with_referer, Client};
use crate::net::{FrontingResolver, NetState};
use crate::service::reader;

/// 下载状态（与 Dart 的 `GalleryDownloadStatus` 索引一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GalleryDownloadStatus {
    NotDownloaded,
    Downloading,
    Downloaded,
    Failed,
}

impl GalleryDownloadStatus {
    pub fn index(&self) -> i64 {
        match self {
            Self::NotDownloaded => 0,
            Self::Downloading => 1,
            Self::Downloaded => 2,
            Self::Failed => 3,
        }
    }
}

/// 一个画廊的下载任务
#[derive(Debug, Clone)]
pub struct GalleryDownload {
    pub detail: GalleryDetail,
    pub download_path: PathBuf,
    /// 已下载的图片序号（1 基）
    pub downloaded: Arc<Mutex<std::collections::HashSet<i64>>>,
    pub total: i64,
    pub status: GalleryDownloadStatus,
}

impl GalleryDownload {
    pub fn new(detail: GalleryDetail, download_path: PathBuf) -> Self {
        let total = detail.page_count;
        Self { detail, download_path, downloaded: Arc::new(Mutex::new(Default::default())), total, status: GalleryDownloadStatus::NotDownloaded }
    }

    /// `<downloadPath>/<gid> - <sanitizedTitle>`
    pub fn directory(&self) -> PathBuf {
        let sanitized = compute_sanitized_gallery_title(&self.detail.raw_title, self.prefix_bytes());
        gallery_directory(&self.download_path, self.detail.gallery_url.gid, &sanitized)
    }

    /// `<sanitizedTitle>` 前缀 `"{gid} - "` 的字节数
    fn prefix_bytes(&self) -> usize {
        format!("{} - ", self.detail.gallery_url.gid).len()
    }

    pub fn downloaded_count(&self) -> i64 {
        self.downloaded.lock().map(|d| d.len() as i64).unwrap_or(0)
    }

    /// 是否所有图片都在磁盘上（断点续传由文件存在性判断）
    pub fn is_complete(&self) -> bool {
        self.downloaded_count() >= self.total && self.total > 0
    }
}

/// 进度回调（UI 线程消费前先转成 Send 数据）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub gid: i64,
    pub downloaded: i64,
    pub total: i64,
    pub status: GalleryDownloadStatus,
}

/// 下载失败原因
#[derive(Debug)]
pub enum DownloadError {
    Network(String),
    Io(String),
    /// 509 / 配额
    ExceedLimit,
    /// 需要登录才能下原图
    LoginRequired,
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "network: {e}"),
            Self::Io(e) => write!(f, "io: {e}"),
            Self::ExceedLimit => write!(f, "exceedImageLimits"),
            Self::LoginRequired => write!(f, "loginRequired"),
        }
    }
}

impl std::error::Error for DownloadError {}

/// 扫描已有下载目录：把磁盘上已经存在的图片记成「已下载」，实现断点续传。
pub fn scan_existing(task: &GalleryDownload) {
    let dir = task.directory();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut downloaded = task.downloaded.lock().unwrap();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|n| n.split('.').next()) else {
            continue;
        };
        if let Ok(serial) = stem.parse::<i64>() {
            if serial >= 1 && serial <= task.total {
                downloaded.insert(serial);
            }
        }
    }
}

/// 图片魔数校验：HTML 错误页/登录页绝不能落盘
pub fn looks_like_image(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(12)];
    head.starts_with(&[0xFF, 0xD8, 0xFF])                        // jpeg
        || head.starts_with(&[0x89, b'P', b'N', b'G'])           // png
        || (head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP") // webp
        || head.starts_with(b"GIF8")                             // gif
}

/// 下载单张图片并落盘（`<dir>/<serialNo>.<ext>`）。
///
/// 先写 `.part` 再 rename，避免半截文件被当成已完成。
pub async fn download_image(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    gallery_dir: &Path,
    image: &GalleryImage,
    serial_no: i64,
    referer: Option<&str>,
    download_original: bool,
) -> Result<PathBuf, DownloadError> {
    use std::io::Write;

    // 原图（fullimg）需要登录；默认用 `#img` 直链
    let url = if download_original {
        image.original_image_url.clone().unwrap_or_else(|| image.url.clone())
    } else {
        image.url.clone()
    };
    let path = image_path(gallery_dir, &url, serial_no);
    if path.exists() {
        return Ok(path);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| DownloadError::Io(e.to_string()))?;
    }

    let (status, bytes) = get_bytes_with_referer(client, state, resolver, &url, referer)
        .await
        .map_err(|e| DownloadError::Network(e.to_string()))?;

    match status {
        200 => {}
        // fullimg 需要登录
        302 | 403 => return Err(DownloadError::LoginRequired),
        other => return Err(DownloadError::Network(format!("status {other}"))),
    }

    if !looks_like_image(&bytes) {
        // 登录页/配额页都是 HTML，直接失败，绝不能当成图片存下来
        return Err(DownloadError::Network(format!("not an image ({} bytes)", bytes.len())));
    }

    let part = path.with_extension(format!("{}.part", path.extension().and_then(|e| e.to_str()).unwrap_or("bin")));
    {
        let mut file = std::fs::File::create(&part).map_err(|e| DownloadError::Io(e.to_string()))?;
        file.write_all(&bytes).map_err(|e| DownloadError::Io(e.to_string()))?;
    }
    std::fs::rename(&part, &path).map_err(|e| DownloadError::Io(e.to_string()))?;
    Ok(path)
}

// ---------------------------------------------------------------- metadata

#[derive(Debug, Serialize, Deserialize)]
struct MetadataFile {
    gallery: GalleryMetadata,
    /// Dart 版把 images 数组再 jsonEncode 一次，所以这里是字符串
    images: String,
}

/// 写 `metadata`（与 Dart 版字段一致，便于旧版读回）
pub fn write_metadata(task: &GalleryDownload, images: &[GalleryImage]) -> std::io::Result<()> {
    let dir = task.directory();
    std::fs::create_dir_all(&dir)?;
    let gallery = metadata_of(&task.detail);
    let images_json = serde_json::to_string(images).unwrap_or_else(|_| "[]".to_string());
    let file = MetadataFile { gallery, images: images_json };
    let text = serde_json::to_string(&file).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(dir.join(METADATA_FILE_NAME), text)
}

/// 从磁盘上的 `metadata` 恢复任务（覆盖安装后读旧版下载）
pub fn read_metadata(path: &Path) -> Option<(GalleryMetadata, Vec<GalleryImage>)> {
    let text = std::fs::read_to_string(path).ok()?;
    let file: MetadataFile = serde_json::from_str(&text).ok()?;
    let images: Vec<GalleryImage> = serde_json::from_str(&file.images).unwrap_or_default();
    Some((file.gallery, images))
}

fn metadata_of(detail: &GalleryDetail) -> GalleryMetadata {
    GalleryMetadata {
        gallery_url: detail.gallery_url.clone(),
        title: detail.raw_title.clone(),
        japanese_title: detail.japanese_title.clone().unwrap_or_default(),
        category: detail.category.clone(),
        cover: detail.cover.clone(),
        page_count: detail.page_count,
        rating: detail.rating,
        language: detail.language.clone(),
        uploader: detail.uploader.clone(),
        publish_time: detail.publish_time.clone(),
        is_expunged: detail.is_expunged,
        size: detail.size.clone(),
        torrent_count: detail.torrent_count.parse().unwrap_or(0),
        tags: detail.tags.clone(),
    }
}

/// 写 `ComicInfo.xml`
pub fn write_comic_info(task: &GalleryDownload) -> std::io::Result<()> {
    let dir = task.directory();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(COMIC_INFO_FILE_NAME), build_comic_info_xml(&task.detail))
}

// ---------------------------------------------------------------- 引擎

/// 下载引擎配置
#[derive(Debug, Clone)]
pub struct DownloadOptions {
    /// 下载根目录（`getExternalFilesDir(null)/download`）
    pub download_path: PathBuf,
    /// 并发数（对应 `downloadSetting.downloadTaskConcurrency`）
    pub concurrency: usize,
    /// 是否下原图（需要登录）
    pub download_original: bool,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self { download_path: PathBuf::from("download"), concurrency: 3, download_original: false }
    }
}

/// 批量下载一组画廊，进度通过 `on_progress` 回调（Send，可在 tokio 线程调用）。
///
/// 流程：补全缩略图（多页画廊逐页抓）→ 扫描磁盘断点续传 → 并发下载（Semaphore 控并发）
/// → 全部成功后写 ComicInfo 并标记完成。
pub async fn download_gallery(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    options: &DownloadOptions,
    detail: GalleryDetail,
    on_progress: Arc<dyn Fn(DownloadProgress) + Send + Sync>,
) -> Result<GalleryDownload, DownloadError> {
    let gid = detail.gallery_url.gid;
    let total = detail.page_count;

    // 1. 缩略图可能是多页的（每页 20-40 张），先补全，否则只会下一部分
    let thumbnails = crate::service::reader::fetch_all_thumbnails(
        client,
        state,
        resolver,
        &detail.gallery_url,
        &detail.thumbnails,
        detail.thumbnails_page_count,
    )
    .await
    .map_err(|e| DownloadError::Network(e.to_string()))?;

    let mut detail = detail;
    detail.thumbnails = thumbnails;

    let mut task = GalleryDownload::new(detail.clone(), options.download_path.clone());
    let dir = task.directory();
    std::fs::create_dir_all(&dir).map_err(|e| DownloadError::Io(e.to_string()))?;
    scan_existing(&task);

    let counter = Arc::new(AtomicI64::new(task.downloaded_count()));
    let report = {
        let on_progress = on_progress.clone();
        move |status: GalleryDownloadStatus, done: i64| {
            on_progress(DownloadProgress { gid, downloaded: done, total, status });
        }
    };
    report(GalleryDownloadStatus::Downloading, counter.load(Ordering::Relaxed));

    // 2. 并发下载（每页先解析图片页拿直链，再下载并落盘）
    let semaphore = Arc::new(tokio::sync::Semaphore::new(options.concurrency.max(1)));
    let is_ex = !detail.gallery_url.is_eh;
    let resolved_images: Arc<Mutex<Vec<(i64, GalleryImage)>>> = Arc::new(Mutex::new(Vec::new()));

    let mut set = tokio::task::JoinSet::new();
    for (index, thumb) in detail.thumbnails.iter().enumerate() {
        let serial = index as i64 + 1;
        if serial > detail.page_count {
            break;
        }
        if task.downloaded.lock().map(|d| d.contains(&serial)).unwrap_or(false) {
            continue;
        }

        let page_url = reader::absolute_page_url(&thumb.href, is_ex);
        let client = client.clone();
        let state = state.clone();
        let resolver = resolver.clone();
        let semaphore = semaphore.clone();
        let dir = dir.clone();
        let counter = counter.clone();
        let downloaded = task.downloaded.clone();
        let resolved_images = resolved_images.clone();
        let on_progress = on_progress.clone();
        let download_original = options.download_original;

        set.spawn(async move {
            let _permit = semaphore.acquire().await.map_err(|e| DownloadError::Network(e.to_string()))?;
            let image = match reader::resolve_image_page(&client, &state, &resolver, &page_url).await {
                Ok(image) => image,
                Err(reader::ReadError::ExceedLimit) => return Err(DownloadError::ExceedLimit),
                Err(e) => return Err(DownloadError::Network(e.to_string())),
            };
            let actual_url = if download_original {
                image.original_image_url.clone().unwrap_or_else(|| image.url.clone())
            } else {
                image.url.clone()
            };

            download_image(&client, &state, &resolver, &dir, &image, serial, Some(&page_url), download_original).await?;

            if let Ok(mut set) = downloaded.lock() {
                set.insert(serial);
            }
            if let Ok(mut list) = resolved_images.lock() {
                list.push((serial, GalleryImage { url: actual_url, ..image }));
            }
            let done = counter.fetch_add(1, Ordering::Relaxed) + 1;
            on_progress(DownloadProgress { gid, downloaded: done, total, status: GalleryDownloadStatus::Downloading });
            Ok::<(), DownloadError>(())
        });
    }

    let mut first_error: Option<DownloadError> = None;
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(DownloadError::Network(e.to_string()));
                }
            }
        }
    }

    // 3. metadata：图片按序号排列，URL 用实际下载的地址（与 Dart 版一致）
    let mut images: Vec<(i64, GalleryImage)> = resolved_images.lock().map(|l| l.clone()).unwrap_or_default();
    images.sort_by_key(|(serial, _)| *serial);
    let images: Vec<GalleryImage> = images.into_iter().map(|(_, image)| image).collect();
    let _ = write_metadata(&task, &images);

    let done = counter.load(Ordering::Relaxed);
    if let Some(error) = first_error {
        task.status = GalleryDownloadStatus::Failed;
        report(GalleryDownloadStatus::Failed, done);
        return Err(error);
    }

    // 4. 只有真的全部落盘才算完成（缩略图补全后仍可能少页）
    if !task.is_complete() {
        task.status = GalleryDownloadStatus::Downloading;
        report(GalleryDownloadStatus::Downloading, done);
        return Err(DownloadError::Network(format!("incomplete: {done}/{total}")));
    }

    let _ = write_comic_info(&task);
    task.status = GalleryDownloadStatus::Downloaded;
    report(GalleryDownloadStatus::Downloaded, done);
    Ok(task)
}

/// 已下载画廊的本地图片路径（离线阅读用）
pub fn offline_images(dir: &Path, page_count: i64) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for serial in 1..=page_count {
        let Ok(entries) = std::fs::read_dir(dir) else {
            break;
        };
        let found = entries.flatten().find(|e| {
            e.file_name()
                .to_str()
                .and_then(|n| n.split('.').next())
                .and_then(|s| s.parse::<i64>().ok())
                .map(|n| n == serial)
                .unwrap_or(false)
        });
        if let Some(entry) = found {
            paths.push(entry.path());
        }
    }
    paths
}

/// 下载根目录下所有画廊目录（本地画廊扫描）
pub fn scan_gallery_directories(download_path: &Path) -> Vec<(i64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(download_path) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name();
            let gid = name.to_str()?.split(" - ").next()?.parse().ok()?;
            Some((gid, e.path()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GalleryThumbnail, GalleryUrl, TagData, TagMap};
    use crate::model::GalleryTag;

    fn detail(gid: i64, title: &str, pages: i64) -> GalleryDetail {
        let mut tags = TagMap::new();
        tags.push(GalleryTag::new(TagData::new("language", "english")));
        GalleryDetail {
            gallery_url: GalleryUrl::new(true, gid, "aaaaaaaaaa"),
            raw_title: title.into(),
            category: "Manga".into(),
            page_count: pages,
            language: "English".into(),
            size: "1.00 MiB".into(),
            torrent_count: "0".into(),
            tags,
            thumbnails: (0..pages)
                .map(|i| GalleryThumbnail { href: format!("/s/abc/{gid}-{i}"), thumb_url: format!("https://x/{i}.jpg"), ..Default::default() })
                .collect(),
            ..Default::default()
        }
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "jhentai-dl-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn task_directory_matches_dart_layout() {
        let task = GalleryDownload::new(detail(123, "Some/Title", 5), PathBuf::from("/dl"));
        let dir = task.directory();
        assert_eq!(dir.file_name().unwrap().to_str().unwrap(), "123 - Some Title");
    }

    #[test]
    fn scan_existing_recovers_partial_download() {
        let root = temp_dir("scan");
        let task = GalleryDownload::new(detail(7, "t", 3), root.clone());
        let dir = task.directory();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1.jpg"), b"x").unwrap();
        std::fs::write(dir.join("3.webp"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();

        scan_existing(&task);
        let mut found: Vec<i64> = task.downloaded.lock().unwrap().iter().copied().collect();
        found.sort();
        assert_eq!(found, vec![1, 3], "只认数字命名的图片，断点续传跳过已存在的");
        assert!(!task.is_complete());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn metadata_roundtrip_matches_dart_shape() {
        let root = temp_dir("meta");
        let task = GalleryDownload::new(detail(42, "Title", 2), root.clone());
        write_metadata(&task, &[GalleryImage::url_only("https://x/1.jpg")]).unwrap();

        let text = std::fs::read_to_string(task.directory().join(METADATA_FILE_NAME)).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(value["gallery"]["galleryUrl"]["gid"].as_i64() == Some(42));
        // Dart 版把 images 再编码一次，这里必须是字符串
        assert!(value["images"].is_string());

        let (meta, images) = read_metadata(&task.directory().join(METADATA_FILE_NAME)).unwrap();
        assert_eq!(meta.title, "Title");
        assert_eq!(images.len(), 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn comic_info_written_next_to_metadata() {
        let root = temp_dir("comic");
        let task = GalleryDownload::new(detail(9, "T", 1), root.clone());
        write_comic_info(&task).unwrap();
        let xml = std::fs::read_to_string(task.directory().join(COMIC_INFO_FILE_NAME)).unwrap();
        assert!(xml.contains("<Title>T</Title>"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn magic_bytes_reject_html() {
        assert!(looks_like_image(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(looks_like_image(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));
        assert!(looks_like_image(b"RIFF0000WEBPVP8X"));
        assert!(looks_like_image(b"GIF89a"));
        // 登录页 / 配额页
        assert!(!looks_like_image(b"<!DOCTYPE html><html>"));
        assert!(!looks_like_image(b""));
    }

    #[test]
    fn scan_directories_reads_gid() {
        let root = temp_dir("dirs");
        std::fs::create_dir_all(root.join("123 - some title")).unwrap();
        std::fs::create_dir_all(root.join("not a gallery")).unwrap();
        let found = scan_gallery_directories(&root);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, 123);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn offline_images_lists_in_serial_order() {
        let root = temp_dir("offline");
        std::fs::write(root.join("2.jpg"), b"x").unwrap();
        std::fs::write(root.join("1.png"), b"x").unwrap();
        let paths = offline_images(&root, 3);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("1.png"));
        assert!(paths[1].ends_with("2.jpg"));
        let _ = std::fs::remove_dir_all(root);
    }
}
