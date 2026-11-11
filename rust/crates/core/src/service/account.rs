//! 账号：登录、cookie 导入、登出（对应 `service/eh_account_service.dart` 的一部分）。

use std::sync::{Arc, Mutex};

use crate::config::keys;
use crate::config::ConfigStore;
use crate::net::http::{post_form, Client};
use crate::net::{consts, Cookie, CookieManager, FrontingResolver, NetState};
use crate::parser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginError {
    /// 账号或密码错误
    UserNameOrPasswordMismatch,
    /// 需要验证码
    NeedCaptcha,
    /// 网络错误
    Network(String),
}

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UserNameOrPasswordMismatch => write!(f, "userNameOrPasswordMismatch"),
            Self::NeedCaptcha => write!(f, "needCaptcha"),
            Self::Network(e) => write!(f, "network: {e}"),
        }
    }
}

impl std::error::Error for LoginError {}

/// 用账号密码登录 E-H / EX，成功后把 cookie 写进 `NetState` 与 `local_config`。
///
/// `is_ex` 只影响后续使用的域名；登录本身在 forums.e-hentai.org 完成。
pub async fn login(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    store: &mut dyn ConfigStore,
    user_name: &str,
    pass_hash: &str,
) -> Result<(), LoginError> {
    let form = vec![
        ("CookieDate".to_string(), "1".to_string()),
        ("b".to_string(), "d".to_string()),
        ("bt".to_string(), "1-1".to_string()),
        ("UserName".to_string(), user_name.to_string()),
        ("PassWord".to_string(), pass_hash.to_string()),
        ("submit".to_string(), "Log me in".to_string()),
    ];

    let response = post_form(client, state, resolver, consts::EH_FORUMS, &form)
        .await
        .map_err(|e| LoginError::Network(e.to_string()))?;

    let set_cookie = response.set_cookie.clone();
    let (member_id, pass_hash) = parser::login_page_user_info(&response.body, &set_cookie).map_err(|key| match key.as_str() {
        "needCaptcha" => LoginError::NeedCaptcha,
        _ => LoginError::UserNameOrPasswordMismatch,
    })?;

    // 登录成功：把 cookie 落盘（`igneous` 只有 EX 会给，缺了就是拿不到 EX 权限）
    let mut guard = state.lock().map_err(|e| LoginError::Network(e.to_string()))?;
    guard.cookies.store([
        Cookie::new("ipb_member_id", member_id.to_string()),
        Cookie::new("ipb_pass_hash", pass_hash),
    ]);
    let json = guard.cookies.to_json();
    drop(guard);
    crate::config::write_string(store, keys::EH_COOKIE, "", &json);
    Ok(())
}

/// 导入浏览器里复制出来的 cookie 字符串（`a=1; b=2`）
pub fn import_cookies(state: &Arc<Mutex<NetState>>, store: &mut dyn ConfigStore, raw: &str) -> usize {
    let parsed: Vec<Cookie> = raw
        .split(';')
        .filter_map(|part| {
            let (name, value) = part.split_once('=')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            Some(Cookie::new(name, value.trim()))
        })
        .collect();

    let count = parsed.len();
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };
    guard.cookies.store(parsed);
    let json = guard.cookies.to_json();
    drop(guard);
    crate::config::write_string(store, keys::EH_COOKIE, "", &json);
    count
}

/// 登出：清空 cookie 并删除持久化记录
pub fn logout(state: &Arc<Mutex<NetState>>, store: &mut dyn ConfigStore) {
    if let Ok(mut guard) = state.lock() {
        guard.cookies.clear();
    }
    crate::config::write_string(store, keys::EH_COOKIE, "", "[]");
}

/// 启动时从 `local_config` 恢复 cookie
pub fn restore_cookies(state: &Arc<Mutex<NetState>>, store: &dyn ConfigStore) -> bool {
    let Some(json) = crate::config::read_string(store, keys::EH_COOKIE, "") else {
        return false;
    };
    let cookies = CookieManager::from_json(&json);
    let logged_in = cookies.has_logged_in();
    if let Ok(mut guard) = state.lock() {
        guard.cookies = cookies;
    }
    logged_in
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemoryConfigStore;
    use crate::net::ClientOptions;

    fn state() -> Arc<Mutex<NetState>> {
        NetState::shared(ClientOptions::default())
    }

    #[test]
    fn import_cookies_parses_browser_string() {
        let mut store = MemoryConfigStore::new();
        let n = import_cookies(&state(), &mut store, "ipb_member_id=42; ipb_pass_hash=deadbeef; igneous=xyz");
        assert_eq!(n, 3);
        let saved = crate::config::read_string(&store, keys::EH_COOKIE, "").unwrap();
        assert!(saved.contains("ipb_member_id=42"));
        assert!(saved.contains("igneous=xyz"));
    }

    #[test]
    fn restore_reports_login_state() {
        let mut store = MemoryConfigStore::new();
        let st = state();
        assert!(!restore_cookies(&st, &store));

        import_cookies(&st, &mut store, "ipb_member_id=42; ipb_pass_hash=d");
        let fresh = state();
        assert!(restore_cookies(&fresh, &store));
        assert_eq!(fresh.lock().unwrap().cookies.ipb_member_id(), Some("42"));
    }

    #[test]
    fn logout_clears_both_sides() {
        let mut store = MemoryConfigStore::new();
        let st = state();
        import_cookies(&st, &mut store, "ipb_member_id=42");
        logout(&st, &mut store);
        assert!(!st.lock().unwrap().cookies.has_logged_in());
        assert_eq!(crate::config::read_string(&store, keys::EH_COOKIE, "").as_deref(), Some("[]"));
    }

    #[test]
    fn login_error_display_is_i18n_key() {
        assert_eq!(LoginError::NeedCaptcha.to_string(), "needCaptcha");
        assert_eq!(LoginError::UserNameOrPasswordMismatch.to_string(), "userNameOrPasswordMismatch");
    }
}
