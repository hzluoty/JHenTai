//! reqwest 实现：域前置、代理、cookie 注入、错误映射。
//!
//! 域前置做法（计划 §4.6）：自定义 DNS 把域名解析到轮询 IP，**保持原域名做 SNI 和证书校验**。
//! 因此不需要 `dangerous_accept_invalid_certs`，比 Dart 版安全。

use std::sync::{Arc, Mutex};

use crate::net::*;
use crate::parser::ParseError;

pub use reqwest::Client;

/// 一次请求的结果
#[derive(Debug, Clone)]
pub struct RawResponse {
    pub status: u16,
    pub body: String,
    pub location: Option<String>,
    pub set_cookie: Vec<String>,
}

#[derive(Debug)]
pub enum RequestError {
    /// E-H 业务错误（正文匹配到了错误页）
    Eh(EhSiteException),
    /// 网络/协议错误
    Network(String),
    /// 解析失败
    Parse(ParseError),
}

impl std::fmt::Display for RequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Eh(e) => write!(f, "{e}"),
            Self::Network(m) => write!(f, "network: {m}"),
            Self::Parse(e) => write!(f, "parse: {e}"),
        }
    }
}

impl std::error::Error for RequestError {}

impl From<ParseError> for RequestError {
    fn from(e: ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<EhSiteException> for RequestError {
    fn from(e: EhSiteException) -> Self {
        Self::Eh(e)
    }
}

/// 域前置解析器。
///
/// 持有共享的 `IpProvider`，在**每次连接**时按 host 查询轮询表：
/// 不存在「按请求改写全局表」的竞争，多个 host 并发前置互不影响；池外的 host 回退系统解析。
#[derive(Clone, Default)]
pub struct FrontingResolver {
    provider: Arc<Mutex<IpProvider>>,
    enabled: Arc<std::sync::atomic::AtomicBool>,
}

impl FrontingResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// 与 `NetState` 共享同一个 IP 池
    pub fn with_provider(provider: Arc<Mutex<IpProvider>>) -> Self {
        Self { provider, enabled: Arc::new(std::sync::atomic::AtomicBool::new(false)) }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// 连接失败时把 IP 放进冷却表
    pub fn mark_unavailable(&self, host: &str, ip: &str) {
        if let Ok(mut provider) = self.provider.lock() {
            provider.add_unavailable_ip(host, ip);
        }
    }
}

impl reqwest::dns::Resolve for FrontingResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let host = name.as_str().to_string();
        let override_ip = if self.is_enabled() {
            self.provider.lock().ok().and_then(|mut provider| provider.next_ip(&host))
        } else {
            None
        };
        Box::pin(async move {
            if let Some(ip) = override_ip {
                if let Ok(addr) = format!("{ip}:443").parse::<std::net::SocketAddr>() {
                    return Ok(Box::new(std::iter::once(addr)) as reqwest::dns::Addrs);
                }
            }
            // 回退系统解析（端口 0：由 URL 覆盖）
            let addrs = tokio::net::lookup_host(format!("{host}:0")).await?;
            Ok(Box::new(addrs) as reqwest::dns::Addrs)
        })
    }
}

/// 依据 `NetState` 构建 reqwest client。
pub fn build_client(state: &NetState, resolver: &FrontingResolver) -> Result<reqwest::Client, RequestError> {
    let mut builder = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36",
        )
        // 与 Dart 版一致：不自动跟随 E-H 的 302（需要读 Location）
        .redirect(reqwest::redirect::Policy::none());

    if let Some(timeout) = state.options.connect_timeout {
        builder = builder.connect_timeout(timeout);
    }
    if let Some(timeout) = state.options.receive_timeout {
        builder = builder.timeout(timeout);
    }
    if let Some(proxy) = &state.options.proxy {
        let mut p = reqwest::Proxy::all(proxy).map_err(|e| RequestError::Network(e.to_string()))?;
        if let (Some(user), Some(pass)) = (&state.options.proxy_username, &state.options.proxy_password) {
            if !user.is_empty() {
                p = p.basic_auth(user, pass);
            }
        }
        builder = builder.proxy(p);
    }
    // 域前置：自定义 DNS 在每次连接时查询轮询表，SNI/证书仍按域名
    builder = builder.dns_resolver(Arc::new(resolver.clone()));
    builder.build().map_err(|e| RequestError::Network(e.to_string()))
}

/// 校验 URL 可解析。域前置在 DNS 解析阶段由 `FrontingResolver` 处理，不改 URL。
fn prepare(url: &str, _state: &NetState) -> Result<(), RequestError> {
    reqwest::Url::parse(url).map(|_| ()).map_err(|e| RequestError::Network(e.to_string()))
}

