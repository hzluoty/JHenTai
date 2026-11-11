//! 设置页 controller：站点、网络、阅读、下载、安全（PIN）、关于。
//!
//! 设置全部落在 `local_config`（`config::ConfigStore`），字段名与 Dart 版一致。
//! 许可证：Slint Royalty-free 要求在「关于」页展示 `AboutSlint`。

use std::rc::Rc;

use slint::ComponentHandle;

use jhentai_core::config::settings::{DownloadSetting, EhSetting, ReadSetting, SecuritySetting};
use jhentai_core::config::{keys, network::NetworkSetting};

/// 设置内存态（UI 线程读写；持久化走 `AppController::store`）
#[derive(Debug, Clone, Default)]
pub struct SettingsState {
    pub eh: EhSetting,
    pub network: NetworkSetting,
    pub read: ReadSetting,
    pub download: DownloadSetting,
    pub security: SecuritySetting,
    /// 应用锁是否已解锁（进程生命周期内有效）
    pub unlocked: bool,
    /// 界面语言
    pub lang: crate::i18n::Lang,
}

impl crate::AppController {
    /// 启动时从 `local_config` 读设置（失败用默认值）
    pub fn load_settings(self: &Rc<Self>) {
        let mut state = self.settings.borrow_mut();
        if let Ok(store) = self.store.lock() {
            if let Some(value) = jhentai_core::config::read_json::<EhSetting>(store.as_ref(), keys::EH_SETTING) {
                state.eh = value;
            }
            if let Some(value) = jhentai_core::config::read_json::<NetworkSetting>(store.as_ref(), keys::NETWORK_SETTING) {
                state.network = value;
            }
            if let Some(value) = jhentai_core::config::read_json::<ReadSetting>(store.as_ref(), keys::READ_SETTING) {
                state.read = value;
            }
            if let Some(value) = jhentai_core::config::read_json::<DownloadSetting>(store.as_ref(), keys::DOWNLOAD_SETTING) {
                state.download = value;
            }
            if let Some(value) = jhentai_core::config::read_json::<SecuritySetting>(store.as_ref(), keys::SECURITY_SETTING) {
                state.security = value;
            }
            if let Some(code) = jhentai_core::config::read_string(store.as_ref(), keys::LANGUAGE, "") {
                state.lang = crate::i18n::Lang::from_code(&code);
            }
        }
        drop(state);
        self.apply_settings_to_ui();
    }

    fn persist<T: serde::Serialize>(&self, key: &str, value: &T) {
        if let Ok(mut store) = self.store.lock() {
            jhentai_core::config::write_json(store.as_mut(), key, value);
        }
    }

    /// 把设置渲染到 UI
    pub fn apply_settings_to_ui(self: &Rc<Self>) {
        let state = self.settings.borrow();
        self.window.set_setting_is_ex(state.eh.is_ex());
        self.window.set_setting_proxy_address(state.network.proxy_address.clone().into());
        self.window.set_setting_proxy_type(match state.network.proxy_type {
            jhentai_core::config::network::ProxyType::System => "system",
            jhentai_core::config::network::ProxyType::Http => "http",
            jhentai_core::config::network::ProxyType::Socks5 => "socks5",
            jhentai_core::config::network::ProxyType::Socks4 => "socks4",
            jhentai_core::config::network::ProxyType::Direct => "direct",
        }
        .into());
        self.window.set_setting_domain_fronting(state.network.enable_domain_fronting);
        self.window.set_setting_read_direction(state.read.read_direction.label().into());
        self.window.set_setting_keep_awake(state.read.keep_screen_awake_when_reading);
        self.window.set_setting_auto_turn(self.auto_turn_enabled.get());
        self.window.set_setting_auto_interval(format!("{:.0}s", state.read.auto_mode_interval).into());
        self.window.set_setting_download_concurrency(state.download.download_task_concurrency as i32);
        self.window.set_setting_download_original(state.download.download_original_image);
        self.window.set_setting_pin_enabled(state.security.is_locked_enabled());
        self.window.set_setting_persistent(self.store_is_persistent);
        self.window.set_setting_version(env!("CARGO_PKG_VERSION").into());
        self.window.set_setting_language(state.lang.code().into());

        // 底部 tab 文案跟着语言走
        let titles: Vec<slint::SharedString> = ["frontPage", "popular", "ranklist", "favorite", "watched", "history"]
            .iter()
            .map(|key| crate::i18n::t(state.lang, key).into())
            .collect();
        self.window.set_tab_titles(Rc::new(slint::VecModel::from(titles)).into());
        self.window.set_ui_read(crate::i18n::t(state.lang, "read").into());
        self.window.set_ui_download(crate::i18n::t(state.lang, "download").into());
        self.window.set_ui_back(crate::i18n::t(state.lang, "back").into());
        self.window.set_ui_tags(crate::i18n::t(state.lang, "tags").into());
        self.window.set_ui_comments(crate::i18n::t(state.lang, "comments").into());
        self.window.set_ui_about(crate::i18n::t(state.lang, "about").into());
        self.window.set_ui_settings(crate::i18n::t(state.lang, "settings").into());
        self.window.set_ui_language(crate::i18n::t(state.lang, "language").into());
        self.window.set_ui_loading(crate::i18n::t(state.lang, "loading").into());
        self.window.set_ui_retry(crate::i18n::t(state.lang, "retry").into());
        self.window.set_ui_no_galleries(crate::i18n::t(state.lang, "noGalleries").into());
        self.window.set_ui_no_downloads(crate::i18n::t(state.lang, "noDownloads").into());
        self.window.set_ui_thumbnails(crate::i18n::t(state.lang, "thumbnails").into());
        self.window.set_ui_sign_in(crate::i18n::t(state.lang, "signIn").into());
        self.window.set_ui_sign_out(crate::i18n::t(state.lang, "signOut").into());
        self.window.set_ui_expunged(crate::i18n::t(state.lang, "expunged").into());
        self.window.set_ui_total_items(crate::i18n::t(state.lang, "total").into());
    }

