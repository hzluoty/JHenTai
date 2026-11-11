//! E-H/EX 网络层：cookie、域前置、代理、错误映射，对应 `network/eh_request.dart`。
//!
//! 与 Dart 版的差异（计划 §4.6）：域前置改成自定义 DNS 解析 + 关闭 SNI，
//! 证书仍按原域名校验，比 Dart 版「换域名 + 放行所有证书」更安全。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub mod http;

pub use http::*;

use serde::{Deserialize, Serialize};

use crate::config::network::{NetworkSetting, ProxyType};

pub mod consts {
    pub const EH_INDEX: &str = "https://e-hentai.org";
    pub const EX_INDEX: &str = "https://exhentai.org";
    pub const EH_POPULAR: &str = "https://e-hentai.org/popular";
    pub const EX_POPULAR: &str = "https://exhentai.org/popular";
    pub const EH_API: &str = "https://api.e-hentai.org/api.php";
    pub const EX_API: &str = "https://exhentai.org/api.php";
    pub const EH_HOME: &str = "https://e-hentai.org/home.php";
    pub const EH_RANKLIST: &str = "https://e-hentai.org/toplist.php";
    pub const EH_WATCHED: &str = "https://e-hentai.org/watched";
    pub const EX_WATCHED: &str = "https://exhentai.org/watched";
    pub const EH_NEWS: &str = "https://e-hentai.org/news.php";
    pub const EH_FORUMS: &str = "https://forums.e-hentai.org/index.php";
    pub const EH_FAVORITE: &str = "https://e-hentai.org/favorites.php";
    pub const EX_FAVORITE: &str = "https://exhentai.org/favorites.php";
    pub const EH_POPUP: &str = "https://e-hentai.org/gallerypopups.php";
    pub const EX_POPUP: &str = "https://exhentai.org/gallerypopups.php";
    pub const EH_TORRENT: &str = "https://e-hentai.org/gallerytorrents.php";
}

/// E-H 站点异常，对应 `exception/eh_site_exception.dart`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EhSiteException {
    pub kind: EhSiteExceptionKind,
    pub message: String,
    /// 是否应暂停全部下载任务
    pub should_pause_all_download_tasks: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EhSiteExceptionKind {
    BlankBody,
    Banned,
    ExceedLimit,
    EhServerError,
    GalleryDeleted,
    Cloudflare,
    UnsupportedImagePageStyle,
    TagSetExceedLimit,
    GetMetaDataFailed,
    Network,
}

impl EhSiteException {
    pub fn new(kind: EhSiteExceptionKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), should_pause_all_download_tasks: true }
    }

    pub fn no_pause(mut self) -> Self {
        self.should_pause_all_download_tasks = false;
        self
    }
}

impl std::fmt::Display for EhSiteException {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for EhSiteException {}

/// `_emitEHExceptionIfFailed`：把 E-H 返回的错误正文映射成异常。
pub fn check_eh_response_body(body: &str) -> Result<(), EhSiteException> {
    if body.is_empty() {
        return Err(EhSiteException::new(EhSiteExceptionKind::BlankBody, "sadPanda").no_pause());
    }
    if body.starts_with("Your IP address") || body.starts_with("This IP address") {
        return Err(EhSiteException::new(EhSiteExceptionKind::Banned, body));
    }
    if body.starts_with("You have exceeded your image") {
        return Err(EhSiteException::new(EhSiteExceptionKind::ExceedLimit, "exceedImageLimits").no_pause());
    }
    if body.contains("Page load has been aborted due to a fatal error") {
        return Err(EhSiteException::new(EhSiteExceptionKind::EhServerError, "ehServerError").no_pause());
    }
    Ok(())
}

// ---------------------------------------------------------------- Cookie

/// 一条 cookie（只保留 name/value，与 Dart 版存库的形式一致）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
}

impl Cookie {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self { name: name.into(), value: value.into() }
    }

    /// `Cookie.toString()`：`name=value; Path=/;`
    pub fn to_set_cookie_string(&self) -> String {
        format!("{}={}; Path=/;", self.name, self.value)
    }

    /// 从 `Set-Cookie` 头解析 name/value
    pub fn from_set_cookie(value: &str) -> Option<Self> {
        let first = value.split(';').next()?;
        let (name, val) = first.split_once('=')?;
        Some(Self { name: name.trim().to_string(), value: val.trim().to_string() })
    }
}

