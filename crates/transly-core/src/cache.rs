use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::Result;

pub struct TranslationCache {
    conn: Mutex<Connection>,
}

impl TranslationCache {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            CREATE TABLE IF NOT EXISTS translation_cache (
                cache_key TEXT PRIMARY KEY,
                source_text TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                source_lang TEXT NOT NULL,
                target_lang TEXT NOT NULL,
                engine TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cache_created_at ON translation_cache(created_at);
            "#,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS translation_cache (
                cache_key TEXT PRIMARY KEY,
                source_text TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                source_lang TEXT NOT NULL,
                target_lang TEXT NOT NULL,
                engine TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn default_shared() -> Arc<Self> {
        let cache = dirs_cache_path()
            .and_then(|p| Self::new(p).ok())
            .or_else(|| Self::in_memory().ok())
            .unwrap_or_else(|| Self {
                conn: Mutex::new(Connection::open_in_memory().unwrap()),
            });
        Arc::new(cache)
    }

    fn generate_key(text: &str, source_lang: &str, target_lang: &str, engine: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        hasher.update(b":");
        hasher.update(source_lang.as_bytes());
        hasher.update(b":");
        hasher.update(target_lang.as_bytes());
        hasher.update(b":");
        hasher.update(engine.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    pub fn get(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
        engine: &str,
    ) -> Result<Option<String>> {
        let key = Self::generate_key(text, source_lang, target_lang, engine);
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let mut stmt = conn
            .prepare_cached("SELECT translated_text FROM translation_cache WHERE cache_key = ?1")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let res: String = row.get(0)?;
            Ok(Some(res))
        } else {
            Ok(None)
        }
    }

    pub fn set(
        &self,
        text: &str,
        translated: &str,
        source_lang: &str,
        target_lang: &str,
        engine: &str,
    ) -> Result<()> {
        let key = Self::generate_key(text, source_lang, target_lang, engine);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        conn.execute(
            r#"
            INSERT OR REPLACE INTO translation_cache 
            (cache_key, source_text, translated_text, source_lang, target_lang, engine, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![key, text, translated, source_lang, target_lang, engine, now],
        )?;

        Ok(())
    }

    /// 清理超过指定天数的过期缓存
    pub fn cleanup_older_than_days(&self, days: u32) -> Result<usize> {
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - (days as i64 * 86400);

        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let deleted = conn.execute(
            "DELETE FROM translation_cache WHERE created_at < ?1",
            params![cutoff],
        )?;

        Ok(deleted)
    }

    /// 限制缓存总条目数，保留最新的 max_items 条
    pub fn trim_to_max_items(&self, max_items: usize) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let deleted = conn.execute(
            r#"
            DELETE FROM translation_cache
            WHERE cache_key NOT IN (
                SELECT cache_key FROM translation_cache
                ORDER BY created_at DESC
                LIMIT ?1
            )
            "#,
            params![max_items as i64],
        )?;

        Ok(deleted)
    }
}

fn dirs_cache_path() -> Option<std::path::PathBuf> {
    // 优先读取用户应用数据目录
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA")?;
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|h| std::path::PathBuf::from(h).join("Library/Application Support"))?;
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME").or_else(|| {
        std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
    })?;

    let dir = std::path::PathBuf::from(base).join("Transly");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("cache.db"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_set_and_get() {
        let cache = TranslationCache::in_memory().expect("in memory cache should succeed");
        let text = "Hello world";
        let translated = "你好，世界";

        let miss = cache
            .get(text, "en", "zh-CN", "mock")
            .expect("query should succeed");
        assert_eq!(miss, None);

        cache
            .set(text, translated, "en", "zh-CN", "mock")
            .expect("insert should succeed");
        let hit = cache
            .get(text, "en", "zh-CN", "mock")
            .expect("query should succeed");
        assert_eq!(hit, Some(translated.to_string()));
    }

    #[test]
    fn test_cache_cleanup_and_trim() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.set("1", "一", "en", "zh-CN", "m").unwrap();
        cache.set("2", "二", "en", "zh-CN", "m").unwrap();
        cache.set("3", "三", "en", "zh-CN", "m").unwrap();

        // trim 保留最新 2 条
        let deleted = cache.trim_to_max_items(2).unwrap();
        assert_eq!(deleted, 1);

        // cleanup 不会误删当前刚插入的条目
        let expired = cache.cleanup_older_than_days(1).unwrap();
        assert_eq!(expired, 0);
    }
}
