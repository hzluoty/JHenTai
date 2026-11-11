//! SQLite 数据层：schema v25，直接接管 Flutter 版留下的 `db.sqlite`。
//!
//! 表名/列名必须与 Drift 生成的 DDL 逐字一致（见计划 §4.4）：
//! Drift 把 Dart 字段名 `pageCount` 落成 `page_count`，`galleryUrl` 落成 `gallery_url`，
//! 而经 `.named()` 指定的列（如 `jsonBody`）保持原样。

use rusqlite::{Connection, OptionalExtension};

/// 当前 schema 版本（= Drift `schemaVersion`）
pub const SCHEMA_VERSION: i64 = 25;

/// Flutter 最后一版写入的 `jhentai.version`
pub const FLUTTER_JHENTAI_VERSION: i64 = 13;

#[derive(Debug)]
pub enum DbError {
    Sqlite(rusqlite::Error),
    /// 数据来自更老的 Flutter 版本，必须先装最后一个 Flutter 版跑完迁移
    LegacyVersion { found: i64 },
    /// schema 比本程序新，拒绝打开（避免写坏）
    FutureSchema { found: i64 },
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(e) => write!(f, "sqlite: {e}"),
            Self::LegacyVersion { found } => write!(f, "legacy jhentai.version {found}: run the last Flutter build first"),
            Self::FutureSchema { found } => write!(f, "db schema v{found} is newer than supported v{SCHEMA_VERSION}"),
        }
    }
}

impl std::error::Error for DbError {}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Sqlite(e)
    }
}

pub type Result<T> = std::result::Result<T, DbError>;