/// `EHCookieManager`：固定带 `nw=1`、`datatags=1`，其余可持久化。
#[derive(Debug, Clone)]
pub struct CookieManager {
    cookies: Vec<Cookie>,
}

impl Default for CookieManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CookieManager {
    /// Dart 默认带 `nw=1` 和 `datatags=1`：前者开「非 H 内容」，后者让缩略图带原图 hash
    pub fn new() -> Self {
        Self { cookies: vec![Cookie::new("nw", "1"), Cookie::new("datatags", "1")] }
    }

    /// 从 `local_config` 里 `eh_cookies` 的 JSON 恢复
    pub fn from_json(json: &str) -> Self {
        let mut manager = Self::new();
        if let Ok(list) = serde_json::from_str::<Vec<String>>(json) {
            manager.cookies.extend(list.iter().filter_map(|s| Cookie::from_set_cookie(s)));
        }
        manager
    }

    pub fn cookies(&self) -> &[Cookie] {
        &self.cookies
    }

    /// 持久化形式：去掉 `nw` / `datatags`
    pub fn to_json(&self) -> String {
        let list: Vec<String> = self
            .cookies
            .iter()
            .filter(|c| c.name != "nw" && c.name != "datatags")
            .map(Cookie::to_set_cookie_string)
            .collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string())
    }

    /// `Cookie` 请求头的值
    pub fn header_value(&self) -> String {
        self.cookies.iter().map(|c| format!("{}={}", c.name, c.value)).collect::<Vec<_>>().join("; ")
    }

    /// 合并响应里的 cookie；`__utmp` 与假 igneous 丢弃（Ehviewer 的已知坑）
    pub fn store(&mut self, cookies: impl IntoIterator<Item = Cookie>) {
        for cookie in cookies {
            if cookie.name == "__utmp" {
                continue;
            }
            if cookie.name == "igneous" && cookie.value == "mystery" {
                continue;
            }
            match self.cookies.iter_mut().find(|c| c.name == cookie.name) {
                Some(existing) => *existing = cookie,
                None => self.cookies.push(cookie),
            }
        }
    }

    pub fn remove(&mut self, names: &[&str]) {
        self.cookies.retain(|c| !names.contains(&c.name.as_str()));
    }

    pub fn clear(&mut self) {
        self.cookies = vec![Cookie::new("nw", "1"), Cookie::new("datatags", "1")];
    }

    pub fn has_logged_in(&self) -> bool {
        self.cookies.iter().any(|c| c.name == "ipb_member_id")
    }

    pub fn ipb_member_id(&self) -> Option<&str> {
        self.cookies.iter().find(|c| c.name == "ipb_member_id").map(|c| c.value.as_str())
    }
}

// ---------------------------------------------------------------- 域前置 IP 池

/// 移植 `RoundRobinIpProvider`：轮询 + 不可用 IP 冷却 5 分钟。
#[derive(Debug, Default)]
pub struct IpProvider {
    host_to_ips: HashMap<String, Vec<String>>,
    host_to_index: HashMap<String, usize>,
    unavailable: HashMap<(String, String), Instant>,
}

impl IpProvider {
    pub fn new() -> Self {
        let mut provider = Self::default();
        for (host, ips) in NetworkSetting::HOST_TO_IPS {
            provider.host_to_ips.insert(host.to_string(), ips.iter().map(|s| s.to_string()).collect());
            provider.host_to_index.insert(host.to_string(), 0);
        }
        provider
    }

    pub fn supports(&self, host: &str) -> bool {
        self.host_to_ips.get(host).map(|v| !v.is_empty()).unwrap_or(false)
    }

    /// 轮询取下一个可用 IP；全部冷却时退回当前下标
    pub fn next_ip(&mut self, host: &str) -> Option<String> {
        let ips = self.host_to_ips.get(host)?.clone();
        if ips.is_empty() {
            return None;
        }
        let start = *self.host_to_index.get(host).unwrap_or(&0) % ips.len();
        let mut index = start;
        let now = Instant::now();
        loop {
            let candidate = &ips[index];
            let cooling = self
                .unavailable
                .get(&(host.to_string(), candidate.clone()))
                .map(|t| now.duration_since(*t) < Duration::from_secs(300))
                .unwrap_or(false);
            if !cooling {
                break;
            }
            index = (index + 1) % ips.len();
            if index == start {
                break;
            }
        }
        let ip = ips[index].clone();
        self.host_to_index.insert(host.to_string(), (index + 1) % ips.len());
        Some(ip)
    }

