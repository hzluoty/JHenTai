//! `ComicInfo.xml` 生成（对应 `model/comic_info.dart` 的 `EHGalleryComicInfo`）。
//!
//! 字段映射与 Dart 版一致，便于 ComicRack/Kavita 之类工具识别。

use crate::model::{GalleryDetail, TagData};

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// 语言 → ISO 缩写（Dart 的 `LocaleConsts.language2Abbreviation`）
pub fn language_abbreviation(language: &str) -> Option<&'static str> {
    Some(match language {
        "Japanese" => "jp",
        "English" => "en",
        "Chinese" => "zh",
        "Spanish" => "es",
        "French" => "fr",
        "Korean" => "ko",
        "Russian" => "ru",
        "Portuguese" => "pt",
        "Thai" => "th",
        "Vietnamese" => "vi",
        "German" => "de",
        "Italian" => "it",
        "Indonesian" => "id",
        "Hungarian" => "hu",
        "Dutch" => "nl",
        "Polish" => "pl",
        "Czech" => "cs",
        "Turkish" => "tr",
        "Ukrainian" => "uk",
        "Greek" => "el",
        _ => return None,
    })
}

fn tags_of<'a>(tag_datas: &'a [TagData], namespace: &str) -> Vec<&'a str> {
    tag_datas
        .iter()
        .filter(|t| t.namespace == namespace)
        .map(|t| t.key.as_str())
        .collect()
}

/// 生成完整 `ComicInfo.xml` 文本
pub fn build_comic_info_xml(detail: &GalleryDetail) -> String {
    let tag_datas: Vec<TagData> = detail
        .tags
        .iter()
        .flat_map(|(_, tags)| tags.iter().map(|t| t.tag_data.clone()))
        .collect();

    let artists = tags_of(&tag_datas, "artist");
    let characters = tags_of(&tag_datas, "character");
    let locations = tags_of(&tag_datas, "location");
    let all_tags: Vec<String> = tag_datas.iter().map(|t| t.full()).collect();

    let mut xml = String::with_capacity(2048);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<ComicInfo xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xmlns:xsn=\"http://www.w3.org/2001/XMLSchema\">\n");
    xml.push_str(&format!("  <Title>{}</Title>\n", escape(&detail.raw_title)));
    xml.push_str(&format!("  <Series>{}</Series>\n", escape(&detail.raw_title)));
    if let Some(jp) = detail.japanese_title.as_deref().filter(|t| !t.is_empty()) {
        xml.push_str(&format!("  <AlternateSeries>{}</AlternateSeries>\n", escape(jp)));
    }
    let artist_line = artists.join(",");
    if !artist_line.is_empty() {
        xml.push_str(&format!("  <Writer>{}</Writer>\n", escape(&artist_line)));
        xml.push_str(&format!("  <Penciller>{}</Penciller>\n", escape(&artist_line)));
    }
    xml.push_str(&format!("  <Genre>{}</Genre>\n", escape(&detail.category)));
    xml.push_str(&format!("  <Tags>{}</Tags>\n", escape(&all_tags.join(","))));
    xml.push_str(&format!("  <Web>{}</Web>\n", escape(&detail.gallery_url.url())));
    xml.push_str(&format!("  <PageCount>{}</PageCount>\n", detail.page_count));
    if let Some(iso) = language_abbreviation(&detail.language) {
        xml.push_str(&format!("  <LanguageISO>{iso}</LanguageISO>\n"));
    }
    let full_color = tag_datas.iter().any(|t| t.key == "full color");
    xml.push_str(&format!("  <BlackAndWhite>{}</BlackAndWhite>\n", if full_color { "No" } else { "Yes" }));
    xml.push_str(&format!("  <Manga>{}</Manga>\n", if detail.category == "Manga" { "Yes" } else { "No" }));
    if !characters.is_empty() {
        xml.push_str(&format!("  <Characters>{}</Characters>\n", escape(&characters.join(","))));
    }
    if !locations.is_empty() {
        xml.push_str(&format!("  <Locations>{}</Locations>\n", escape(&locations.join(","))));
    }
    let age_rating = if detail.category == "Non-H" { "Kids to Adults" } else { "Adults Only 18+" };
    xml.push_str(&format!("  <AgeRating>{age_rating}</AgeRating>\n"));
    xml.push_str(&format!("  <CommunityRating>{:.1}</CommunityRating>\n", detail.real_rating));
    xml.push_str("</ComicInfo>\n");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GalleryDetail, GalleryUrl, GalleryTag, TagMap};

    fn sample() -> GalleryDetail {
        let mut tags = TagMap::new();
        tags.push(GalleryTag::new(TagData::new("artist", "someone")));
        tags.push(GalleryTag::new(TagData::new("female", "sole female")));
        tags.push(GalleryTag::new(TagData::new("character", "lute")));
        GalleryDetail {
            gallery_url: GalleryUrl::new(true, 42, "aaaaaaaaaa"),
            raw_title: "Test & <Title>".into(),
            japanese_title: Some("テスト".into()),
            category: "Manga".into(),
            page_count: 12,
            language: "Japanese".into(),
            real_rating: 4.25,
            tags,
            ..Default::default()
        }
    }

    #[test]
    fn xml_has_expected_fields() {
        let xml = build_comic_info_xml(&sample());
        assert!(xml.contains("<Title>Test &amp; &lt;Title&gt;</Title>"));
        assert!(xml.contains("<AlternateSeries>テスト</AlternateSeries>"));
        assert!(xml.contains("<Writer>someone</Writer>"));
        assert!(xml.contains("<Characters>lute</Characters>"));
        assert!(xml.contains("<LanguageISO>jp</LanguageISO>"));
        assert!(xml.contains("<Manga>Yes</Manga>"));
        assert!(xml.contains("<BlackAndWhite>Yes</BlackAndWhite>"));
        assert!(xml.contains("<CommunityRating>4.2</CommunityRating>"));
        assert!(xml.contains("<PageCount>12</PageCount>"));
    }

    #[test]
    fn full_color_and_age_rating() {
        let mut detail = sample();
        detail.category = "Non-H".into();
        detail.tags.push(GalleryTag::new(TagData::new("other", "full color")));
        let xml = build_comic_info_xml(&detail);
        assert!(xml.contains("<BlackAndWhite>No</BlackAndWhite>"));
        assert!(xml.contains("<AgeRating>Kids to Adults</AgeRating>"));
        assert!(xml.contains("<Manga>No</Manga>"));
    }

    #[test]
    fn unknown_language_omits_iso() {
        let mut detail = sample();
        detail.language = "Klingon".into();
        assert!(!build_comic_info_xml(&detail).contains("LanguageISO"));
        assert_eq!(language_abbreviation("Klingon"), None);
    }
}