/// 所有 DDL。列顺序与 Drift 生成的建表语句一致，便于对比。
pub const DDL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS gallery_downloaded_v2 (
        gid INTEGER NOT NULL PRIMARY KEY,
        token TEXT NOT NULL,
        title TEXT NOT NULL,
        category TEXT NOT NULL,
        page_count INTEGER NOT NULL,
        gallery_url TEXT NOT NULL,
        old_version_gallery_url TEXT,
        uploader TEXT,
        publish_time TEXT NOT NULL,
        download_status_index INTEGER NOT NULL,
        insert_time TEXT NOT NULL,
        download_original_image INTEGER NOT NULL DEFAULT 0,
        priority INTEGER NOT NULL,
        sort_order INTEGER NOT NULL DEFAULT 0,
        group_name TEXT NOT NULL,
        tags TEXT NOT NULL DEFAULT '',
        tag_refresh_time TEXT,
        sanitized_title TEXT
    )",
    "CREATE TABLE IF NOT EXISTS archive_downloaded_v2 (
        gid INTEGER NOT NULL PRIMARY KEY,
        token TEXT NOT NULL,
        title TEXT NOT NULL,
        category TEXT NOT NULL,
        page_count INTEGER NOT NULL,
        gallery_url TEXT NOT NULL,
        cover_url TEXT NOT NULL,
        uploader TEXT,
        size INTEGER NOT NULL,
        publish_time TEXT NOT NULL,
        archive_status_index INTEGER NOT NULL,
        archive_page_url TEXT NOT NULL,
        download_page_url TEXT,
        download_url TEXT,
        is_original INTEGER NOT NULL,
        insert_time TEXT NOT NULL,
        sort_order INTEGER NOT NULL DEFAULT 0,
        group_name TEXT NOT NULL,
        tags TEXT NOT NULL DEFAULT '',
        tag_refresh_time TEXT,
        parse_source INTEGER NOT NULL DEFAULT 0,
        sanitized_title TEXT
    )",
    "CREATE TABLE IF NOT EXISTS image (
        gid INTEGER NOT NULL REFERENCES gallery_downloaded_v2(gid),
        url TEXT NOT NULL,
        original_image_url TEXT,
        serial_no INTEGER NOT NULL,
        path TEXT NOT NULL,
        image_hash TEXT NOT NULL,
        download_status_index INTEGER NOT NULL,
        PRIMARY KEY (gid, serial_no)
    )",
    "CREATE TABLE IF NOT EXISTS gallery_group (
        group_name TEXT NOT NULL PRIMARY KEY,
        sort_order INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS archive_group (
        group_name TEXT NOT NULL PRIMARY KEY,
        sort_order INTEGER NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS gallery_history_v2 (
        gid INTEGER NOT NULL PRIMARY KEY,
        jsonBody TEXT NOT NULL,
        lastReadTime TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS local_config (
        configKey TEXT NOT NULL,
        subConfigKey TEXT NOT NULL,
        value TEXT NOT NULL,
        utime TEXT NOT NULL,
        PRIMARY KEY (configKey, subConfigKey)
    )",
    "CREATE TABLE IF NOT EXISTS tag (
        namespace TEXT NOT NULL,
        _key TEXT NOT NULL,
        translatedNamespace TEXT,
        tagName TEXT,
        fullTagName TEXT,
        intro TEXT,
        links TEXT,
        PRIMARY KEY (namespace, _key)
    )",
    "CREATE TABLE IF NOT EXISTS tag_count (
        namespaceWithKey TEXT NOT NULL PRIMARY KEY,
        count INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS dio_cache (
        cacheKey TEXT NOT NULL PRIMARY KEY,
        url TEXT NOT NULL,
        expireDate INTEGER NOT NULL,
        content BLOB NOT NULL,
        headers BLOB NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS block_rule (
        id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
        group_id TEXT NOT NULL,
        target INTEGER NOT NULL,
        attribute INTEGER NOT NULL,
        pattern INTEGER NOT NULL,
        expression TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_gh2_last_read_time ON gallery_history_v2 (lastReadTime)",
    "CREATE INDEX IF NOT EXISTS g_idx_insert_time ON gallery_downloaded_v2 (insert_time)",
    "CREATE INDEX IF NOT EXISTS g_idx_sort_order ON gallery_downloaded_v2 (sort_order)",
    "CREATE INDEX IF NOT EXISTS g_idx_group_name ON gallery_downloaded_v2 (group_name)",
    "CREATE INDEX IF NOT EXISTS g_idx_tag_refresh_time ON gallery_downloaded_v2 (tag_refresh_time)",
    "CREATE INDEX IF NOT EXISTS a_idx_insert_time ON archive_downloaded_v2 (insert_time)",
    "CREATE INDEX IF NOT EXISTS a_idx_sort_order ON archive_downloaded_v2 (sort_order)",
    "CREATE INDEX IF NOT EXISTS a_idx_group_name ON archive_downloaded_v2 (group_name)",
    "CREATE INDEX IF NOT EXISTS a_idx_tag_refresh_time ON archive_downloaded_v2 (tag_refresh_time)",
    "CREATE INDEX IF NOT EXISTS idx_expire_date ON dio_cache (expireDate)",
    "CREATE INDEX IF NOT EXISTS idx_url ON dio_cache (url)",
    "CREATE INDEX IF NOT EXISTS idx_group_id ON block_rule (group_id)",
    "CREATE INDEX IF NOT EXISTS idx_target ON block_rule (target)",
    "CREATE INDEX IF NOT EXISTS l_idx_u_time ON local_config (utime)",
];

/// 打开数据库并做首启动检查（计划 §4.4）。
///
/// - `PRAGMA user_version` 必须是 25；更高则拒绝（避免写坏新版数据）
/// - 若 `db.sqlite.flutter.bak` 不存在，先复制一份备份
pub fn open(path: &std::path::Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // 备份：只在第一次打开（备份不存在）时做
    let backup = path.with_extension("sqlite.flutter.bak");
    if path.exists() && !backup.exists() {
        std::fs::copy(path, &backup).map_err(|e| DbError::Sqlite(rusqlite::Error::InvalidParameterName(e.to_string())))?;
    }

    let conn = Connection::open(path)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

    if version > SCHEMA_VERSION {
        return Err(DbError::FutureSchema { found: version });
    }
    if version == 0 {
        // 全新安装
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        for stmt in DDL {
            conn.execute(stmt, [])?;
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        return Ok(conn);
    }
    if version < SCHEMA_VERSION {
        return Err(DbError::LegacyVersion { found: version });
    }

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

/// 读 `jhentai.version`（Flutter 版写在外部私有目录里的文本文件）
pub fn read_jhentai_version(path: &std::path::Path) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// 首启动数据连续性检查：只接受 Flutter 最后一版（v25 / 13）。
pub fn check_upgrade_path(db_path: &std::path::Path, version_file: &std::path::Path) -> Result<()> {
    let conn = Connection::open(db_path)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(DbError::FutureSchema { found: version });
    }
    if version < SCHEMA_VERSION {
        return Err(DbError::LegacyVersion { found: version });
    }
    if let Some(v) = read_jhentai_version(version_file) {
        if v < FLUTTER_JHENTAI_VERSION {
            return Err(DbError::LegacyVersion { found: v });
        }
    }
    Ok(())
}

/// `local_config` 读写（设置、cookie、阅读进度、搜索配置都走这一张表）。
pub mod local_config {
    use super::*;

    pub fn read(conn: &Connection, config_key: &str, sub_config_key: &str) -> Result<Option<String>> {
        Ok(conn
            .query_row(
                "SELECT value FROM local_config WHERE configKey = ?1 AND subConfigKey = ?2",
                (config_key, sub_config_key),
                |row| row.get::<_, String>(0),
            )
            .optional()?)
    }

    pub fn write(conn: &Connection, config_key: &str, sub_config_key: &str, value: &str) -> Result<()> {
        conn.execute(
            "INSERT INTO local_config (configKey, subConfigKey, value, utime) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(configKey, subConfigKey) DO UPDATE SET value = ?3, utime = ?4",
            (config_key, sub_config_key, value, utime_now()),
        )?;
        Ok(())
    }

    pub fn delete(conn: &Connection, config_key: &str, sub_config_key: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM local_config WHERE configKey = ?1 AND subConfigKey = ?2",
            (config_key, sub_config_key),
        )?;
        Ok(())
    }

    /// 一个 configKey 下的全部子键（`readWithAllSubKeys`）
    pub fn read_all_sub_keys(conn: &Connection, config_key: &str) -> Result<Vec<(String, String)>> {
        let mut stmt = conn.prepare("SELECT subConfigKey, value FROM local_config WHERE configKey = ?1")?;
        let rows = stmt
            .query_map((config_key,), |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    fn utime_now() -> String {
        match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => format!("{}.{:06}", d.as_secs(), d.subsec_micros()),
            Err(_) => "0.000000".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db() -> (tempdir::TempDirGuard, Connection) {
        let dir = tempdir::make();
        let conn = open(&dir.path().join("db.sqlite")).unwrap();
        (dir, conn)
    }

    /// 免依赖的临时目录
    mod tempdir {
        use std::path::{Path, PathBuf};

        pub struct TempDirGuard(PathBuf);

        impl TempDirGuard {
            pub fn path(&self) -> &Path {
                &self.0
            }
        }

        impl Drop for TempDirGuard {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        pub fn make() -> TempDirGuard {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "jhentai-core-test-{}-{:?}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            );
            path.push(unique);
            std::fs::create_dir_all(&path).unwrap();
            TempDirGuard(path)
        }
    }

    #[test]
    fn fresh_db_gets_schema_version_25() {
        let (_dir, conn) = temp_db();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn all_tables_exist() {
        let (_dir, conn) = temp_db();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        for expected in [
            "archive_downloaded_v2",
            "archive_group",
            "block_rule",
            "dio_cache",
            "gallery_downloaded_v2",
            "gallery_group",
            "gallery_history_v2",
            "image",
            "local_config",
            "tag",
            "tag_count",
        ] {
            assert!(tables.iter().any(|t| t == expected), "missing table {expected}, got {tables:?}");
        }
    }

    #[test]
    fn local_config_roundtrip_and_upsert() {
        let (_dir, conn) = temp_db();
        local_config::write(&conn, "EHSetting", "", r#"{"site":"EX","redirect2Eh":true}"#).unwrap();
        assert_eq!(
            local_config::read(&conn, "EHSetting", "").unwrap().as_deref(),
            Some(r#"{"site":"EX","redirect2Eh":true}"#)
        );
        local_config::write(&conn, "EHSetting", "", r#"{"site":"EH","redirect2Eh":false}"#).unwrap();
        assert!(local_config::read(&conn, "EHSetting", "").unwrap().unwrap().contains("EH"));

        local_config::write(&conn, "readIndexRecord", "4321294", "12").unwrap();
        assert_eq!(local_config::read_all_sub_keys(&conn, "readIndexRecord").unwrap(), vec![("4321294".to_string(), "12".to_string())]);

        local_config::delete(&conn, "EHSetting", "").unwrap();
        assert_eq!(local_config::read(&conn, "EHSetting", "").unwrap(), None);
    }

    #[test]
    fn future_schema_is_refused() {
        let dir = tempdir::make();
        let path = dir.path().join("db.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 99).unwrap();
        }
        match open(&path) {
            Err(DbError::FutureSchema { found }) => assert_eq!(found, 99),
            other => panic!("expected FutureSchema, got {other:?}", other = other.err()),
        }
    }

    #[test]
    fn legacy_schema_requires_flutter_migration() {
        let dir = tempdir::make();
        let path = dir.path().join("db.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.pragma_update(None, "user_version", 24).unwrap();
        }
        match open(&path) {
            Err(DbError::LegacyVersion { found }) => assert_eq!(found, 24),
            other => panic!("expected LegacyVersion, got {other:?}", other = other.err()),
        }
    }

    #[test]
    fn backup_created_once_for_existing_db() {
        let dir = tempdir::make();
        let path = dir.path().join("db.sqlite");
        // 模拟 Flutter 版留下的数据库
        drop(open(&path).unwrap());
        let backup = path.with_extension("sqlite.flutter.bak");
        drop(std::fs::remove_file(&backup));
        std::fs::write(&path, b"flutter-data").unwrap();

        // 无效数据库文件会在打开时报错，但备份必须先于打开完成
        let _ = open(&path);
        assert_eq!(std::fs::read(&backup).unwrap(), b"flutter-data");

        // 已有备份不再覆盖
        std::fs::write(&backup, b"keep me").unwrap();
        let _ = open(&path);
        assert_eq!(std::fs::read(&backup).unwrap(), b"keep me");
    }
}
