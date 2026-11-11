//! 缩略图/封面管线：网络拉取 → 后台解码 → UI 线程回填。
//!
//! 列表封面和详情页缩略图共用这一段（计划 §4.3 的 ZoomableImage 之前的脏活）。

use std::sync::{Arc, Mutex};

use jhentai_core::image::RgbaImage;
use jhentai_core::net::http::{get_bytes, Client};
use jhentai_core::net::{FrontingResolver, NetState};

/// 列表封面解码宽度（卡片宽 60dp，2 倍屏够用）
pub const COVER_WIDTH: u32 = 128;
/// 详情页缩略图宽度
pub const THUMBNAIL_WIDTH: u32 = 220;

/// 一次图片下载 + 解码。失败返回 None（调用方自己决定是否重试）。
pub async fn fetch_image(
    client: &Client,
    net: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    url: &str,
    max_width: u32,
) -> Option<RgbaImage> {
    if url.is_empty() {
        return None;
    }
    let bytes = match get_bytes(client, net, resolver, url).await {
        Ok((200, bytes)) => bytes,
        _ => return None,
    };
    // 解码是 CPU 活，放到阻塞线程池，别占 async worker
    tokio::task::spawn_blocking(move || jhentai_core::image::decode(&bytes, max_width))
        .await
        .ok()
        .and_then(|r| r.ok())
}

/// 位图 → Slint 图片（必须在 UI 线程调用）
pub fn to_slint_image(image: RgbaImage) -> slint::Image {
    slint::Image::from_rgba8(slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
        &image.pixels,
        image.width,
        image.height,
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn fails_fast_on_empty_url() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let net = jhentai_core::net::NetState::shared(jhentai_core::net::ClientOptions::default());
        let client = {
            let guard = net.lock().unwrap();
            jhentai_core::net::build_client(&guard, &jhentai_core::net::FrontingResolver::new()).unwrap()
        };
        let resolver = jhentai_core::net::FrontingResolver::new();
        let result = rt.block_on(super::fetch_image(&client, &net, &resolver, "", 64));
        assert!(result.is_none());
    }
}
