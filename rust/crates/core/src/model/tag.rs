use serde::{Deserialize, Serialize};

/// 与 Dart `TagData` 一致：namespace + key，JSON 里 key 存为 `_key`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TagData {
    pub namespace: String,
    #[serde(rename = "_key")]
    pub key: String,
}

impl TagData {
    pub fn new(namespace: impl Into<String>, key: impl Into<String>) -> Self {
        Self { namespace: namespace.into(), key: key.into() }
    }

    /// `namespace:key`，E-H 页面上标签的表示形式。
    pub fn full(&self) -> String {
        format!("{}:{}", self.namespace, self.key)
    }
}

/// `gt` 置信 / `gtl` 存疑 / `gtw` 错误
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EhTagStatus {
    Confidence,
    Skepticism,
    Incorrect,
}

impl Default for EhTagStatus {
    fn default() -> Self {
        Self::Confidence
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EhTagVoteStatus {
    #[default]
    None,
    Up,
    Down,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GalleryTag {
    /// ARGB
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(rename = "backgroundColor", skip_serializing_if = "Option::is_none")]
    pub background_color: Option<u32>,
    #[serde(rename = "tagData")]
    pub tag_data: TagData,
    #[serde(rename = "tagStatus", skip_serializing_if = "Option::is_none")]
    pub tag_status: Option<EhTagStatus>,
    #[serde(rename = "voteStatus", skip_serializing_if = "Option::is_none")]
    pub vote_status: Option<EhTagVoteStatus>,
}

impl GalleryTag {
    pub fn new(tag_data: TagData) -> Self {
        Self { tag_data, ..Default::default() }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EhRawTag {
    pub namespace: String,
    pub key: String,
}

/// `LinkedHashMap<String, List<GalleryTag>>` 的等价物：保持插入顺序。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TagMap(pub Vec<(String, Vec<GalleryTag>)>);

impl TagMap {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn push(&mut self, tag: GalleryTag) {
        let ns = tag.tag_data.namespace.clone();
        match self.0.iter_mut().find(|(n, _)| *n == ns) {
            Some((_, v)) => v.push(tag),
            None => self.0.push((ns, vec![tag])),
        }
    }

    pub fn get(&self, namespace: &str) -> Option<&Vec<GalleryTag>> {
        self.0.iter().find(|(n, _)| n == namespace).map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<GalleryTag>)> {
        self.0.iter().map(|(n, v)| (n, v))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
