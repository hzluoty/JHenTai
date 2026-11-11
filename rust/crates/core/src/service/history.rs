//! 阅读历史：写 `gallery_history_v2` 表（JSON 行 + 最后阅读时间），与 Dart 版同 schema。

use rusqlite::OptionalExtension;

use crate::db::Result as DbResult;
use crate::model::GalleryHistoryModel;

pub fn upsert(conn: &rusqlite::Connection, history: &GalleryHistoryModel, last_read_time: &str) -> DbResult<()> {
    let json = serde_json::to_string(history).unwrap_or_default();
    conn.execute(
        "INSERT INTO gallery_history_v2 (gid, jsonBody, lastReadTime) VALUES (?1, ?2, ?3)
         ON CONFLICT(gid) DO UPDATE SET jsonBody = ?2, lastReadTime = ?3",
        (history.gallery_url.gid, json, last_read_time),
    )?;
    Ok(())
}

pub fn load(conn: &rusqlite::Connection, gid: i64) -> DbResult<Option<GalleryHistoryModel>> {
    let json: Option<String> = conn
        .query_row("SELECT jsonBody FROM gallery_history_v2 WHERE gid = ?1", (gid,), |row| row.get(0))
        .optional()?;
    Ok(json.and_then(|j| serde_json::from_str(&j).ok()))
}

/// 按最后阅读时间倒序（对应历史页）
pub fn recent(conn: &rusqlite::Connection, limit: usize) -> DbResult<Vec<GalleryHistoryModel>> {
    let mut stmt = conn.prepare("SELECT jsonBody FROM gallery_history_v2 ORDER BY lastReadTime DESC LIMIT ?1")?;
    let rows = stmt
        .query_map((limit as i64,), |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().filter_map(|j| serde_json::from_str(&j).ok()).collect())
}

pub fn remove(conn: &rusqlite::Connection, gid: i64) -> DbResult<()> {
    conn.execute("DELETE FROM gallery_history_v2 WHERE gid = ?1", (gid,))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GalleryUrl;

    fn temp_conn() -> (std::path::PathBuf, rusqlite::Connection) {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "jhentai-history-{}-{:?}.sqlite",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let conn = crate::db::open(&path).unwrap();
        (path, conn)
    }

    fn sample(gid: i64) -> GalleryHistoryModel {
        GalleryHistoryModel { gallery_url: GalleryUrl::new(true, gid, "aaaaaaaaaa"), title: format!("g{gid}"), ..Default::default() }
    }

    #[test]
    fn upsert_load_recent_remove() {
        let (path, conn) = temp_conn();
        upsert(&conn, &sample(1), "2026-01-01 00:00").unwrap();
        upsert(&conn, &sample(2), "2026-01-02 00:00").unwrap();
        // 同一 gid 更新而不是新增
        upsert(&conn, &sample(1), "2026-01-03 00:00").unwrap();

        assert_eq!(load(&conn, 1).unwrap().unwrap().title, "g1");
        let recent = recent(&conn, 10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].gallery_url.gid, 1, "最新的排前面");

        remove(&conn, 1).unwrap();
        assert!(load(&conn, 1).unwrap().is_none());
        drop(conn);
        let _ = std::fs::remove_file(path);
    }
}
