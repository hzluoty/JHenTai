//! 文案表（对应 `lib/src/l18n/`）。
//!
//! Dart 版用 GetX 的 `LocaleText` + 6 个语言文件。这里先做同一套 key 的最小实现：
//! Rust 侧查表，UI 通过 `window` 上的属性取值（Slint 的 `@tr()` 需要 .po 工具链，后续可替换）。
//! 未翻译的 key 回退英文，未收录的 key 原样返回，保证不会出现空白文案。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    EnUs,
    ZhCn,
    ZhTw,
    KoKr,
    PtBr,
    RuRu,
}

impl Lang {
    /// 与 Dart `LocaleConsts` 的 key 一致
    pub fn code(&self) -> &'static str {
        match self {
            Self::EnUs => "en_US",
            Self::ZhCn => "zh_CN",
            Self::ZhTw => "zh_TW",
            Self::KoKr => "ko_KR",
            Self::PtBr => "pt_BR",
            Self::RuRu => "ru_RU",
        }
    }

    pub fn from_code(code: &str) -> Self {
        match code {
            "zh_CN" => Self::ZhCn,
            "zh_TW" => Self::ZhTw,
            "ko_KR" => Self::KoKr,
            "pt_BR" => Self::PtBr,
            "ru_RU" => Self::RuRu,
            _ => Self::EnUs,
        }
    }

    pub fn all() -> [Self; 6] {
        [Self::EnUs, Self::ZhCn, Self::ZhTw, Self::KoKr, Self::PtBr, Self::RuRu]
    }

    /// 设置页循环切换
    pub fn next(&self) -> Self {
        let all = Self::all();
        let index = all.iter().position(|l| l == self).unwrap_or(0);
        all[(index + 1) % all.len()]
    }
}