    pub fn add_unavailable_ip(&mut self, host: &str, ip: &str) {
        self.unavailable.insert((host.to_string(), ip.to_string()), Instant::now());
    }
}

// ---------------------------------------------------------------- SMTP-free 请求客户端

/// 请求超时/代理配置（从 `NetworkSetting` 推导）。
#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    pub connect_timeout: Option<Duration>,
    pub receive_timeout: Option<Duration>,
    /// `direct` 时不走系统代理也不走自定义代理
    pub proxy: Option<String>,
    pub proxy_username: Option<String>,
    pub proxy_password: Option<String>,
    pub enable_domain_fronting: bool,
}

impl ClientOptions {
    pub fn from_setting(setting: &NetworkSetting, system_proxy: Option<&str>) -> Self {
        let proxy = match setting.proxy_type {
            ProxyType::System => system_proxy.map(str::to_string),
            ProxyType::Http | ProxyType::Socks5 | ProxyType::Socks4 => {
                if setting.proxy_address.trim().is_empty() {
                    None
                } else {
                    let scheme = match setting.proxy_type {
                        ProxyType::Socks5 => "socks5h",
                        ProxyType::Socks4 => "socks4",
                        _ => "http",
                    };
                    let auth = match (&setting.proxy_username, &setting.proxy_password) {
                        (Some(u), Some(p)) if !u.is_empty() => format!("{u}:{p}@"),
                        _ => String::new(),
                    };
                    Some(format!("{scheme}://{auth}{}", setting.proxy_address))
                }
            }
            ProxyType::Direct => None,
        };

        Self {
            connect_timeout: Some(Duration::from_millis(setting.connect_timeout)),
            receive_timeout: Some(Duration::from_millis(setting.receive_timeout)),
            proxy,
            proxy_username: setting.proxy_username.clone(),
            proxy_password: setting.proxy_password.clone(),
            enable_domain_fronting: setting.enable_domain_fronting,
        }
    }
}

/// `requestLookup` 的目标（以图搜图）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupTarget {
    Eh,
    Ex,
    Google,
    Iqdb,
    SauceNao,
    TinEye,
}

impl LookupTarget {
    pub fn url(&self, image_url: &str) -> String {
        match self {
            Self::Eh => format!("https://e-hentai.org/upld.login?ss=1&url={image_url}"),
            Self::Ex => format!("https://exhentai.org/upld.login?ss=1&url={image_url}"),
            Self::Google => format!("https://lens.google.com/uploadbyurl?url={image_url}"),
            Self::Iqdb => format!("https://iqdb.org/?url={image_url}"),
            Self::SauceNao => format!("https://saucenao.com/search.php?db=999&url={image_url}"),
            Self::TinEye => format!("https://tineye.com/search?url={image_url}"),
        }
    }
}

/// 收藏夹排序
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteSort {
    FavoritedTime,
    PublishedTime,
}

impl FavoriteSort {
    pub fn inline_set(&self) -> &'static str {
        match self {
            Self::FavoritedTime => "fs_f",
            Self::PublishedTime => "fs_p",
        }
    }
}

/// 构建搜索查询参数（对应 `SearchConfig.toQueryParameters`）。
#[derive(Debug, Clone, Default)]
pub struct GalleryQuery {
    pub keyword: Option<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub categories: Vec<&'static str>,
    pub only_expunged: bool,
    pub only_with_torrents: bool,
    pub page_at_least: Option<i64>,
    pub page_at_most: Option<i64>,
    pub minimum_rating: i64,
    pub disable_filter_for_language: bool,
    pub disable_filter_for_uploader: bool,
    pub disable_filter_for_tags: bool,
    pub prev_gid: Option<String>,
    pub next_gid: Option<String>,
    pub seek: Option<String>,
}

/// `f_cats` 的位掩码：勾选**不搜索**的分类
pub const ALL_CATEGORIES: [&str; 10] =
    ["Doujinshi", "Manga", "ArtistCG", "GameCG", "Western", "Non-H", "ImageSet", "Cosplay", "AsianPorn", "Misc"];

