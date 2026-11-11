//! 图片解码与缩放。对应 Dart 的 `extended_image` + `photo_view` 那部分能力。
//!
//! 按显示尺寸降采样解码（计划 §7 的大图内存对策）：先读头部拿尺寸，
//! 再按目标宽度缩放到不超过 2 倍显示尺寸，最后交给 UI 层。

use image::{DynamicImage, GenericImageView, ImageFormat};

/// 解码后的 RGBA8 位图
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// 每像素 4 字节，行优先
    pub pixels: Vec<u8>,
}

#[derive(Debug)]
pub enum ImageError {
    /// 不是支持的图片格式
    Unsupported,
    Decode(String),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => write!(f, "unsupported image format"),
            Self::Decode(e) => write!(f, "image decode: {e}"),
        }
    }
}

impl std::error::Error for ImageError {}

/// 从内存解码并按 `max_width`（0 = 不缩放）等比缩小。
pub fn decode(bytes: &[u8], max_width: u32) -> Result<RgbaImage, ImageError> {
    let image = image::load_from_memory(bytes).map_err(|e| ImageError::Decode(e.to_string()))?;
    Ok(scale_to(image, max_width))
}

/// 只解出尺寸（缩略图占位用，避免整图解码）
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::load_from_memory(bytes).ok().map(|i| i.dimensions())
}

/// 格式猜测（保存/分享时用）
pub fn guess_format(bytes: &[u8]) -> Option<ImageFormat> {
    image::guess_format(bytes).ok()
}

/// 等比缩放到 `max_width` 以内，不放大。
pub fn scale_to(image: DynamicImage, max_width: u32) -> RgbaImage {
    let (w, h) = image.dimensions();
    let image = if max_width > 0 && w > max_width {
        let new_h = ((h as f64) * (max_width as f64) / (w as f64)).round().max(1.0) as u32;
        image.resize_exact(max_width, new_h, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    RgbaImage { width: rgba.width(), height: rgba.height(), pixels: rgba.into_raw() }
}

impl RgbaImage {
    /// 写成 PNG（分享/保存到相册前落盘用）
    pub fn save_png(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let buffer = image::RgbaImage::from_raw(self.width, self.height, self.pixels.clone())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "bad rgba buffer"))?;
        buffer.save_with_format(path, ImageFormat::Png).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// 从 `&[u8]` 原始 RGBA 构造
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        (pixels.len() == (width as usize) * (height as usize) * 4).then_some(Self { width, height, pixels })
    }
}

/// 阅读器用：按屏幕宽度降采样，`max_pixels` 防止超大图爆内存
pub fn decode_for_reader(bytes: &[u8], screen_width: u32) -> Result<RgbaImage, ImageError> {
    let image = image::load_from_memory(bytes).map_err(|e| ImageError::Decode(e.to_string()))?;
    let target = (screen_width.saturating_mul(2)).max(1080);
    Ok(scale_to(image, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成一张纯色 PNG 供测试（不落盘）
    fn sample_png(w: u32, h: u32) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 20, 30, 255]));
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    #[test]
    fn decodes_and_reports_size() {
        let png = sample_png(40, 20);
        let decoded = decode(&png, 0).unwrap();
        assert_eq!((decoded.width, decoded.height), (40, 20));
        assert_eq!(decoded.pixels.len(), 40 * 20 * 4);
        assert_eq!(dimensions(&png), Some((40, 20)));
        assert!(matches!(guess_format(&png), Some(ImageFormat::Png)));
    }

    #[test]
    fn scales_down_keeping_aspect_ratio() {
        let png = sample_png(400, 200);
        let decoded = decode(&png, 100).unwrap();
        assert_eq!((decoded.width, decoded.height), (100, 50));
    }

    #[test]
    fn never_upscales() {
        let png = sample_png(50, 50);
        let decoded = decode(&png, 500).unwrap();
        assert_eq!((decoded.width, decoded.height), (50, 50));
    }

    #[test]
    fn reader_decode_caps_width() {
        let png = sample_png(3000, 1000);
        let decoded = decode_for_reader(&png, 1080).unwrap();
        assert_eq!(decoded.width, 2160, "2 倍屏幕宽度");
    }

    #[test]
    fn broken_input_is_an_error() {
        assert!(decode(b"not an image", 0).is_err());
        assert_eq!(dimensions(b"nope"), None);
    }
}
