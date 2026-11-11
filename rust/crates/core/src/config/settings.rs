//! 与 Dart 版逐字段对齐的设置 struct。
//!
//! 字段名必须与 `lib/src/setting/*.dart` 的 `toConfigString()` JSON 键一致，
//! 否则覆盖安装后旧设置读不出来（见计划 §4.4）。
//! 只覆盖当前 UI 用到的字段；缺失字段用 `#[serde(default)]` 容忍（Dart 写多字段不影响读）。

use serde::{Deserialize, Serialize};

/// `EHSetting`：站点选择
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EhSetting {
    /// "EH" 或 "EX"
    #[serde(default = "default_site")]
    pub site: String,
    /// Dart 版的历史字段：登录后自动跳 EX。目前 Rust 版用显式站点选择，
    /// 保留字段只为「读旧配置不报错」，不参与逻辑（写回时保持原值）。
    #[serde(rename = "redirect2Eh", default = "default_true")]
    pub redirect_2_eh: bool,
}

fn default_site() -> String {
    "EH".to_string()
}

fn default_true() -> bool {
    true
}

impl Default for EhSetting {
    fn default() -> Self {
        Self { site: default_site(), redirect_2_eh: true }
    }
}

impl EhSetting {
    pub fn is_ex(&self) -> bool {
        self.site == "EX"
    }

    pub fn set_site(&mut self, is_ex: bool) {
        self.site = if is_ex { "EX".to_string() } else { "EH".to_string() };
    }
}

/// 阅读方向（Dart `ReadDirection` enum 的 index 顺序）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReadDirectionIndex(pub i64);

impl ReadDirectionIndex {
    pub fn is_right_to_left(&self) -> bool {
        // Dart: topToBottom=0, leftToRight=1, rightToLeft=2
        self.0 == 2
    }

    pub fn label(&self) -> &'static str {
        match self.0 {
            1 => "Left to Right",
            2 => "Right to Left",
            _ => "Vertical",
        }
    }

    pub fn cycle(&mut self) {
        self.0 = (self.0 + 1) % 3;
    }
}

/// `ReadSetting` 的子集（其余字段 Dart 写进来也能读，只是这里不展示）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadSetting {
    #[serde(rename = "readDirection", default)]
    pub read_direction: ReadDirectionIndex,
    #[serde(rename = "keepScreenAwakeWhenReading", default = "default_true")]
    pub keep_screen_awake_when_reading: bool,
    #[serde(rename = "enablePageTurnByVolumeKeys", default = "default_true")]
    pub enable_page_turn_by_volume_keys: bool,
    #[serde(rename = "showStatusInfo", default = "default_true")]
    pub show_status_info: bool,
    #[serde(rename = "preloadDistance", default = "default_preload")]
    pub preload_distance: i64,
    #[serde(rename = "preloadPageCount", default = "default_preload")]
    pub preload_page_count: i64,
}

fn default_preload() -> i64 {
    1
}

impl Default for ReadSetting {
    fn default() -> Self {
        Self {
            read_direction: ReadDirectionIndex::default(),
            keep_screen_awake_when_reading: true,
            enable_page_turn_by_volume_keys: true,
            show_status_info: true,
            preload_distance: 1,
            preload_page_count: 1,
        }
    }
}

/// `SecuritySetting` 的子集：应用锁
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SecuritySetting {
    /// 4-8 位数字 PIN；None 表示未启用应用锁
    #[serde(rename = "pin", default)]
    pub pin: Option<String>,
}

impl SecuritySetting {
    pub fn is_locked_enabled(&self) -> bool {
        self.pin.as_deref().map(|p| !p.is_empty()).unwrap_or(false)
    }

    /// 校验输入；未启用时一律通过
    pub fn verify(&self, input: &str) -> bool {
        match &self.pin {
            None => true,
            Some(pin) if pin.is_empty() => true,
            Some(pin) => pin == input,
        }
    }
}

/// `DownloadSetting` 的子集
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSetting {
    #[serde(rename = "downloadTaskConcurrency", default = "default_concurrency")]
    pub download_task_concurrency: i64,
    #[serde(rename = "downloadOriginalImage", default)]
    pub download_original_image: bool,
}

fn default_concurrency() -> i64 {
    3
}

impl Default for DownloadSetting {
    fn default() -> Self {
        Self { download_task_concurrency: 3, download_original_image: false }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eh_setting_matches_dart_json() {
        // Dart 写出来的字段名
        let setting: EhSetting = serde_json::from_str(r#"{"site":"EX","redirect2Eh":false}"#).unwrap();
        assert!(setting.is_ex());
        assert!(!setting.redirect_2_eh);
        assert_eq!(serde_json::to_string(&setting).unwrap(), r#"{"site":"EX","redirect2Eh":false}"#);
    }

    #[test]
    fn eh_setting_defaults_tolerate_missing_fields() {
        let setting: EhSetting = serde_json::from_str(r#"{"site":"EH"}"#).unwrap();
        assert!(setting.redirect_2_eh, "缺字段用默认值");
        let empty: EhSetting = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.site, "EH");
    }

    #[test]
    fn read_setting_tolerates_unknown_fields() {
        // Dart 版会写几十个字段，Rust 只认自己那些
        let json = r#"{"readDirection":2,"keepScreenAwakeWhenReading":false,"enablePageTurnByVolumeKeys":true,
            "showStatusInfo":true,"preloadDistance":3,"preloadPageCount":2,
            "imageSpace":6,"showThumbnails":true,"enableBottomMenu":false}"#;
        let setting: ReadSetting = serde_json::from_str(json).unwrap();
        assert!(setting.read_direction.is_right_to_left());
        assert!(!setting.keep_screen_awake_when_reading);
        assert_eq!(setting.preload_distance, 3);
    }

    #[test]
    fn read_direction_cycles() {
        let mut direction = ReadDirectionIndex::default();
        assert_eq!(direction.label(), "Vertical");
        direction.cycle();
        assert_eq!(direction.label(), "Left to Right");
        direction.cycle();
        assert!(direction.is_right_to_left());
        direction.cycle();
        assert_eq!(direction.0, 0, "循环回垂直");
    }

    #[test]
    fn security_verify() {
        let mut setting = SecuritySetting::default();
        assert!(!setting.is_locked_enabled());
        assert!(setting.verify("anything"), "未启用时一律通过");

        setting.pin = Some("1234".into());
        assert!(setting.is_locked_enabled());
        assert!(setting.verify("1234"));
        assert!(!setting.verify("0000"));
    }

    #[test]
    fn download_setting_defaults() {
        let setting: DownloadSetting = serde_json::from_str(r#"{"downloadTaskConcurrency":5}"#).unwrap();
        assert_eq!(setting.download_task_concurrency, 5);
        assert!(!setting.download_original_image);
    }
}
