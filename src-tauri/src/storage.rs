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

/// 解析应用实际使用的数据目录，支持便携模式智能判定。
///
/// 便携模式判定逻辑（满足其一即尝试激活）：
/// 1. 环境变量 `TRANSLY_PORTABLE=1` 强制开启，`TRANSLY_PORTABLE=0` 强制关闭；
/// 2. 命令行参数 `--portable`；
/// 3. 二进制特征：文件名包含 `portable`（不区分大小写，如 `Transly-Portable-windows-x64.exe`）；
/// 4. 同级标记：可执行文件同级目录存在 `portable.flag` 文件或已有 `data` 目录。
///
/// 若便携模式生效且同级目录可写，返回 `(exe_dir/data, true)`；
/// 若便携目录不可写（例如只读介质、受限目录）或未处于便携模式，安全回退至平台系统目录 `(app_data_dir(identifier), false)`。
pub fn resolve_data_dir(identifier: &str) -> (PathBuf, bool) {
    let args: Vec<String> = std::env::args().collect();
    let env_var = std::env::var("TRANSLY_PORTABLE").ok();
    let current_exe = std::env::current_exe().ok();

    resolve_data_dir_internal(
        identifier,
        current_exe.as_deref(),
        &args,
        env_var.as_deref(),
    )
}

pub fn resolve_data_dir_internal(
    identifier: &str,
    current_exe: Option<&Path>,
    args: &[String],
    env_override: Option<&str>,
) -> (PathBuf, bool) {
    let is_portable_intent = match env_override {
        Some(v) if v == "1" || v.eq_ignore_ascii_case("true") => Some(true),
        Some(v) if v == "0" || v.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    };

    let should_try_portable = is_portable_intent.unwrap_or_else(|| {
        if args.iter().any(|a| a == "--portable") {
            return true;
        }

        if let Some(exe_path) = current_exe {
            if let Some(name) = exe_path.file_name().and_then(|s| s.to_str()) {
                if name.to_lowercase().contains("portable") {
                    return true;
                }
            }
            if let Some(parent) = exe_path.parent() {
                if parent.join("portable.flag").is_file() || parent.join("data").is_dir() {
                    return true;
                }
            }
        }

        false
    });

    if should_try_portable {
        if let Some(exe_path) = current_exe {
            if let Some(portable_dir) = try_resolve_portable_data_dir(exe_path) {
                return (portable_dir, true);
            }
            eprintln!("便携数据目录不可写，已安全降级至系统标准数据目录");
        }
    }

    (app_data_dir(identifier), false)
}

fn try_resolve_portable_data_dir(exe_path: &Path) -> Option<PathBuf> {
    let exe_dir = exe_path.parent()?;
    let data_dir = exe_dir.join("data");

    if std::fs::create_dir_all(&data_dir).is_ok() {
        let probe = data_dir.join(format!(".write_probe_{}", std::process::id()));
        if std::fs::write(&probe, b"ok").is_ok() {
            let _ = std::fs::remove_file(probe);
            return Some(data_dir);
        }
    }

    None
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
    migrate_db_files_with(from_dir, to_dir, |a, b| std::fs::rename(a, b))
}

