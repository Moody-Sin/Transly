use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Result;

/// 翻译历史单条记录（对外暴露给 UI 的只读视图）。
///
/// `created_at` 使用 `u32`（纪元秒）而非 `i64`：specta 会把 `i64` 导出为
/// JS `BigInt`，在 `JSON.stringify` 时直接抛错（见 Lessons Learned 第 1 条）。
/// `u32` 上限至 2106 年，对「最近翻译」语义绰绰有余。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CacheEntry {
    pub cache_key: String,
    pub source_text: String,
    pub translated_text: String,
    pub source_lang: String,
    pub target_lang: String,
    pub engine: String,
    pub created_at: u32,
}

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

            CREATE TABLE IF NOT EXISTS glossary (
                source_term TEXT PRIMARY KEY,
                target_term TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
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

            CREATE TABLE IF NOT EXISTS glossary (
                source_term TEXT PRIMARY KEY,
                target_term TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// 在指定应用数据目录下打开持久化缓存，失败时依次回退到内存缓存。
    ///
    /// 目录由调用方（平台适配层）提供：本 crate 不引入任何平台目录假设，
    /// 以便被 CLI / WASM 等无文件系统或不同目录约定的宿主复用。
    pub fn open_default(data_dir: impl AsRef<Path>) -> Arc<Self> {
        let cache = Self::new(data_dir.as_ref().join("cache.db"))
            .or_else(|_| Self::in_memory())
            // 最后兜底：仅当系统内存耗尽致 sqlite 无法分配连接时才会失败。
            // 此时进程已无法提供翻译服务，用 expect 显式暴露启动期故障，优于静默降级。
            .unwrap_or_else(|_| Self {
                conn: Mutex::new(
                    Connection::open_in_memory().expect("内存耗尽，无法建立内存缓存连接"),
                ),
            });
        Arc::new(cache)
    }

    fn generate_key(text: &str, source_lang: &str, target_lang: &str, engine: &str) -> String {
        let mut hasher = Sha256::new();
        // 使用明确的长度前缀编码，杜绝字段含冒号或分隔符碰撞
        hasher.update((text.len() as u64).to_le_bytes());
        hasher.update(text.as_bytes());
        hasher.update((source_lang.len() as u64).to_le_bytes());
        hasher.update(source_lang.as_bytes());
        hasher.update((target_lang.len() as u64).to_le_bytes());
        hasher.update(target_lang.as_bytes());
        hasher.update((engine.len() as u64).to_le_bytes());
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

    /// 限制缓存总条目数，保留最新的 max_items 条（基于 created_at 索引高效删除）
    pub fn trim_to_max_items(&self, max_items: usize) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let deleted = conn.execute(
            r#"
            DELETE FROM translation_cache 
            WHERE rowid IN (
                SELECT rowid FROM translation_cache 
                ORDER BY created_at DESC, rowid DESC 
                LIMIT -1 OFFSET ?1
            )
            "#,
            params![max_items as i64],
        )?;

        Ok(deleted)
    }

    /// 读取持久化保存的所有术语库规则
    pub fn get_all_glossary_terms(&self) -> Result<Vec<(String, String)>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let mut stmt = conn.prepare_cached("SELECT source_term, target_term FROM glossary")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;

        let mut terms = Vec::new();
        for item in rows {
            terms.push(item?);
        }
        Ok(terms)
    }

    /// 持久化保存或更新术语
    pub fn set_glossary_term(&self, source: &str, target: &str) -> Result<()> {
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
            INSERT OR REPLACE INTO glossary (source_term, target_term, created_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![source, target, now],
        )?;

        Ok(())
    }

    /// 删除指定术语
    pub fn delete_glossary_term(&self, source: &str) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        conn.execute(
            "DELETE FROM glossary WHERE source_term = ?1",
            params![source],
        )?;
        Ok(())
    }

    /// 清空所有翻译缓存
    pub fn clear_all_cache(&self) -> Result<usize> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let deleted = conn.execute("DELETE FROM translation_cache", [])?;
        Ok(deleted)
    }

    /// 按时间倒序分页读取翻译历史（最近优先）。
    ///
    /// 该表是去重缓存（`cache_key` 为主键，重复翻译同一文本会刷新 `created_at`），
    /// 因此结果语义为「近期不重复的翻译记录」，与 UI「最近翻译」的直觉一致。
    /// 排序附加 `rowid DESC` 作为时间相同时的稳定次序，避免分页出现重复/漏项。
    pub fn list_recent(&self, limit: u32, offset: u32) -> Result<Vec<CacheEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let mut stmt = conn.prepare_cached(
            r#"
            SELECT cache_key, source_text, translated_text, source_lang, target_lang, engine, created_at
            FROM translation_cache
            ORDER BY created_at DESC, rowid DESC
            LIMIT ?1 OFFSET ?2
            "#,
        )?;

        let rows = stmt.query_map(params![limit as i64, offset as i64], |row| {
            Ok(CacheEntry {
                cache_key: row.get(0)?,
                source_text: row.get(1)?,
                translated_text: row.get(2)?,
                source_lang: row.get(3)?,
                target_lang: row.get(4)?,
                engine: row.get(5)?,
                // 库内为 i64 纪元秒，正值恒可回落 u32（2106 年前）
                created_at: row.get::<_, i64>(6)? as u32,
            })
        })?;

        let mut entries = Vec::new();
        for item in rows {
            entries.push(item?);
        }
        Ok(entries)
    }

    /// 按 `cache_key` 删除单条翻译历史。返回是否确实删除了记录。
    pub fn delete_entry(&self, cache_key: &str) -> Result<bool> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::Error::Internal(e.to_string()))?;

        let deleted = conn.execute(
            "DELETE FROM translation_cache WHERE cache_key = ?1",
            params![cache_key],
        )?;
        Ok(deleted > 0)
    }
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

    #[test]
    fn test_clear_all_cache_retains_glossary() {
        let cache = TranslationCache::in_memory().unwrap();
        // 1. 插入翻译缓存与术语库
        cache.set("apple", "苹果", "en", "zh-CN", "m").unwrap();
        cache.set_glossary_term("LLM", "大模型").unwrap();

        assert_eq!(
            cache.get("apple", "en", "zh-CN", "m").unwrap(),
            Some("苹果".to_string())
        );
        assert_eq!(cache.get_all_glossary_terms().unwrap().len(), 1);

        // 2. 清空翻译缓存
        let cleared = cache.clear_all_cache().unwrap();
        assert_eq!(cleared, 1);

        // 3. 验证翻译缓存已空，但术语库依然完整保留
        assert_eq!(cache.get("apple", "en", "zh-CN", "m").unwrap(), None);
        let glossary_terms = cache.get_all_glossary_terms().unwrap();
        assert_eq!(glossary_terms.len(), 1);
        assert_eq!(glossary_terms[0], ("LLM".to_string(), "大模型".to_string()));
    }

    #[test]
    fn test_list_recent_orders_newest_first_and_paginates() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.set("one", "一", "en", "zh-CN", "m").unwrap();
        cache.set("two", "二", "en", "zh-CN", "m").unwrap();
        cache.set("three", "三", "en", "zh-CN", "m").unwrap();

        // 同一秒内写入时 created_at 相同，靠 rowid DESC 保证「后写入的在前」
        let all = cache.list_recent(50, 0).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].source_text, "three", "最近翻译必须排在首位");
        assert_eq!(all[2].source_text, "one");

        // 分页：第一页 2 条 + 第二页 1 条，且不重复不遗漏
        let page1 = cache.list_recent(2, 0).unwrap();
        let page2 = cache.list_recent(2, 2).unwrap();
        assert_eq!(page1.len(), 2);
        assert_eq!(page2.len(), 1);
        assert_eq!(page1[0].source_text, "three");
        assert_eq!(page1[1].source_text, "two");
        assert_eq!(page2[0].source_text, "one");

        // 字段映射正确（含 u32 时间戳，不得退化为 i64/BigInt）
        let entry = &all[0];
        assert_eq!(entry.translated_text, "三");
        assert_eq!(entry.source_lang, "en");
        assert_eq!(entry.target_lang, "zh-CN");
        assert_eq!(entry.engine, "m");
        assert!(entry.created_at > 0);
    }

    #[test]
    fn test_delete_entry_removes_only_target() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.set("keep", "留", "en", "zh-CN", "m").unwrap();
        cache.set("drop", "删", "en", "zh-CN", "m").unwrap();

        let target_key = cache
            .list_recent(10, 0)
            .unwrap()
            .into_iter()
            .find(|e| e.source_text == "drop")
            .expect("待删除条目必须存在")
            .cache_key;

        assert!(
            cache.delete_entry(&target_key).unwrap(),
            "命中删除应返回 true"
        );
        assert!(
            !cache.delete_entry(&target_key).unwrap(),
            "重复删除应返回 false"
        );

        let remaining = cache.list_recent(10, 0).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].source_text, "keep");
    }

    #[test]
    fn test_history_persists_across_file_reopen() {
        // 内存路径与文件路径共用同一 SQL 逻辑，但文件路径额外涉及
        // 建表、WAL 与「重开连接后仍可读」。历史面板在生产走文件路径，
        // 故此处覆盖内存测试触及不到的重开持久化。
        let dir = std::env::temp_dir().join(format!(
            "transly_history_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("cache.db");

        {
            let cache = TranslationCache::new(&db_path).unwrap();
            cache.set("persist", "持久", "en", "zh-CN", "m").unwrap();
        }

        // 重新打开同一文件，历史必须仍在（且字段完整）
        let reopened = TranslationCache::new(&db_path).unwrap();
        let entries = reopened.list_recent(10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].source_text, "persist");
        assert_eq!(entries[0].translated_text, "持久");
        assert!(entries[0].created_at > 0);

        drop(reopened);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
