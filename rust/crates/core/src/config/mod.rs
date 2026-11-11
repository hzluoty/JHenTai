//! 配置：`local_config` 表的读写 + 与 Dart 版逐字段对齐的各个设置 struct。
//!
//! 不引入第二个 KV 存储（计划 §4.4）。所有设置都是 serde struct，
//! 字段名与 Dart `toConfigString` 的 JSON 键一致。

pub mod network;
pub mod settings;

use serde_json::Value;

/// `local_config` 的一行。四列与 Dart 版一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalConfigRow {
    pub config_key: String,
    pub sub_config_key: String,
    pub value: String,
    pub utime: String,
}

/// 设置存储接口：生产用 SQLite 实现，测试用内存实现。
pub trait ConfigStore {
    fn read(&self, config_key: &str, sub_config_key: &str) -> Option<String>;
    fn write(&mut self, config_key: &str, sub_config_key: &str, value: &str);
    fn delete(&mut self, config_key: &str, sub_config_key: &str);
}

/// 内存实现（host 测试与首启动迁移用）。
#[derive(Debug, Default, Clone)]
pub struct MemoryConfigStore {
    rows: Vec<LocalConfigRow>,
}

impl MemoryConfigStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 `local_config` 导出数据恢复（覆盖安装首启动）
    pub fn from_rows(rows: Vec<LocalConfigRow>) -> Self {
        Self { rows }
    }

    pub fn rows(&self) -> &[LocalConfigRow] {
        &self.rows
    }
}

impl ConfigStore for MemoryConfigStore {
    fn read(&self, config_key: &str, sub_config_key: &str) -> Option<String> {
        self.rows
            .iter()
            .find(|r| r.config_key == config_key && r.sub_config_key == sub_config_key)
            .map(|r| r.value.clone())
    }

    fn write(&mut self, config_key: &str, sub_config_key: &str, value: &str) {
        let utime = now_string();
        match self.rows.iter_mut().find(|r| r.config_key == config_key && r.sub_config_key == sub_config_key) {
            Some(row) => {
                row.value = value.to_string();
                row.utime = utime;
            }
            None => self.rows.push(LocalConfigRow {
                config_key: config_key.to_string(),
                sub_config_key: sub_config_key.to_string(),
                value: value.to_string(),
                utime,
            }),
        }
    }

    fn delete(&mut self, config_key: &str, sub_config_key: &str) {
        self.rows.retain(|r| !(r.config_key == config_key && r.sub_config_key == sub_config_key));
    }
}

/// `ConfigEnum` 的键（与 `enum/config_enum.dart` 逐项一致，只保留仍在使用的一部分时用常量字符串）
pub mod keys {
    pub const FAVORITE_SETTING: &str = "favoriteSetting";
    pub const ADVANCED_SETTING: &str = "advancedSetting";
    pub const DOWNLOAD_SETTING: &str = "downloadSetting";
    pub const EH_SETTING: &str = "EHSetting";
    pub const NETWORK_SETTING: &str = "networkSetting";
    pub const PERFORMANCE_SETTING: &str = "performanceSetting";
    pub const PREFERENCE_SETTING: &str = "preferenceSetting";
    pub const READ_SETTING: &str = "readSetting";
    pub const SECURITY_SETTING: &str = "securitySetting";
    pub const SITE_SETTING: &str = "siteSetting";
    pub const STYLE_SETTING: &str = "styleSetting";
    pub const USER_SETTING: &str = "userSetting";
    pub const ARCHIVE_BOT_SETTING: &str = "archiveBotSetting";
    pub const MY_TAGS_SETTING: &str = "MyTagsSetting";

    pub const EH_COOKIE: &str = "eh_cookies";
    pub const SEARCH_CONFIG: &str = "searchConfig";
    pub const READ_INDEX_RECORD: &str = "readIndexRecord";
    pub const QUICK_SEARCH: &str = "quickSearch";
    pub const SEARCH_HISTORY: &str = "searchHistory";
    pub const BUILT_IN_BLOCKED_USER: &str = "builtInBlockedUser";
    pub const DISMISS_VERSION: &str = "dismissVersion";
    pub const LANGUAGE: &str = "language";
    pub const GALLERY_IMAGE_HASH: &str = "galleryImageHash";
}

/// 读一个 JSON 设置；缺失或解析失败都返回 None（对应 Dart 的 `readBeanConfig` 容错）。
pub fn read_json<T: serde::de::DeserializeOwned>(store: &dyn ConfigStore, key: &str) -> Option<T> {
    serde_json::from_str(&store.read(key, "")?).ok()
}

/// 写一个 JSON 设置。
pub fn write_json<T: serde::Serialize>(store: &mut dyn ConfigStore, key: &str, value: &T) {
    if let Ok(json) = serde_json::to_string(value) {
        store.write(key, "", &json);
    }
}

/// 读原始字符串配置（cookie、搜索历史这类非 struct 数据）。
pub fn read_string(store: &dyn ConfigStore, key: &str, sub_key: &str) -> Option<String> {
    store.read(key, sub_key)
}

pub fn write_string(store: &mut dyn ConfigStore, key: &str, sub_key: &str, value: &str) {
    store.write(key, sub_key, value);
}

/// 解析失败返回 None
pub fn parse_json(value: &str) -> Option<Value> {
    serde_json::from_str(value).ok()
}

fn now_string() -> String {
    // Dart 写的是 `DateTime.now().toString()`，只用于排序/展示，格式保持可比较即可
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => format!("{}.{:06}", d.as_secs(), d.subsec_micros()),
        Err(_) => "0.000000".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Demo {
        #[serde(rename = "site")]
        site: String,
        #[serde(rename = "redirect2Eh")]
        redirect_2_eh: bool,
    }

    #[test]
    fn roundtrip_json_config() {
        let mut store = MemoryConfigStore::new();
        let demo = Demo { site: "EX".into(), redirect_2_eh: true };
        write_json(&mut store, keys::EH_SETTING, &demo);
        assert_eq!(read_json::<Demo>(&store, keys::EH_SETTING), Some(demo));

        // Dart 写出来的 JSON 必须能被读入
        store.write(keys::EH_SETTING, "", r#"{"site":"EH","redirect2Eh":false}"#);
        assert_eq!(read_json::<Demo>(&store, keys::EH_SETTING).unwrap().site, "EH");
    }

    #[test]
    fn missing_or_broken_config_is_none() {
        let store = MemoryConfigStore::new();
        assert_eq!(read_json::<Demo>(&store, keys::EH_SETTING), None);
        let mut store = MemoryConfigStore::new();
        store.write(keys::EH_SETTING, "", "{not json");
        assert_eq!(read_json::<Demo>(&store, keys::EH_SETTING), None);
    }
}