    /// 站点切换（EH ↔ EX）
    pub fn toggle_site(self: &Rc<Self>) {
        let is_ex = {
            let mut state = self.settings.borrow_mut();
            let next = !state.eh.is_ex();
            state.eh.set_site(next);
            next
        };
        {
            let state = self.settings.borrow();
            self.persist(keys::EH_SETTING, &state.eh);
        }
        self.window.set_setting_is_ex(is_ex);
    }

    /// 网络：代理地址 + 域前置开关（改完立即重建 client）
    pub fn set_proxy(self: &Rc<Self>, address: String) {
        {
            let mut state = self.settings.borrow_mut();
            state.network.proxy_address = address;
            // 有地址就算 HTTP 代理，空则回系统
            state.network.proxy_type = if state.network.proxy_address.trim().is_empty() {
                jhentai_core::config::network::ProxyType::System
            } else {
                jhentai_core::config::network::ProxyType::Http
            };
        }
        self.persist(keys::NETWORK_SETTING, &self.settings.borrow().network);
        self.rebuild_network();
        self.apply_settings_to_ui();
    }

    pub fn toggle_domain_fronting(self: &Rc<Self>) {
        {
            let mut state = self.settings.borrow_mut();
            state.network.enable_domain_fronting = !state.network.enable_domain_fronting;
            if let Ok(mut net) = self.net.lock() {
                net.options.enable_domain_fronting = state.network.enable_domain_fronting;
            }
        }
        self.persist(keys::NETWORK_SETTING, &self.settings.borrow().network);
        self.apply_settings_to_ui();
    }

    /// 切换界面语言（6 种）
    pub fn cycle_language(self: &Rc<Self>) {
        {
            let mut state = self.settings.borrow_mut();
            state.lang = state.lang.next();
        }
        {
            let lang = self.settings.borrow().lang;
            if let Ok(mut store) = self.store.lock() {
                jhentai_core::config::write_string(store.as_mut(), keys::LANGUAGE, "", lang.code());
            }
        }
        self.apply_settings_to_ui();
    }

    /// 自动翻页开关
    pub fn toggle_auto_turn(self: &Rc<Self>) {
        let enabled = !self.auto_turn_enabled.get();
        self.auto_turn_enabled.set(enabled);
        *self.auto_turn.lock().unwrap() = Some(std::time::Instant::now());
        self.window.set_setting_auto_turn(enabled);
    }

    /// 自动翻页间隔循环：1 → 2 → 3 → 5 → 10 → 1
    pub fn cycle_auto_interval(self: &Rc<Self>) {
        {
            let mut state = self.settings.borrow_mut();
            state.read.auto_mode_interval = match state.read.auto_mode_interval as i64 {
                1 => 2.0,
                2 => 3.0,
                3 => 5.0,
                5 => 10.0,
                _ => 1.0,
            };
        }
        self.persist(keys::READ_SETTING, &self.settings.borrow().read);
        self.apply_settings_to_ui();
    }

    /// 阅读方向三态循环
    pub fn cycle_read_direction(self: &Rc<Self>) {
        {
            let mut state = self.settings.borrow_mut();
            state.read.read_direction.cycle();
        }
        self.persist(keys::READ_SETTING, &self.settings.borrow().read);
        self.apply_settings_to_ui();
    }

