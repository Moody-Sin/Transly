//! 应用数据目录解析与旧版缓存迁移。
//!
//! 平台目录假设集中在本层（平台适配层），`transly-core` 保持零平台假设，
//! 以便被 CLI / WASM 等不同宿主复用。

use std::path::{Path, PathBuf};

/// 应用数据目录：Tauri 约定为 `<系统数据目录>/<bundle identifier>`。
///
/// 旧版手拼 `%APPDATA%\Transly`，与沙箱、清理、多用户策略都对不上，
/// macOS 上也不符合平台惯例。
pub fn app_data_dir(identifier: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));

    base.unwrap_or_else(std::env::temp_dir).join(identifier)
}

/// 缓存库涉及的全部文件：主库与 WAL 模式的旁路文件。
const DB_SUFFIXES: [&str; 3] = ["", "-wal", "-shm"];

/// 把旧版目录（`<父目录>/Transly`）下的缓存库迁移到新目录。
///
/// 术语库与翻译缓存在**同一个** SQLite 文件中，不迁移就等于直接丢掉
/// 用户自建的术语。因此迁移必须执行，且必须覆盖 WAL 旁路文件，
/// 否则会丢掉尚未 checkpoint 的最新写入。
///
/// 迁移是幂等且不覆盖的：新库已存在时一律跳过，避免二次启动覆盖新数据。
pub fn migrate_legacy_cache(new_dir: &Path) -> std::io::Result<bool> {
    let Some(legacy_dir) = new_dir.parent().map(|p| p.join("Transly")) else {
        return Ok(false);
    };
    // identifier 恰好就是 "Transly" 时新旧目录重合，无需迁移
    if legacy_dir == new_dir {
        return Ok(false);
    }

    let mut migrated = false;
    for suffix in DB_SUFFIXES {
        let from = legacy_dir.join(format!("cache.db{suffix}"));
        let to = new_dir.join(format!("cache.db{suffix}"));
        // 新库已存在即视为已完成迁移，绝不覆盖
        if from.exists() && !to.exists() {
            std::fs::rename(&from, &to)?;
            migrated = true;
        }
    }
    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 建一个隔离的临时目录，避免污染开发者真实数据目录。
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "transly_test_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("创建临时目录失败");
        dir
    }

    #[test]
    fn test_app_data_dir_appends_identifier() {
        let dir = app_data_dir("com.transly.desktop");
        assert!(
            dir.ends_with("com.transly.desktop"),
            "数据目录必须以 identifier 结尾，实际 {dir:?}"
        );
    }

    #[test]
    fn test_migrate_moves_db_and_wal_files() {
        let root = temp_dir("migrate");
        let legacy = root.join("Transly");
        let new_dir = root.join("com.transly.desktop");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        // 旧库 + WAL 旁路文件都必须迁移，否则丢失未 checkpoint 的写入
        std::fs::write(legacy.join("cache.db"), b"legacy-db").unwrap();
        std::fs::write(legacy.join("cache.db-wal"), b"legacy-wal").unwrap();

        let migrated = migrate_legacy_cache(&new_dir).unwrap();
        assert!(migrated, "存在旧库时必须执行迁移");
        assert_eq!(
            std::fs::read(new_dir.join("cache.db")).unwrap(),
            b"legacy-db"
        );
        assert_eq!(
            std::fs::read(new_dir.join("cache.db-wal")).unwrap(),
            b"legacy-wal"
        );
        assert!(!legacy.join("cache.db").exists(), "旧库应已移走");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_migrate_never_overwrites_existing_new_db() {
        let root = temp_dir("nooverwrite");
        let legacy = root.join("Transly");
        let new_dir = root.join("com.transly.desktop");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        std::fs::write(legacy.join("cache.db"), b"legacy-db").unwrap();
        std::fs::write(new_dir.join("cache.db"), b"current-db").unwrap();

        let migrated = migrate_legacy_cache(&new_dir).unwrap();
        assert!(!migrated, "新库已存在时不得迁移");
        assert_eq!(
            std::fs::read(new_dir.join("cache.db")).unwrap(),
            b"current-db",
            "绝不能覆盖新库"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_migrate_noop_when_legacy_absent() {
        let root = temp_dir("absent");
        let new_dir = root.join("com.transly.desktop");
        std::fs::create_dir_all(&new_dir).unwrap();

        assert!(!migrate_legacy_cache(&new_dir).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }
}