/// `migrate_db_files` 的可注入实现（`rename_fn` 便于测试注入失败）。
///
/// 中途失败必须回滚：若 `cache.db` 已搬走而 `cache.db-wal` 搬运失败，
/// 新目录会留下**残缺的主库**（丢掉未 checkpoint 的最新写入），
/// 且下次启动因 `to_db.exists()` 直接跳过迁移，WAL 数据永久丢失。
/// 因此任何一步失败都逆序把本次已搬移的文件还原回 `from_dir`。
fn migrate_db_files_with<F>(
    from_dir: &Path,
    to_dir: &Path,
    mut rename_fn: F,
) -> std::io::Result<bool>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    let from_db = from_dir.join("cache.db");
    let to_db = to_dir.join("cache.db");

    if to_db.exists() || !from_db.exists() {
        return Ok(false);
    }

    std::fs::create_dir_all(to_dir)?;
    let mut migrated = false;
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();

    for suffix in DB_SUFFIXES {
        let from = from_dir.join(format!("cache.db{suffix}"));
        let to = to_dir.join(format!("cache.db{suffix}"));
        if from.exists() {
            if let Err(e) = rename_fn(&from, &to) {
                // 逆序回滚本次已成功搬移的文件，确保 to_dir 不留残缺状态
                for (moved_from, moved_to) in moved.iter().rev() {
                    let _ = std::fs::rename(moved_to, moved_from);
                }
                return Err(e);
            }
            moved.push((from, to));
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

/// 首次运行时将系统 Keyring 中的旧版非敏感配置一次性迁移至 SQLite `app_settings` 表。
/// 迁移完成后写入 `__migrated_from_keyring = "true"` 标记。
/// 后续冷启动与日常查询直接走本地数据库，彻底杜绝主线程访问系统凭据管理器的阻塞 I/O。
pub fn migrate_keyring_settings_to_db(cache: &transly_core::TranslationCache) {
    if cache
        .get_setting("__migrated_from_keyring")
        .ok()
        .flatten()
        .is_some()
    {
        return;
    }

    const NON_SENSITIVE_KEYS: &[(&str, &str)] = &[
        ("clipboard_monitor_enabled", "false"),
        ("cache_enabled", "true"),
        ("custom_shortcut", "Alt+Shift+D"),
        ("custom_replace_shortcut", "Alt+Shift+R"),
        ("preferred_replace_engine", "google"),
        ("llm_style", "general"),
    ];

    for (key, default_val) in NON_SENSITIVE_KEYS {
        let current_db_val = cache.get_setting(key).ok().flatten();
        if current_db_val.is_none() {
            let val = crate::secrets::get_secret(key)
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| default_val.to_string());
            let _ = cache.set_setting(key, &val);
            let _ = crate::secrets::delete_secret(key);
        }
    }

    let _ = cache.set_setting("__migrated_from_keyring", "true");
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

    /// 回归：`cache.db` 搬移成功、`cache.db-wal` 搬移失败时，必须回滚已搬移的主库。
    /// 否则新目录留下残缺主库（丢掉未 checkpoint 的写入），且下次启动因
    /// `to_db.exists()` 跳过迁移，WAL 数据永久丢失。
    #[test]
    fn test_migrate_rolls_back_on_sidecar_failure() {
        let root = temp_dir("rollback");
        let legacy = root.join("Transly");
        let new_dir = root.join("com.transly.desktop");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();

        std::fs::write(legacy.join("cache.db"), b"legacy-db").unwrap();
        std::fs::write(legacy.join("cache.db-wal"), b"legacy-wal").unwrap();

        // 注入：主库正常搬运，WAL 搬运失败
        let result = migrate_db_files_with(&legacy, &new_dir, |a, b| {
            if a.to_string_lossy().ends_with("-wal") {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "注入的 WAL 搬运失败",
                ));
            }
            std::fs::rename(a, b)
        });

        assert!(result.is_err(), "旁路文件搬运失败必须向外抛错");
        assert!(
            !new_dir.join("cache.db").exists(),
            "主库必须被回滚，新目录不得以残缺状态存留"
        );
        assert_eq!(
            std::fs::read(legacy.join("cache.db")).unwrap(),
            b"legacy-db",
            "主库必须完整还原回 from_dir"
        );
        assert_eq!(
            std::fs::read(legacy.join("cache.db-wal")).unwrap(),
            b"legacy-wal",
            "失败的 WAL 必须原封不动留在 from_dir"
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

    #[test]
    fn test_resolve_data_dir_default_non_portable() {
        let root = temp_dir("non_portable");
        let exe = root.join("Transly.exe");
        let (dir, is_portable) =
            resolve_data_dir_internal("com.transly.desktop", Some(&exe), &[], None);

        assert!(!is_portable, "普通命名且无标记时必须是非便携模式");
        assert_eq!(dir, app_data_dir("com.transly.desktop"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_resolve_data_dir_portable_by_filename() {
        let root = temp_dir("portable_name");
        let exe = root.join("Transly-Portable-windows-x64.exe");
        let (dir, is_portable) =
            resolve_data_dir_internal("com.transly.desktop", Some(&exe), &[], None);

        assert!(is_portable, "文件名包含 portable 时应自动启用便携模式");
        assert_eq!(dir, root.join("data"));
        assert!(dir.is_dir(), "便携数据目录必须被自动创建");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_resolve_data_dir_portable_by_flag_file() {
        let root = temp_dir("portable_flag");
        let exe = root.join("Transly.exe");
        std::fs::write(root.join("portable.flag"), b"").unwrap();

        let (dir, is_portable) =
            resolve_data_dir_internal("com.transly.desktop", Some(&exe), &[], None);

        assert!(is_portable, "存在 portable.flag 时应自动启用便携模式");
        assert_eq!(dir, root.join("data"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_resolve_data_dir_portable_by_data_dir() {
        let root = temp_dir("portable_data_dir");
        let exe = root.join("Transly.exe");
        std::fs::create_dir_all(root.join("data")).unwrap();

        let (dir, is_portable) =
            resolve_data_dir_internal("com.transly.desktop", Some(&exe), &[], None);

        assert!(is_portable, "存在 data 目录时应自动启用便携模式");
        assert_eq!(dir, root.join("data"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_resolve_data_dir_portable_by_cli_arg() {
        let root = temp_dir("portable_arg");
        let exe = root.join("Transly.exe");
        let args = vec!["Transly.exe".to_string(), "--portable".to_string()];

        let (dir, is_portable) =
            resolve_data_dir_internal("com.transly.desktop", Some(&exe), &args, None);

        assert!(is_portable, "--portable 命令行参数必须启用便携模式");
        assert_eq!(dir, root.join("data"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_resolve_data_dir_env_override_disable() {
        let root = temp_dir("portable_env_disable");
        let exe = root.join("Transly-Portable.exe");

        let (dir, is_portable) =
            resolve_data_dir_internal("com.transly.desktop", Some(&exe), &[], Some("0"));

        assert!(
            !is_portable,
            "TRANSLY_PORTABLE=0 必须强制关闭便携模式（即使文件名包含 portable）"
        );
        assert_eq!(dir, app_data_dir("com.transly.desktop"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_resolve_data_dir_env_override_enable() {
        let root = temp_dir("portable_env_enable");
        let exe = root.join("Transly.exe");

        let (dir, is_portable) =
            resolve_data_dir_internal("com.transly.desktop", Some(&exe), &[], Some("1"));

        assert!(is_portable, "TRANSLY_PORTABLE=1 必须强制开启便携模式");
        assert_eq!(dir, root.join("data"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_migrate_keyring_settings_to_db() {
        let cache = transly_core::TranslationCache::in_memory().unwrap();
        assert_eq!(cache.get_setting("__migrated_from_keyring").unwrap(), None);

        migrate_keyring_settings_to_db(&cache);
        assert_eq!(
            cache
                .get_setting("__migrated_from_keyring")
                .unwrap()
                .as_deref(),
            Some("true")
        );
        assert_eq!(
            cache.get_setting("custom_shortcut").unwrap().as_deref(),
            Some("Alt+Shift+D")
        );
        assert_eq!(
            cache
                .get_setting("preferred_replace_engine")
                .unwrap()
                .as_deref(),
            Some("google")
        );

        // 二次调用幂等性测试
        cache.set_setting("custom_shortcut", "Ctrl+T").unwrap();
        migrate_keyring_settings_to_db(&cache);
        assert_eq!(
            cache.get_setting("custom_shortcut").unwrap().as_deref(),
            Some("Ctrl+T")
        );
    }
}
