//! 下载路径规则（对应 `service/gallery_download/download_path_resolver.dart`）。
//!
//! 新旧两套命名规则都要保留：旧版按 **字符数** 截断（gallery 85 / archive 80），
//! 新版按 **字节数** 截断到 200 字节。恢复旧数据时必须用旧规则，
//! 否则算出来的目录名在磁盘上根本不存在。

use std::path::{Path, PathBuf};

/// 单个文件名允许的最大字节数
pub const MAX_FILE_NAME_BYTES: usize = 200;
/// 旧版按字符截断的上限
pub const LEGACY_GALLERY_TITLE_MAX_CHARS: usize = 85;
/// 旧版归档目录按字符截断的上限
pub const LEGACY_ARCHIVE_TITLE_MAX_CHARS: usize = 80;

/// 去掉非法字符（与 Dart 的 `[/|?,:*"<>\.]` 一致）
pub fn sanitize_title(raw: &str) -> String {
    const ILLEGAL: &[u32] = &[0x2F, 0x7C, 0x3F, 0x2C, 0x3A, 0x2A, 0x22, 0x3C, 0x3E, 0x5C, 0x2E];
    raw.chars()
        .map(|c| if ILLEGAL.contains(&(c as u32)) { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// 按字节截断，且不切开多字节字符
pub fn truncate_to_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = 0;
    for (index, ch) in text.char_indices() {
        if index + ch.len_utf8() > max_bytes {
            break;
        }
        end = index + ch.len_utf8();
    }
    text[..end].trim().to_string()
}

/// 新版规则：非法字符替换 + 按字节截断（预留前缀字节数）
pub fn compute_sanitized_gallery_title(raw_title: &str, reserved_bytes: usize) -> String {
    let cleaned = sanitize_title(raw_title);
    truncate_to_bytes(&cleaned, MAX_FILE_NAME_BYTES.saturating_sub(reserved_bytes))
}

/// 旧版规则：非法字符替换 + 按字符截断到 85
pub fn compute_legacy_sanitized_gallery_title(raw_title: &str) -> String {
    let cleaned = sanitize_title(raw_title);
    let truncated: String = cleaned.chars().take(LEGACY_GALLERY_TITLE_MAX_CHARS).collect();
    truncated.trim().to_string()
}

/// 旧版归档目录规则：按字符截断到 80
pub fn compute_legacy_archive_title(raw_title: &str) -> String {
    let cleaned = sanitize_title(raw_title);
    let truncated: String = cleaned.chars().take(LEGACY_ARCHIVE_TITLE_MAX_CHARS).collect();
    truncated.trim().to_string()
}

/// `<downloadPath>/<gid> - <sanitizedTitle>`
pub fn gallery_directory(download_path: &Path, gid: i64, sanitized_title: &str) -> PathBuf {
    download_path.join(format!("{gid} - {sanitized_title}"))
}

/// 从磁盘上的目录名反推标题（回填旧数据时目录名比 metadata 更可信）
pub fn title_from_directory_name(gid: i64, directory: &Path) -> Option<String> {
    let name = directory.file_name()?.to_str()?;
    let prefix = format!("{gid} - ");
    name.strip_prefix(&prefix).map(str::to_string)
}

/// 图片扩展名：`fullimg.php` 这类原图链接没有扩展名，按 jpg 处理
pub fn image_extension(image_url: &str) -> String {
    // 与 Dart 一致：只有老式 `fullimg.php`（无扩展名）才强制 jpg
    if image_url.contains("fullimg.php") {
        return "jpg".to_string();
    }
    let path = image_url.split(['?', '#']).next().unwrap_or(image_url);
    let ext = path.rsplit('.').next().unwrap_or("jpg").to_ascii_lowercase();
    // 只接受看起来像扩展名的短串，其余按 jpg
    if ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        ext
    } else {
        "jpg".to_string()
    }
}

/// `<galleryDir>/<serialNo>.<ext>`
pub fn image_path(gallery_dir: &Path, image_url: &str, serial_no: i64) -> PathBuf {
    gallery_dir.join(format!("{serial_no}.{}", image_extension(image_url)))
}

/// `metadata` 文件名
pub const METADATA_FILE_NAME: &str = "metadata";
/// `ComicInfo.xml` 文件名
pub const COMIC_INFO_FILE_NAME: &str = "ComicInfo.xml";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_illegal_chars() {
        // 用 char 数组拼出含反斜杠的输入，避免源码里的转义歧义
        let backslash = char::from_u32(0x5C).unwrap();
        let raw = format!("a/b|c?d,e:f*g\"h<i>j{backslash}k.l");
        assert_eq!(sanitize_title(&raw), "a b c d e f g h i j k l");
        assert_eq!(sanitize_title("  spaced  "), "spaced");
    }

    #[test]
    fn truncates_on_char_boundary() {
        // 每个汉字 3 字节；10 字节只能放下 3 个汉字
        assert_eq!(truncate_to_bytes("一二三四", 10), "一二三");
        assert_eq!(truncate_to_bytes("short", 100), "short");
    }

    #[test]
    fn reserved_bytes_are_subtracted() {
        let title = compute_sanitized_gallery_title(&"x".repeat(500), 20);
        assert!(title.len() <= MAX_FILE_NAME_BYTES - 20);
    }

    #[test]
    fn legacy_rules_match_dart() {
        let long = "y".repeat(200);
        assert_eq!(compute_legacy_sanitized_gallery_title(&long).chars().count(), LEGACY_GALLERY_TITLE_MAX_CHARS);
        assert_eq!(compute_legacy_archive_title(&long).chars().count(), LEGACY_ARCHIVE_TITLE_MAX_CHARS);
    }

    #[test]
    fn directory_and_title_roundtrip() {
        let dir = gallery_directory(Path::new("/dl"), 123, "some title");
        assert!(dir.ends_with("123 - some title"));
        assert_eq!(title_from_directory_name(123, &dir).as_deref(), Some("some title"));
        assert_eq!(title_from_directory_name(999, &dir), None);
    }

    #[test]
    fn image_extension_rules() {
        assert_eq!(image_extension("https://x/1.webp"), "webp");
        assert_eq!(image_extension("https://x/1.JPG"), "jpg");
        // 新式 /fullimg/<hash>/name.png 按扩展名落 .png（与 Dart 一致）
        assert_eq!(image_extension("https://e/fullimg/1/2/abc/name.png"), "png");
        assert_eq!(image_extension("https://e/fullimg.php?gid=1"), "jpg");
        assert_eq!(image_extension("https://x/noext"), "jpg");
    }
}