    pub fn toggle_keep_awake(self: &Rc<Self>) {
        {
            let mut state = self.settings.borrow_mut();
            state.read.keep_screen_awake_when_reading = !state.read.keep_screen_awake_when_reading;
        }
        self.persist(keys::READ_SETTING, &self.settings.borrow().read);
        self.apply_settings_to_ui();
    }

    /// 下载并发 +1（1..=8 循环）
    pub fn bump_download_concurrency(self: &Rc<Self>) {
        {
            let mut state = self.settings.borrow_mut();
            state.download.download_task_concurrency = if state.download.download_task_concurrency >= 8 {
                1
            } else {
                state.download.download_task_concurrency + 1
            };
        }
        {
            let concurrency = self.settings.borrow().download.download_task_concurrency as usize;
            let mut downloads = self.downloads.lock().unwrap();
            downloads.concurrency = concurrency;
        }
        self.persist(keys::DOWNLOAD_SETTING, &self.settings.borrow().download);
        self.apply_settings_to_ui();
    }

    pub fn toggle_download_original(self: &Rc<Self>) {
        {
            let mut state = self.settings.borrow_mut();
            state.download.download_original_image = !state.download.download_original_image;
        }
        self.persist(keys::DOWNLOAD_SETTING, &self.settings.borrow().download);
        self.apply_settings_to_ui();
    }

    /// 设置 / 清除 PIN
    pub fn set_pin(self: &Rc<Self>, pin: Option<String>) {
        {
            let mut state = self.settings.borrow_mut();
            state.security.pin = pin;
            state.unlocked = true;
        }
        self.persist(keys::SECURITY_SETTING, &self.settings.borrow().security);
        self.apply_settings_to_ui();
    }

    /// 校验 PIN；成功则解锁
    pub fn unlock(self: &Rc<Self>, input: &str) -> bool {
        let ok = self.settings.borrow().security.verify(input);
        if ok {
            self.settings.borrow_mut().unlocked = true;
            self.window.set_locked(false);
        }
        ok
    }

    /// 启动时是否需要显示锁屏
    pub fn is_locked(self: &Rc<Self>) -> bool {
        let state = self.settings.borrow();
        state.security.is_locked_enabled() && !state.unlocked
    }

    /// 用当前设置重建 reqwest client（改代理/域前置后调用）
    pub fn rebuild_network(self: &Rc<Self>) {
        let network = self.settings.borrow().network.clone();
        // 系统代理只能从平台层读（reqwest 在 Android 不会自己读）
        let system_proxy = jhentai_core::net::normalize_proxy(self.platform.system_proxy());
        let options = jhentai_core::net::ClientOptions::from_setting(&network, system_proxy.as_deref());
        if let Ok(mut net) = self.net.lock() {
            net.options = options.clone();
        }
        if let Ok(client) = jhentai_core::net::build_client(&self.net.lock().unwrap(), &self.resolver) {
            // client 存在 Mutex 里，支持代理热替换
            *self.client.lock().unwrap() = client;
        }
    }
}

impl crate::AppController {
    /// 账号状态同步到 UI
    pub fn apply_account_to_ui(self: &Rc<Self>) {
        let (logged_in, name) = {
            let guard = self.net.lock().unwrap();
            let logged_in = guard.cookies.has_logged_in();
            (logged_in, guard.cookies.ipb_member_id().map(|id| format!("member #{id}")).unwrap_or_default())
        };
        self.window.set_account_logged_in(logged_in);
        self.window.set_account_name(name.into());
    }

    /// 账号密码登录（E-H 的 forums 登录页）
    pub fn login(self: &Rc<Self>, user_name: String, password: String) {
        if user_name.trim().is_empty() || password.is_empty() {
            self.window.set_account_status("username and password required".into());
            return;
        }
        self.window.set_account_status("signing in…".into());

        let client = self.client.lock().unwrap().clone();
        let net = self.net.clone();
        let resolver = self.resolver.clone();
        let store = self.store.clone();
        let window_weak = self.window.as_weak();
        let ui_events = self.ui_events.clone();

        self.runtime.spawn(async move {
            // 网络登录只碰 NetState（Send），落盘在 UI 线程做，避免 MutexGuard 跨 await
            let result = jhentai_core::service::account::login_net(&client, &net, &resolver, &user_name, &password).await;
            let message = match &result {
                Ok(()) => "signed in".to_string(),
                Err(e) => e.to_string(),
            };
            let _ = slint::invoke_from_event_loop(move || {
                if result.is_ok() {
                    if let (Ok(guard), Ok(mut store)) = (net.lock(), store.lock()) {
                        let json = guard.cookies.to_json();
                        jhentai_core::config::write_string(store.as_mut(), jhentai_core::config::keys::EH_COOKIE, "", &json);
                    }
                }
                if let Some(window) = window_weak.upgrade() {
                    window.set_account_status(message.into());
                }
                ui_events.request_account();
            });
        });
    }