impl GalleryQuery {
    pub fn to_query_pairs(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = Vec::new();

        let mut search = self.keyword.clone().unwrap_or_default();
        if !self.tags.is_empty() {
            if !search.is_empty() {
                search.push(' ');
            }
            search.push_str(&self.tags.join(" "));
        }
        if let Some(language) = &self.language {
            if !search.is_empty() {
                search.push(' ');
            }
            search.push_str(&format!("language:\"{language}\""));
        }
        if !search.is_empty() {
            pairs.push(("f_search".into(), search));
        }

        if !self.categories.is_empty() {
            // f_cats 记录的是「排除」的分类位
            let mut mask = 0;
            for (i, name) in ALL_CATEGORIES.iter().enumerate() {
                if !self.categories.contains(name) {
                    mask |= 1 << i;
                }
            }
            pairs.push(("f_cats".into(), mask.to_string()));
        }
        if self.only_expunged {
            pairs.push(("f_sh".into(), "on".into()));
        }
        if self.only_with_torrents {
            pairs.push(("f_sto".into(), "on".into()));
        }
        if let Some(v) = self.page_at_least {
            pairs.push(("f_spf".into(), v.to_string()));
        }
        if let Some(v) = self.page_at_most {
            if self.page_at_least.map(|least| v >= least).unwrap_or(true) {
                pairs.push(("f_spt".into(), v.to_string()));
            }
        }
        if self.minimum_rating > 1 {
            pairs.push(("f_srdd".into(), self.minimum_rating.to_string()));
        }
        if self.disable_filter_for_language {
            pairs.push(("f_sfl".into(), "on".into()));
        }
        if self.disable_filter_for_uploader {
            pairs.push(("f_sfu".into(), "on".into()));
        }
        if self.disable_filter_for_tags {
            pairs.push(("f_sft".into(), "on".into()));
        }
        if let Some(prev) = &self.prev_gid {
            pairs.push(("prev".into(), prev.clone()));
        }
        if let Some(next) = &self.next_gid {
            pairs.push(("next".into(), next.clone()));
        }
        if let Some(seek) = &self.seek {
            pairs.push(("seek".into(), seek.clone()));
        }
        pairs
    }