/// 发送 GET，注入 cookie 并保存响应 cookie。
pub async fn get(client: &reqwest::Client, state: &Arc<Mutex<NetState>>, resolver: &FrontingResolver, url: &str) -> Result<RawResponse, RequestError> {
    {
        let guard = state.lock().map_err(|e| RequestError::Network(e.to_string()))?;
        prepare(url, &guard)?;
        resolver.set_enabled(guard.options.enable_domain_fronting);
    }

    let cookie_header = state.lock().map(|g| g.cookies.header_value()).unwrap_or_default();
    let mut request = client.get(url);
    if !cookie_header.is_empty() {
        request = request.header(reqwest::header::COOKIE, cookie_header);
    }

    let response = request.send().await.map_err(|e| RequestError::Network(e.to_string()))?;
    finish(response, state).await
}

/// 下载二进制内容（图片、种子等）：只注入 cookie，不做业务正文校验。
///
/// 图片常被 302 到别的图床（client 级 redirect 是关的，E-H 页面需要自己读 Location），
/// 这里手动跟最多 `MAX_REDIRECTS` 跳。
pub async fn get_bytes(
    client: &reqwest::Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    url: &str,
) -> Result<(u16, Vec<u8>), RequestError> {
    get_bytes_with_referer(client, state, resolver, url, None).await
}

/// 带 `Referer` 的二进制下载：E-H 的图床会拒绝没有 referer 的直链。
pub async fn get_bytes_with_referer(
    client: &reqwest::Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    url: &str,
    referer: Option<&str>,
) -> Result<(u16, Vec<u8>), RequestError> {
    const MAX_REDIRECTS: usize = 5;
    let mut current = url.to_string();

    for _ in 0..=MAX_REDIRECTS {
        {
            let guard = state.lock().map_err(|e| RequestError::Network(e.to_string()))?;
            prepare(&current, &guard)?;
            resolver.set_enabled(guard.options.enable_domain_fronting);
        }
        let cookie_header = state.lock().map(|g| g.cookies.header_value()).unwrap_or_default();
        let mut request = client.get(&current);
        if !cookie_header.is_empty() {
            request = request.header(reqwest::header::COOKIE, cookie_header);
        }
        if let Some(referer) = referer {
            request = request.header(reqwest::header::REFERER, referer);
        }
        let response = request.send().await.map_err(|e| RequestError::Network(e.to_string()))?;
        let status = response.status().as_u16();

        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            let location = response.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).map(str::to_string);
            let Some(location) = location else {
                return Err(RequestError::Network(format!("redirect {status} without Location")));
            };
            current = absolute_url(&current, &location)?;
            continue;
        }
        if status == 403 {
            return Err(RequestError::Eh(EhSiteException::new(EhSiteExceptionKind::Cloudflare, "cloudflare403").no_pause()));
        }

        let bytes = response.bytes().await.map_err(|e| RequestError::Network(e.to_string()))?;
        return Ok((status, bytes.to_vec()));
    }

    Err(RequestError::Network(format!("too many redirects: {url}")))
}

/// 把 Location 解析成绝对地址
fn absolute_url(base: &str, location: &str) -> Result<String, RequestError> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_string());
    }
    let base = reqwest::Url::parse(base).map_err(|e| RequestError::Network(e.to_string()))?;
    base.join(location).map(|u| u.to_string()).map_err(|e| RequestError::Network(e.to_string()))
}

/// 发送 POST 表单。
pub async fn post_form(
    client: &reqwest::Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    url: &str,
    form: &[(String, String)],
) -> Result<RawResponse, RequestError> {
    {
        let guard = state.lock().map_err(|e| RequestError::Network(e.to_string()))?;
        prepare(url, &guard)?;
        resolver.set_enabled(guard.options.enable_domain_fronting);
    }

    let cookie_header = state.lock().map(|g| g.cookies.header_value()).unwrap_or_default();
    let mut request = client.post(url).form(form);
    if !cookie_header.is_empty() {
        request = request.header(reqwest::header::COOKIE, cookie_header);
    }
    let response = request.send().await.map_err(|e| RequestError::Network(e.to_string()))?;
    finish(response, state).await
}

async fn finish(response: reqwest::Response, state: &Arc<Mutex<NetState>>) -> Result<RawResponse, RequestError> {
    let status = response.status().as_u16();
    let location = response.headers().get(reqwest::header::LOCATION).and_then(|v| v.to_str().ok()).map(str::to_string);
    let set_cookie: Vec<String> = response
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(str::to_string)
        .collect();
    let body = response.text().await.map_err(|e| RequestError::Network(e.to_string()))?;

    if let Ok(mut guard) = state.lock() {
        guard.last_set_cookie = set_cookie.clone();
        // 只保留 name/value
        guard.cookies.store(set_cookie.iter().filter_map(|s| Cookie::from_set_cookie(s)));
    }

    // 403 = Cloudflare 拦截，在 E-H 域名上出现
    if status == 403 {
        return Err(RequestError::Eh(EhSiteException::new(EhSiteExceptionKind::Cloudflare, "cloudflare403").no_pause()));
    }
    // 200 时检查业务错误正文
    if status == 200 {
        check_eh_response_body(&body)?;
    }

    Ok(RawResponse { status, location, body, set_cookie })
}

