//! 阅读进度（对应 `service/read_progress_service.dart`）：存 `local_config` 的子键。

use crate::config::{keys, ConfigStore};

/// 记录某画廊读到第几页（0 基）
pub fn save(store: &mut dyn ConfigStore, gid: i64, index: i64) {
    crate::config::write_string(store, keys::READ_INDEX_RECORD, &gid.to_string(), &index.to_string());
}

pub fn load(store: &dyn ConfigStore, gid: i64) -> Option<i64> {
    crate::config::read_string(store, keys::READ_INDEX_RECORD, &gid.to_string())?.parse().ok()
}

pub fn remove(store: &mut dyn ConfigStore, gid: i64) {
    store.delete(keys::READ_INDEX_RECORD, &gid.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MemoryConfigStore;

    #[test]
    fn save_load_remove() {
        let mut store = MemoryConfigStore::new();
        assert_eq!(load(&store, 1), None);
        save(&mut store, 1, 12);
        assert_eq!(load(&store, 1), Some(12));
        save(&mut store, 1, 20);
        assert_eq!(load(&store, 1), Some(20));
        remove(&mut store, 1);
        assert_eq!(load(&store, 1), None);
    }
}
