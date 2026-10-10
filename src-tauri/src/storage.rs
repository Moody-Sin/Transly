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
    // Windows 必须用 LOCALAPPDATA 而非 APPDATA。
    //
    // APPDATA 是**漫游**目录：域环境下会被组策略同步到用户登录的其它机器与备份介质。
    // 把翻译历史与用户自建术语库放进漫游目录，等于在用户不知情时把这些数据复制出本机，
    // 与「本地优先、零上报」的产品承诺直接冲突。LOCALAPPDATA 才是本机专属数据目录。
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"));
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));

    base.unwrap_or_else(std::env::temp_dir).join(identifier)
}

/// 把旧版漫游目录（`%APPDATA%/<identifier>`）下的缓存库迁移到本机目录。
///
/// 与 `migrate_legacy_cache` 同理，术语库与翻译缓存同处一个 SQLite 文件，
/// 不迁移就等于丢掉用户自建术语。
///
/// 迁移是幂等且不覆盖的：新库已存在时一律跳过。
pub fn migrate_roaming_cache(new_dir: &Path) -> std::io::Result<bool> {
    #[cfg(target_os = "windows")]
    {
        let Some(roaming_root) = std::env::var_os("APPDATA").map(PathBuf::from) else {
            return Ok(false);
        };
        let Some(identifier) = new_dir.file_name() else {
            return Ok(false);
        };
        let roaming_dir = roaming_root.join(identifier);
        if roaming_dir == new_dir {
            return Ok(false);
        }
        migrate_db_files(&roaming_dir, new_dir)
    }
    #[cfg(not(target_os = "windows"))]
    {
        // 非 Windows 平台不存在漫游目录概念
        let _ = new_dir;
        Ok(false)
    }
}

/// 把 `from_dir` 下的 cache.db 系列文件整体迁移到 `to_dir`。
///
/// 三个文件作为**原子单元**处理：新目录已有主库即视为已完成迁移，
/// 绝不单独搬运旁路文件——SQLite 的 WAL 不校验盐值，
/// 陈旧 WAL 被回放进新库会造成静默的数据损坏。
fn migrate_db_files(from_dir: &Path, to_dir: &Path) -> std::io::Result<bool> {
    let from_db = from_dir.join("cache.db");
    let to_db = to_dir.join("cache.db");

    if to_db.exists() || !from_db.exists() {
        return Ok(false);
    }

    std::fs::create_dir_all(to_dir)?;
    let mut migrated = false;
    for suffix in DB_SUFFIXES {
        let from = from_dir.join(format!("cache.db{suffix}"));
        let to = to_dir.join(format!("cache.db{suffix}"));
        if from.exists() {
            std::fs::rename(&from, &to)?;
            migrated = true;
        }
    }
    Ok(migrated)
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

    // 三个文件必须作为**原子单元**整体判断（见 `migrate_db_files` 的说明）。
    migrate_db_files(&legacy_dir, new_dir)
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

    /// 回归：新目录已有主库但缺 `-wal` 时，旧目录遗留的 WAL **绝不能**被搬进去。
    /// SQLite 的 WAL 不在主库文件头保存盐值、只校验页大小，陈旧 WAL 会被回放进
    /// 新库并重放其中的页，造成静默的数据损坏（用户自建术语可能混入旧库内容）。
    #[test]
    fn test_migrate_never_mixes_stale_wal_into_new_db() {
        let root = temp_dir("stalewal");
        let legacy = root.join("Transly");
        let new_dir = root.join("com.transly.desktop");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        // 典型触发态：新库已在用（主库存在），旧版本回滚后留下 WAL 旁路文件
        std::fs::write(new_dir.join("cache.db"), b"current-db").unwrap();
        std::fs::write(legacy.join("cache.db"), b"legacy-db").unwrap();
        std::fs::write(legacy.join("cache.db-wal"), b"stale-wal").unwrap();
        std::fs::write(legacy.join("cache.db-shm"), b"stale-shm").unwrap();

        let migrated = migrate_legacy_cache(&new_dir).unwrap();
        assert!(!migrated, "主库已存在时整体判定为无需迁移");

        assert!(
            !new_dir.join("cache.db-wal").exists(),
            "陈旧 WAL 绝不能被搬进新库（会导致数据损坏）"
        );
        assert!(
            !new_dir.join("cache.db-shm").exists(),
            "陈旧 SHM 绝不能被搬进新库"
        );
        assert_eq!(
            std::fs::read(new_dir.join("cache.db")).unwrap(),
            b"current-db"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 旧目录只有旁路文件而没有主库时，属于残留垃圾，不应被当作迁移源。
    #[test]
    fn test_migrate_ignores_orphan_sidecar_files() {
        let root = temp_dir("orphan");
        let legacy = root.join("Transly");
        let new_dir = root.join("com.transly.desktop");
        std::fs::create_dir_all(&legacy).unwrap();

        // 只有 WAL 与 SHM，主库缺失 —— SQLite 不会把这种组合视作有效数据库
        std::fs::write(legacy.join("cache.db-wal"), b"orphan-wal").unwrap();
        std::fs::write(legacy.join("cache.db-shm"), b"orphan-shm").unwrap();

        let migrated = migrate_legacy_cache(&new_dir).unwrap();
        assert!(!migrated, "主库缺失时不应迁移任何文件");
        assert!(!new_dir.join("cache.db-wal").exists());
        assert!(!new_dir.join("cache.db").exists());

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