/// 供 UI 展示的站点基址
pub fn index_url(is_ex: bool) -> &'static str {
    if is_ex {
        consts::EX_INDEX
    } else {
        consts::EH_INDEX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_toggles_and_shares_provider() {
        let resolver = FrontingResolver::new();
        assert!(!resolver.is_enabled());
        resolver.set_enabled(true);
        assert!(resolver.is_enabled());

        let state = NetState::new(ClientOptions { enable_domain_fronting: true, ..Default::default() });
        let shared = FrontingResolver::with_provider(state.ip_provider.clone());
        assert!(shared.provider.lock().unwrap().supports("e-hentai.org"));
    }

    #[test]
    fn client_builds_with_proxy_and_fronting() {
        let options = ClientOptions {
            proxy: Some("http://127.0.0.1:10808".into()),
            enable_domain_fronting: true,
            ..Default::default()
        };
        let state = NetState::new(options);
        let resolver = FrontingResolver::with_provider(state.ip_provider.clone());
        resolver.set_enabled(true);
        assert!(build_client(&state, &resolver).is_ok());
    }

    #[test]
    fn absolute_url_resolves_relative() {
        assert_eq!(super::absolute_url("https://e-hentai.org/a/b", "/img/1.jpg").unwrap(), "https://e-hentai.org/img/1.jpg");
        assert_eq!(
            super::absolute_url("https://e-hentai.org/a/b", "https://ehgt.org/x.jpg").unwrap(),
            "https://ehgt.org/x.jpg"
        );
        assert_eq!(super::absolute_url("https://e-hentai.org/a/b", "c.jpg").unwrap(), "https://e-hentai.org/a/c.jpg");
    }

    /// 域前置接线：关闭时走系统解析；开启后按 host 命中 IP 池并跳过冷却 IP；池外 host 不受影响
    #[tokio::test]
    async fn resolver_hits_ip_provider_per_host() {
        use reqwest::dns::{Name, Resolve};

        let mut provider = IpProvider::new();
        // 让首个 IP 进入冷却，验证轮询确实在生效
        provider.add_unavailable_ip("e-hentai.org", "172.66.132.196");
        let resolver = FrontingResolver::with_provider(Arc::new(Mutex::new(provider)));

        // 关闭时走系统解析（用 localhost，避免测试环境没有 DNS）
        let name: Name = "localhost".parse().unwrap();
        assert!(!resolver.resolve(name).await.unwrap().collect::<Vec<_>>().is_empty(), "关闭时走系统解析");

        resolver.set_enabled(true);
        let name: Name = "e-hentai.org".parse().unwrap();
        let addrs: Vec<_> = resolver.resolve(name).await.unwrap().collect();
        assert_eq!(addrs.len(), 1);
        assert_eq!(addrs[0].ip().to_string(), "172.66.140.62", "应跳过冷却中的 IP");
        assert_eq!(addrs[0].port(), 443);

        // 池外的 host 仍走系统解析，不会被前置影响
        let name: Name = "localhost".parse().unwrap();
        assert!(!resolver.resolve(name).await.unwrap().collect::<Vec<_>>().is_empty());
    }

    /// 并发前置多个 host：各自的 IP 不会互相覆盖（旧实现在这里是 bug）
    #[tokio::test]
    async fn concurrent_hosts_do_not_clobber_each_other() {
        use reqwest::dns::{Name, Resolve};

        let state = NetState::new(ClientOptions { enable_domain_fronting: true, ..Default::default() });
        let resolver = FrontingResolver::with_provider(state.ip_provider.clone());
        resolver.set_enabled(true);

        let a = resolver.resolve("e-hentai.org".parse::<Name>().unwrap());
        let b = resolver.resolve("exhentai.org".parse::<Name>().unwrap());
        let (a, b) = tokio::join!(a, b);
        let a: Vec<_> = a.unwrap().collect();
        let b: Vec<_> = b.unwrap().collect();

        assert!(a[0].ip().to_string().starts_with("172.66."), "e-hentai 用自己的池: {}", a[0].ip());
        assert!(b[0].ip().to_string().starts_with("178.175."), "exhentai 用自己的池: {}", b[0].ip());
    }
}