    /// 导入浏览器 cookie 串
    pub fn import_cookies(self: &Rc<Self>, raw: String) {
        let count = {
            let Ok(mut store) = self.store.lock() else {
                return;
            };
            jhentai_core::service::account::import_cookies(&self.net, store.as_mut(), &raw)
        };
        self.window.set_account_status(format!("imported {count} cookies").into());
        self.apply_account_to_ui();
    }

    pub fn logout(self: &Rc<Self>) {
        if let Ok(mut store) = self.store.lock() {
            jhentai_core::service::account::logout(&self.net, store.as_mut());
        }
        self.window.set_account_status("signed out".into());
        self.apply_account_to_ui();
    }

    /// 启动时恢复 cookie
    pub fn restore_account(self: &Rc<Self>) {
        let logged_in = {
            let Ok(store) = self.store.lock() else {
                return;
            };
            jhentai_core::service::account::restore_cookies(&self.net, store.as_ref())
        };
        let _ = logged_in;
        self.apply_account_to_ui();
    }
}

impl crate::AppController {
    /// 新增一条「标题包含」屏蔽规则
    pub fn add_block_rule(self: &Rc<Self>, keyword: String) {
        let keyword = keyword.trim().to_string();
        if keyword.is_empty() {
            return;
        }
        let Some(path) = self.db_path.clone() else {
            return;
        };
        let Ok(conn) = jhentai_core::db::open(&path) else {
            return;
        };
        let rule = jhentai_core::service::block_rule::BlockRule {
            group_id: "default".into(),
            target: jhentai_core::service::block_rule::BlockTarget::Gallery,
            attribute: jhentai_core::service::block_rule::BlockAttribute::Title,
            pattern: jhentai_core::service::block_rule::BlockPattern::Contain,
            expression: keyword,
        };
        if jhentai_core::service::block_rule::insert_rule(&conn, &rule).is_ok() {
            drop(conn);
            self.load_block_rules();
            self.refresh_block_rules_ui();
        }
    }

    /// 删除第 index 条规则（按设置页展示顺序）
    pub fn delete_block_rule(self: &Rc<Self>, index: i32) {
        let Some(path) = self.db_path.clone() else {
            return;
        };
        let Ok(conn) = jhentai_core::db::open(&path) else {
            return;
        };
        let Ok(rules) = jhentai_core::service::block_rule::all_rules(&conn) else {
            return;
        };
        let Some(stored) = rules.get(index.max(0) as usize) else {
            return;
        };
        if jhentai_core::service::block_rule::delete_rule(&conn, stored.id).is_ok() {
            drop(conn);
            self.load_block_rules();
            self.refresh_block_rules_ui();
        }
    }

    /// 规则列表同步到 UI
    pub fn refresh_block_rules_ui(self: &Rc<Self>) {
        let Some(path) = self.db_path.clone() else {
            self.window.set_block_rules(Rc::new(slint::VecModel::from(Vec::<slint::SharedString>::new())).into());
            return;
        };
        let items: Vec<slint::SharedString> = jhentai_core::db::open(&path)
            .ok()
            .and_then(|conn| jhentai_core::service::block_rule::all_rules(&conn).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|stored| jhentai_core::service::block_rule::describe(&stored.rule).into())
            .collect();
        self.window.set_block_rules(Rc::new(slint::VecModel::from(items)).into());
    }

    /// 从 `block_rule` 表加载屏蔽规则（启动时与规则编辑后调用）
    pub fn load_block_rules(self: &Rc<Self>) {
        let Some(path) = self.db_path.clone() else {
            return;
        };
        let rules = match jhentai_core::db::open(&path) {
            Ok(conn) => jhentai_core::service::block_rule::load_rules(&conn).unwrap_or_default(),
            Err(_) => return,
        };
        if let Ok(mut slot) = self.block_rules.lock() {
            *slot = rules;
        }
        self.refresh_block_rules_ui();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let state = SettingsState::default();
        assert_eq!(state.eh.site, "EH");
        assert!(!state.eh.is_ex());
        assert!(state.read.keep_screen_awake_when_reading);
        assert_eq!(state.download.download_task_concurrency, 3);
        assert!(!state.security.is_locked_enabled());
    }

    #[test]
    fn pin_gating() {
        let mut state = SettingsState::default();
        state.security.pin = Some("4321".into());
        assert!(!state.unlocked);
        assert!(state.security.is_locked_enabled());
        assert!(state.security.verify("4321"));
        assert!(!state.security.verify("1234"));
    }
}