    pub fn to_query_string(&self) -> String {
        self.to_query_pairs()
            .into_iter()
            .map(|(k, v)| format!("{}={}", urlencode(&k), urlencode(&v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// 把 `host:port` 形式的系统代理补成 reqwest 能接受的 URL。
/// Android 的 `Settings.Global.http_proxy` 只给 `host:port`，reqwest 需要 scheme。
pub fn normalize_proxy(proxy: Option<String>) -> Option<String> {
    let proxy = proxy?;
    let trimmed = proxy.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") || trimmed.starts_with("socks") {
        Some(trimmed.to_string())
    } else {
        Some(format!("http://{trimmed}"))
    }
}

/// 最小百分号编码（E-H 查询参数只需要这个子集）
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// ranklist 的 `tl` 参数
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RanklistType {
    Day,
    Month,
    Year,
    AllTime,
}

impl RanklistType {
    pub fn tl(&self) -> i64 {
        match self {
            Self::Day => 15,
            Self::Month => 13,
            Self::Year => 12,
            Self::AllTime => 11,
        }
    }
}

// ---------------------------------------------------------------- 共享状态

/// 网络层共享状态：cookie + IP 池。UI 与后台任务通过 Arc 共享。
#[derive(Debug, Default)]
pub struct NetState {
    pub cookies: CookieManager,
    /// 与 `FrontingResolver` 共享：域前置按 host 各查各的，并发请求不会互相覆盖
    pub ip_provider: Arc<Mutex<IpProvider>>,
    pub options: ClientOptions,
    /// 上一次拉取页面返回的 `Set-Cookie`，供登录解析
    pub last_set_cookie: Vec<String>,
}

impl NetState {
    pub fn new(options: ClientOptions) -> Self {
        Self {
            cookies: CookieManager::new(),
            ip_provider: Arc::new(Mutex::new(IpProvider::new())),
            options,
            last_set_cookie: Vec::new(),
        }
    }

    pub fn shared(options: ClientOptions) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self::new(options)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_manager_roundtrip() {
        let mut m = CookieManager::new();
        assert_eq!(m.cookies().len(), 2);
        m.store([Cookie::new("ipb_member_id", "123"), Cookie::new("igneous", "mystery"), Cookie::new("__utmp", "x")]);
        // 假 igneous 与 __utmp 应被丢弃
        assert!(m.has_logged_in());
        assert_eq!(m.ipb_member_id(), Some("123"));
        let json = m.to_json();
        assert!(!json.contains("nw="));
        let restored = CookieManager::from_json(&json);
        assert_eq!(restored.ipb_member_id(), Some("123"));
        assert!(restored.header_value().contains("datatags=1"));
    }

    #[test]
    fn cookie_json_matches_dart_format() {
        // Dart 存的是 `name=value; Path=/;` 字符串数组
        let m = CookieManager::from_json(r#"["ipb_member_id=42; Path=/;","ipb_pass_hash=deadbeef; Path=/;"]"#);
        assert_eq!(m.ipb_member_id(), Some("42"));
        assert_eq!(m.to_json(), r#"["ipb_member_id=42; Path=/;","ipb_pass_hash=deadbeef; Path=/;"]"#);
    }

    #[test]
    fn ip_provider_round_robin_and_cooldown() {
        let mut provider = IpProvider::new();
        assert!(provider.supports("e-hentai.org"));
        let a = provider.next_ip("e-hentai.org").unwrap();
        let b = provider.next_ip("e-hentai.org").unwrap();
        assert_ne!(a, b);
        provider.add_unavailable_ip("e-hentai.org", &a);
        assert_eq!(provider.next_ip("e-hentai.org").unwrap(), b);
        assert!(!provider.supports("example.com"));
    }

    #[test]
    fn query_pairs_match_dart() {
        let query = GalleryQuery {
            keyword: Some("azur".into()),
            language: Some("chinese".into()),
            categories: vec!["Doujinshi", "Misc"],
            minimum_rating: 3,
            ..Default::default()
        };
        let pairs = query.to_query_pairs();
        assert_eq!(pairs[0], ("f_search".to_string(), "azur language:\"chinese\"".to_string()));
        // 未勾选的 8 个分类进入掩码
        let cats = pairs.iter().find(|(k, _)| k == "f_cats").unwrap();
        assert_eq!(cats.1, (((1u32 << 10) - 1) & !(1 << 0) & !(1 << 9)).to_string());
        assert!(pairs.iter().any(|(k, v)| k == "f_srdd" && v == "3"));
    }

    #[test]
    fn proxy_normalization() {
        assert_eq!(normalize_proxy(None), None);
        assert_eq!(normalize_proxy(Some("  ".into())), None);
        assert_eq!(normalize_proxy(Some("10.0.2.2:10808".into())).as_deref(), Some("http://10.0.2.2:10808"));
        assert_eq!(normalize_proxy(Some("http://a:1".into())).as_deref(), Some("http://a:1"));
        assert_eq!(normalize_proxy(Some("socks5h://a:2".into())).as_deref(), Some("socks5h://a:2"));
    }

    #[test]
    fn body_errors_map_to_exceptions() {
        assert_eq!(check_eh_response_body("").unwrap_err().kind, EhSiteExceptionKind::BlankBody);
        assert_eq!(check_eh_response_body("Your IP address has been temporarily banned").unwrap_err().kind, EhSiteExceptionKind::Banned);
        assert_eq!(
            check_eh_response_body("You have exceeded your image viewing limits").unwrap_err().kind,
            EhSiteExceptionKind::ExceedLimit
        );
        assert!(check_eh_response_body("<html>ok</html>").is_ok());
    }

    #[test]
    fn client_options_from_setting() {
        let mut setting = NetworkSetting::default();
        setting.proxy_type = ProxyType::Socks5;
        setting.proxy_address = "127.0.0.1:1080".into();
        let options = ClientOptions::from_setting(&setting, None);
        assert_eq!(options.proxy.as_deref(), Some("socks5h://127.0.0.1:1080"));

        setting.proxy_type = ProxyType::Direct;
        assert!(ClientOptions::from_setting(&setting, Some("127.0.0.1:8080")).proxy.is_none());
    }
}