/// 翻译一条文案。`(en, zh_CN, zh_TW, ko_KR, pt_BR, ru_RU)`
fn table(key: &str) -> Option<[&'static str; 6]> {
    Some(match key {
        "frontPage" => ["Front Page", "首页", "首頁", "첫 페이지", "Página inicial", "Главная"],
        "popular" => ["Popular", "热门", "熱門", "인기", "Popular", "Популярное"],
        "ranklist" => ["Ranklist", "排行", "排行", "순위", "Ranking", "Рейтинг"],
        "favorite" => ["Favorites", "收藏", "收藏", "즐겨찾기", "Favoritos", "Избранное"],
        "watched" => ["Watched", "关注", "關注", "관심", "Observados", "Отслеживаемые"],
        "history" => ["History", "历史", "歷史", "기록", "Histórico", "История"],
        "read" => ["Read", "阅读", "閱讀", "읽기", "Ler", "Читать"],
        "download" => ["Download", "下载", "下載", "다운로드", "Baixar", "Скачать"],
        "downloads" => ["Downloads", "下载", "下載", "다운로드", "Downloads", "Загрузки"],
        "settings" => ["Settings", "设置", "設定", "설정", "Configurações", "Настройки"],
        "back" => ["Back", "返回", "返回", "뒤로", "Voltar", "Назад"],
        "refresh" => ["Refresh", "刷新", "重新整理", "새로고침", "Atualizar", "Обновить"],
        "retry" => ["Retry", "重试", "重試", "다시 시도", "Tentar novamente", "Повторить"],
        "loading" => ["Loading…", "加载中…", "載入中…", "불러오는 중…", "Carregando…", "Загрузка…"],
        "noGalleries" => ["No galleries", "没有画廊", "沒有畫廊", "갤러리 없음", "Nenhuma galeria", "Нет галерей"],
        "noDownloads" => ["No downloads yet", "还没有下载", "還沒有下載", "다운로드 없음", "Nenhum download", "Нет загрузок"],
        "tags" => ["Tags", "标签", "標籤", "태그", "Tags", "Теги"],
        "thumbnails" => ["Thumbnails", "缩略图", "縮圖", "썸네일", "Miniaturas", "Миниатюры"],
        "comments" => ["Comments", "评论", "評論", "댓글", "Comentários", "Комментарии"],
        "accountSite" => ["Account & Site", "账号与站点", "帳號與站點", "계정 및 사이트", "Conta e site", "Аккаунт и сайт"],
        "notSignedIn" => [
            "Not signed in (EX & original images need login)",
            "未登录（EX 与下载原图需要登录）",
            "未登入（EX 與下載原圖需要登入）",
            "로그인하지 않음 (EX 및 원본 이미지는 로그인 필요)",
            "Não conectado (EX e imagens originais exigem login)",
            "Вход не выполнен (EX и оригиналы требуют входа)",
        ],
        "signIn" => ["Sign in", "登录", "登入", "로그인", "Entrar", "Войти"],
        "signOut" => ["Sign out", "退出登录", "登出", "로그아웃", "Sair", "Выйти"],
        "network" => ["Network", "网络", "網路", "네트워크", "Rede", "Сеть"],
        "proxy" => ["Proxy", "代理", "代理", "프록시", "Proxy", "Прокси"],
        "domainFronting" => ["Domain fronting", "域前置", "網域前置", "도메인 프론팅", "Domain fronting", "Domain fronting"],
        "reading" => ["Reading", "阅读", "閱讀", "읽기", "Leitura", "Чтение"],
        "direction" => ["Direction", "阅读方向", "閱讀方向", "방향", "Direção", "Направление"],
        "keepAwake" => ["Keep screen awake", "保持屏幕常亮", "保持螢幕常亮", "화면 켜짐 유지", "Manter tela ativa", "Не гасить экран"],
        "downloadSettings" => ["Download", "下载", "下載", "다운로드", "Download", "Загрузки"],
        "concurrency" => ["Concurrency", "并发数", "並行數", "동시 실행", "Concorrência", "Параллельно"],
        "downloadOriginal" => [
            "Download original (needs login)",
            "下载原图（需登录）",
            "下載原圖（需登入）",
            "원본 다운로드 (로그인 필요)",
            "Baixar original (exige login)",
            "Скачивать оригиналы (нужен вход)",
        ],
        "security" => ["Security", "安全", "安全", "보안", "Segurança", "Безопасность"],
        "appLockOff" => ["App lock: off (set 4-8 digits)", "应用锁：关闭（设置 4-8 位数字）", "應用鎖：關閉（設定 4-8 位數字）", "앱 잠금: 꺼짐 (4-8자리 설정)", "Bloqueio: desligado (4-8 dígitos)", "Блокировка: выкл (4-8 цифр)"],
        "appLockOn" => ["App lock: on (type empty to disable)", "应用锁：开启（清空可关闭）", "應用鎖：開啟（清空可關閉）", "앱 잠금: 켜짐 (비우면 해제)", "Bloqueio: ligado (vazio desativa)", "Блокировка: вкл (пусто — выкл)"],
        "enterPin" => ["Enter PIN", "输入 PIN", "輸入 PIN", "PIN 입력", "Digite o PIN", "Введите PIN"],
        "about" => ["About", "关于", "關於", "정보", "Sobre", "О программе"],
        "persisted" => ["Data: persisted to db.sqlite", "数据：已保存到 db.sqlite", "資料：已儲存到 db.sqlite", "데이터: db.sqlite에 저장됨", "Dados: salvos em db.sqlite", "Данные: сохранены в db.sqlite"],
        "notPersisted" => [
            "Database unavailable — settings and progress are NOT saved",
            "数据库不可用 — 设置与进度不会保存",
            "資料庫不可用 — 設定與進度不會儲存",
            "데이터베이스 사용 불가 — 설정과 진행률이 저장되지 않습니다",
            "Banco indisponível — configurações e progresso não serão salvos",
            "База недоступна — настройки и прогресс не сохраняются",
        ],
        "language" => ["Language", "语言", "語言", "언어", "Idioma", "Язык"],
        "total" => ["items", "项", "項", "개", "itens", "элементов"],
        "done" => ["done", "已完成", "已完成", "완료", "concluído", "готово"],
        "failed" => ["failed", "失败", "失敗", "실패", "falhou", "ошибка"],
        "downloading" => ["downloading…", "下载中…", "下載中…", "다운로드 중…", "baixando…", "скачивание…"],
        _ => return None,
    })
}

/// 取一条文案；未收录时返回 key 本身（便于发现漏翻译）
pub fn t(lang: Lang, key: &str) -> &'static str {
    match table(key) {
        Some(row) => row[lang_index(lang)],
        None => Box::leak(key.to_string().into_boxed_str()),
    }
}

fn lang_index(lang: Lang) -> usize {
    match lang {
        Lang::EnUs => 0,
        Lang::ZhCn => 1,
        Lang::ZhTw => 2,
        Lang::KoKr => 3,
        Lang::PtBr => 4,
        Lang::RuRu => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_languages_have_translations() {
        for key in ["frontPage", "read", "download", "settings", "about"] {
            let row = table(key).unwrap();
            assert!(row.iter().all(|s| !s.is_empty()), "{key} 缺语言");
        }
    }

    #[test]
    fn lang_roundtrip_and_cycle() {
        for lang in Lang::all() {
            assert_eq!(Lang::from_code(lang.code()), lang);
        }
        assert_eq!(Lang::EnUs.next(), Lang::ZhCn);
        assert_eq!(Lang::RuRu.next(), Lang::EnUs, "循环回到英文");
    }

    #[test]
    fn unknown_key_falls_back_to_key() {
        assert_eq!(t(Lang::ZhCn, "notAKey"), "notAKey");
        assert_eq!(t(Lang::ZhCn, "frontPage"), "首页");
    }
}
