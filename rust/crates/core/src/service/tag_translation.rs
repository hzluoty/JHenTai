//! 标签翻译（对应 `service/tag_translation_service.dart`）。
//!
//! 数据源：`https://fastly.jsdelivr.net/gh/EhTagTranslation/DatabaseReleases/db.html.json`
//! （Dart 版用同一个地址）。结构：
//!
//! ```json
//! { "data": [ { "namespace": "female", "data": { "sole female": { "name": "<p>单女</p>" } } },
//!             { "namespace": "rows", "data": { "female": { "name": "<p>女性</p>" } } } ] }
//! ```
//!
//! 翻译是 HTML 片段，去标签后使用。入库复用 Dart 版的 `tag` 表（namespace/_key/tagName）。

use std::collections::HashMap;

/// 解析结果：命名空间名 + 每个命名空间下的标签名
#[derive(Debug, Default, Clone)]
pub struct TagTranslation {
    /// namespace -> 显示名（如 female -> 女性）
    pub namespaces: HashMap<String, String>,
    /// namespace -> (tag key -> 显示名)
    pub tags: HashMap<String, HashMap<String, String>>,
}

impl TagTranslation {
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }

    /// 标签翻译（如 ("female", "sole female") -> "单女"）
    pub fn tag_name(&self, namespace: &str, key: &str) -> Option<&str> {
        self.tags.get(namespace)?.get(key).map(String::as_str)
    }

    /// 命名空间翻译
    pub fn namespace_name(&self, namespace: &str) -> Option<&str> {
        self.namespaces.get(namespace).map(String::as_str)
    }

    /// 规则匹配用：某命名空间下所有可匹配的字符串（原始 key + 翻译名），
    /// 让按翻译名写的屏蔽规则也能命中（Dart 的 `GalleryTagAttributeGetter` 同样处理）。
    pub fn match_values(&self, namespace: &str, key: &str) -> Vec<String> {
        let mut values = vec![key.to_string()];
        if let Some(translated) = self.tag_name(namespace, key) {
            if translated != key {
                values.push(translated.to_string());
            }
        }
        values
    }
}

/// 去 HTML 标签（翻译里带 `<p>`、`<abbr>`）
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// 解析 `db.html.json`
pub fn parse(json: &str) -> Option<TagTranslation> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let data = value.get("data")?.as_array()?;

    let mut result = TagTranslation::default();
    for item in data {
        let Some(namespace) = item.get("namespace").and_then(|n| n.as_str()) else {
            continue;
        };
        let Some(inner) = item.get("data").and_then(|d| d.as_object()) else {
            continue;
        };

        if namespace == "rows" {
            // 命名空间显示名
            for (ns, info) in inner {
                if let Some(name) = info.get("name").and_then(|n| n.as_str()) {
                    result.namespaces.insert(ns.clone(), strip_html(name));
                }
            }
        } else {
            let entry = result.tags.entry(namespace.to_string()).or_default();
            for (key, info) in inner {
                if let Some(name) = info.get("name").and_then(|n| n.as_str()) {
                    entry.insert(key.clone(), strip_html(name));
                }
            }
        }
    }
    Some(result)
}

/// 写入 Dart 版同名的 `tag` 表（覆盖式更新）
pub fn store(conn: &rusqlite::Connection, translation: &TagTranslation) -> crate::db::Result<()> {
    // 10 万+ 行：必须包在事务里，否则每行一次隐式事务，模拟器上要跑几分钟
    conn.execute("DELETE FROM tag", [])?;
    conn.execute_batch("BEGIN")?;
    let result = (|| -> crate::db::Result<()> {
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO tag (namespace, _key, translatedNamespace, tagName, fullTagName, intro, links)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL)",
        )?;
        for (namespace, tags) in &translation.tags {
            let namespace_name = translation.namespace_name(namespace).unwrap_or(namespace);
            for (key, name) in tags {
                stmt.execute((namespace, key, namespace_name, name, format!("{namespace_name}:{name}")))?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

/// 从 `tag` 表读回（启动时用，避免每次都下载 7MB）
pub fn load(conn: &rusqlite::Connection) -> crate::db::Result<TagTranslation> {
    let mut stmt = conn.prepare("SELECT namespace, _key, tagName, translatedNamespace FROM tag")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;

    let mut result = TagTranslation::default();
    for row in rows {
        let (namespace, key, name, namespace_name) = row?;
        if let Some(name) = name {
            result.tags.entry(namespace.clone()).or_default().insert(key, name);
        }
        if let Some(ns_name) = namespace_name {
            result.namespaces.entry(namespace).or_insert(ns_name);
        }
    }
    Ok(result)
}

/// 数据源地址（与 Dart 一致）
pub const DATABASE_URL: &str = "https://fastly.jsdelivr.net/gh/EhTagTranslation/DatabaseReleases/db.html.json";

/// 下载并解析
pub async fn fetch(
    client: &crate::net::http::Client,
    state: &std::sync::Arc<std::sync::Mutex<crate::net::NetState>>,
    resolver: &crate::net::FrontingResolver,
) -> Result<TagTranslation, String> {
    let (status, bytes) = crate::net::http::get_bytes(client, state, resolver, DATABASE_URL)
        .await
        .map_err(|e| e.to_string())?;
    if status != 200 {
        return Err(format!("status {status}"));
    }
    let text = String::from_utf8_lossy(&bytes);
    parse(&text).ok_or_else(|| "parse failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"data":[
        {"namespace":"rows","data":{
            "female":{"name":"<p>女性</p>"},
            "male":{"name":"<p>男性</p>"}
        }},
        {"namespace":"female","data":{
            "sole female":{"name":"<p>单女</p>"},
            "paizuri":{"name":"<p>乳交</p>"}
        }},
        {"namespace":"other","data":{"full color":{"name":"<p>全彩</p>"}}}
    ]}"#;

    #[test]
    fn parses_namespaces_and_tags() {
        let t = parse(SAMPLE).unwrap();
        assert_eq!(t.namespace_name("female"), Some("女性"));
        assert_eq!(t.tag_name("female", "sole female"), Some("单女"));
        assert_eq!(t.tag_name("other", "full color"), Some("全彩"));
        assert_eq!(t.tag_name("female", "nope"), None);
        assert!(!t.is_empty());
    }

    #[test]
    fn match_values_include_both_forms() {
        let t = parse(SAMPLE).unwrap();
        let values = t.match_values("female", "paizuri");
        assert_eq!(values, vec!["paizuri".to_string(), "乳交".to_string()]);
        // 无翻译时只剩原 key
        assert_eq!(t.match_values("female", "unknown"), vec!["unknown".to_string()]);
    }

    #[test]
    fn store_and_load_roundtrip() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "jhentai-tagtrans-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let conn = crate::db::open(&path).unwrap();
        let t = parse(SAMPLE).unwrap();
        store(&conn, &t).unwrap();

        let loaded = load(&conn).unwrap();
        assert_eq!(loaded.tag_name("female", "sole female"), Some("单女"));
        assert_eq!(loaded.namespace_name("female"), Some("女性"));
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn broken_json_is_none() {
        assert!(parse("{}").is_none());
        assert!(parse("not json").is_none());
    }
}
