//! 网络设置（对应 `setting/network_setting.dart`）。

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyType {
    #[default]
    System,
    Http,
    Socks5,
    Socks4,
    Direct,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkSetting {
    #[serde(rename = "proxyType", default)]
    pub proxy_type: ProxyType,
    #[serde(rename = "proxyAddress", default = "default_proxy_address")]
    pub proxy_address: String,
    #[serde(rename = "proxyUsername", default)]
    pub proxy_username: Option<String>,
    #[serde(rename = "proxyPassword", default)]
    pub proxy_password: Option<String>,
    #[serde(rename = "enableDomainFronting", default)]
    pub enable_domain_fronting: bool,
    #[serde(rename = "connectTimeout", default = "default_timeout")]
    pub connect_timeout: u64,
    #[serde(rename = "receiveTimeout", default = "default_timeout")]
    pub receive_timeout: u64,
    #[serde(rename = "pageCacheMaxAge", default = "default_cache_age")]
    pub page_cache_max_age: u64,
}

fn default_proxy_address() -> String {
    "localhost:1080".to_string()
}

fn default_timeout() -> u64 {
    6000
}

fn default_cache_age() -> u64 {
    86_400_000
}

impl Default for NetworkSetting {
    fn default() -> Self {
        Self {
            proxy_type: ProxyType::System,
            proxy_address: default_proxy_address(),
            proxy_username: None,
            proxy_password: None,
            enable_domain_fronting: false,
            connect_timeout: default_timeout(),
            receive_timeout: default_timeout(),
            page_cache_max_age: default_cache_age(),
        }
    }
}

impl NetworkSetting {
    /// `NetworkSetting.host2IPs`：域前置用的 IP 池
    pub const HOST_TO_IPS: [(&'static str, &'static [&'static str]); 5] = [
        ("e-hentai.org", &["172.66.132.196", "172.66.140.62"]),
        (
            "exhentai.org",
            &[
                "178.175.128.251",
                "178.175.128.252",
                "178.175.128.253",
                "178.175.128.254",
                "178.175.129.251",
                "178.175.129.252",
                "178.175.129.253",
                "178.175.129.254",
                "178.175.132.19",
                "178.175.132.20",
                "178.175.132.21",
                "178.175.132.22",
            ],
        ),
        ("upld.e-hentai.org", &["95.211.208.236", "89.149.221.236"]),
        (
            "api.e-hentai.org",
            &["37.48.92.161", "212.7.202.51", "5.79.104.110", "37.48.81.204", "212.7.200.104"],
        ),
        ("forums.e-hentai.org", &["172.66.132.196", "172.66.140.62"]),
    ];

    /// `allHostAndIPs`：判断 host 是否属于 E-H，决定要不要带 cookie / 走域前置
    pub fn is_eh_host(&self, host: &str) -> bool {
        Self::HOST_TO_IPS.iter().any(|(h, ips)| *h == host || ips.contains(&host))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_dart() {
        let s = NetworkSetting::default();
        assert_eq!(s.proxy_address, "localhost:1080");
        assert_eq!(s.connect_timeout, 6000);
        assert!(!s.enable_domain_fronting);
    }

    #[test]
    fn deserializes_dart_json() {
        let json = r#"{"proxyType":"http","proxyAddress":"127.0.0.1:8888","proxyUsername":"u","proxyPassword":"p","enableDomainFronting":true,"connectTimeout":9000,"receiveTimeout":8000,"pageCacheMaxAge":100}"#;
        let s: NetworkSetting = serde_json::from_str(json).unwrap();
        assert_eq!(s.proxy_type, ProxyType::Http);
        assert!(s.enable_domain_fronting);
        assert!(s.is_eh_host("exhentai.org"));
        assert!(s.is_eh_host("172.66.132.196"));
        assert!(!s.is_eh_host("example.com"));
    }
}
