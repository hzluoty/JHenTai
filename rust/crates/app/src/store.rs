//! 设置存储的组合根：打开 `db.sqlite`，失败则降级为内存实现。
//!
//! 降级是计划 §4.4 允许的（拒绝旧 schema、不做迁移），但**必须可观测**：
//! `store_is_persistent` 供设置/关于页提示「数据不会保存」。

use jhentai_core::config::{ConfigStore, MemoryConfigStore};
use jhentai_core::db::SqliteConfigStore;

pub struct OpenedStore {
    pub store: Box<dyn ConfigStore + Send>,
    /// 真正落盘时为 true；降级到内存时为 false
    pub persistent: bool,
    /// db.sqlite 路径（屏蔽规则等表需要单独连接读取）
    pub db_path: Option<std::path::PathBuf>,
}

/// `data_dir` 是 `getExternalFilesDir(null)`，里面应有 `db.sqlite`
pub fn open(data_dir: Option<&std::path::Path>) -> OpenedStore {
    let Some(dir) = data_dir else {
        return OpenedStore { store: Box::new(MemoryConfigStore::new()), persistent: false, db_path: None };
    };
    let path = dir.join("db.sqlite");
    match jhentai_core::db::open(&path) {
        Ok(conn) => OpenedStore { store: Box::new(SqliteConfigStore::new(conn)), persistent: true, db_path: Some(path) },
        Err(e) => {
            eprintln!("jhentai: open {} failed ({e}); settings will not persist", path.display());
            OpenedStore { store: Box::new(MemoryConfigStore::new()), persistent: false, db_path: None }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn missing_dir_degrades_to_memory() {
        let opened = super::open(None);
        assert!(!opened.persistent);
    }

    #[test]
    fn fresh_db_is_persistent() {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "jhentai-store-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let opened = super::open(Some(&dir));
        assert!(opened.persistent);
        drop(opened);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
