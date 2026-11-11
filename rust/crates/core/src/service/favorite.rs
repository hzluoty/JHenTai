//! 收藏：加入 / 移除收藏夹（对应 Dart 的 `requestAddFavorite` / `requestRemoveFavorite`）。
//!
//! E-H 的接口是同一个 `gallerypopups.php?gid=..&t=..&act=addfav`：
//! 加收藏用 `favcat=<0..9>`，移除用 `favcat=favdel`。

use std::sync::{Arc, Mutex};

use crate::net::http::{post_form, Client};
use crate::net::{consts, FrontingResolver, NetState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoriteError {
    /// 未登录（cookie 缺 ipb_member_id，或被 302 到登录页）
    LoginRequired,
    /// 被站点拒绝（配额、封禁等）
    Rejected(String),
    Network(String),
}

impl std::fmt::Display for FavoriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoginRequired => write!(f, "loginRequired"),
            Self::Rejected(msg) => write!(f, "{msg}"),
            Self::Network(e) => write!(f, "network: {e}"),
        }
    }
}

impl std::error::Error for FavoriteError {}

/// 收藏弹窗地址（`act=addfav` 同时用于加和删）
pub fn favorite_url(is_ex: bool, gid: i64, token: &str) -> String {
    let base = if is_ex { consts::EX_POPUP } else { consts::EH_POPUP };
    format!("{base}?gid={gid}&t={token}&act=addfav")
}

/// 收藏夹槽位：0..=9 是十个收藏夹，`favdel` 表示移除
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FavoriteAction {
    Add { slot: i64, note: String },
    Remove,
}

impl FavoriteAction {
    fn form(&self) -> Vec<(String, String)> {
        match self {
            Self::Add { slot, note } => vec![
                ("favcat".to_string(), slot.to_string()),
                ("favnote".to_string(), note.clone()),
                ("apply".to_string(), "Add to Favorites".to_string()),
                ("update".to_string(), "1".to_string()),
            ],
            Self::Remove => vec![
                ("favcat".to_string(), "favdel".to_string()),
                ("favnote".to_string(), String::new()),
                ("apply".to_string(), "Apply Changes".to_string()),
                ("update".to_string(), "1".to_string()),
            ],
        }
    }
}

/// 执行收藏操作。成功后返回 `()`；需要登录时返回 `LoginRequired`。
pub async fn apply(
    client: &Client,
    state: &Arc<Mutex<NetState>>,
    resolver: &FrontingResolver,
    is_ex: bool,
    gid: i64,
    token: &str,
    action: FavoriteAction,
) -> Result<(), FavoriteError> {
    // 本地先判一次，省一次往返
    let logged_in = state.lock().map(|n| n.cookies.has_logged_in()).unwrap_or(false);
    if !logged_in {
        return Err(FavoriteError::LoginRequired);
    }

    let url = favorite_url(is_ex, gid, token);
    let form = action.form();
    let response = post_form(client, state, resolver, &url, &form)
        .await
        .map_err(|e| FavoriteError::Network(e.to_string()))?;

    // 未登录时 E-H 会 302 到 bounce_login；也有的路径直接给登录页正文
    if response.status == 302 || response.body.contains("requires you to log") {
        return Err(FavoriteError::LoginRequired);
    }
    if response.body.contains("You have been temporarily banned") {
        return Err(FavoriteError::Rejected("banned".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorite_url_matches_dart() {
        assert_eq!(
            favorite_url(false, 42, "aaaaaaaaaa"),
            "https://e-hentai.org/gallerypopups.php?gid=42&t=aaaaaaaaaa&act=addfav"
        );
        assert!(favorite_url(true, 42, "aaaaaaaaaa").starts_with("https://exhentai.org/"));
    }

    #[test]
    fn add_form_matches_dart_fields() {
        let form = FavoriteAction::Add { slot: 3, note: "later".into() }.form();
        assert_eq!(form[0], ("favcat".to_string(), "3".to_string()));
        assert_eq!(form[1], ("favnote".to_string(), "later".to_string()));
        assert_eq!(form[2].0, "apply");
        assert_eq!(form[3], ("update".to_string(), "1".to_string()));
    }

    #[test]
    fn remove_uses_favdel() {
        let form = FavoriteAction::Remove.form();
        assert_eq!(form[0], ("favcat".to_string(), "favdel".to_string()));
        assert_eq!(form[2], ("apply".to_string(), "Apply Changes".to_string()));
    }

    #[tokio::test]
    async fn refuses_without_login() {
        let state = NetState::shared(crate::net::ClientOptions::default());
        let resolver = FrontingResolver::new();
        let client = crate::net::build_client(&state.lock().unwrap(), &resolver).unwrap();
        let result = apply(&client, &state, &resolver, false, 1, "aaaaaaaaaa", FavoriteAction::Remove).await;
        assert_eq!(result, Err(FavoriteError::LoginRequired), "没有 cookie 时不该发请求");
    }
}
