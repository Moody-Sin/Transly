use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Result;

/// 翻译历史单条记录（对外暴露给 UI 的只读视图）。
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

struct L1Cache {
    map: std::collections::HashMap<String, String>,
    order: std::collections::VecDeque<String>,
    capacity: usize,
}

impl L1Cache {
    fn new(capacity: usize) -> Self {
        Self {
            map: std::collections::HashMap::with_capacity(capacity),
            order: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get(&self, key: &str) -> Option<String> {
        self.map.get(key).cloned()
    }

    fn insert(&mut self, key: String, val: String) {
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.map.entry(key.clone()) {
            e.insert(val);
            if let Some(pos) = self.order.iter().position(|k| k == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key);
            return;
        }
        while self.map.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            } else {
                break;
            }
        }
        if self.order.len() > self.capacity * 2 {
            self.order.retain(|k| self.map.contains_key(k));
        }
        self.order.push_back(key.clone());
        self.map.insert(key, val);
    }

    fn remove(&mut self, key: &str) {
        if self.map.remove(key).is_some() {
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
        }
    }

    fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}

enum CacheBackend {
    File {
        writer: Mutex<Connection>,
        reader: Mutex<Connection>,
        /// 数据库文件路径，供大体量导出开启独立只读连接。
        ///
        /// 导出会持续数秒并伴随磁盘 I/O，若复用共享 `reader`（全局唯一互斥锁），
        /// 期间所有缓存查询、划词查词与插件整页翻译都会被阻塞。
        /// WAL 模式下独立只读连接与主连接互不干扰，故专门保留此路径。
        path: std::path::PathBuf,
    },
    Memory {
        conn: Mutex<Connection>,
    },
}

pub struct TranslationCache {
    backend: CacheBackend,
    l1: Mutex<L1Cache>,
}

impl TranslationCache {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let p = path.as_ref();
        let writer = Connection::open(p)?;
        writer.execute_batch(
            r#"
            PRAGMA busy_timeout = 5000;
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
            CREATE INDEX IF NOT EXISTS idx_cache_created_at ON translation_cache(created_at DESC);

            CREATE TABLE IF NOT EXISTS glossary (
                source_term TEXT PRIMARY KEY,
                target_term TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                match_case INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;

        // 针对旧版本（v0.1 / v0.2）SQLite 文件执行幂等迁移，补充缺失的 match_case 列
        let _ = writer.execute(
            "ALTER TABLE glossary ADD COLUMN match_case INTEGER NOT NULL DEFAULT 1",
            [],
        );

        // 读连接独立打开，专供高频非阻塞并发查询（WAL 模式下完全不阻塞写操作）
        let reader = Connection::open_with_flags(
            p,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        reader.execute_batch(
            r#"
            PRAGMA busy_timeout = 5000;
            PRAGMA query_only = ON;
            PRAGMA synchronous = NORMAL;
            "#,
        )?;

        Ok(Self {
            backend: CacheBackend::File {
                writer: Mutex::new(writer),
                reader: Mutex::new(reader),
                path: p.to_path_buf(),
            },
            l1: Mutex::new(L1Cache::new(4096)),
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
            CREATE INDEX IF NOT EXISTS idx_cache_created_at ON translation_cache(created_at DESC);

            CREATE TABLE IF NOT EXISTS glossary (
                source_term TEXT PRIMARY KEY,
                target_term TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                match_case INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        Ok(Self {
            backend: CacheBackend::Memory {
                conn: Mutex::new(conn),
            },
            l1: Mutex::new(L1Cache::new(4096)),
        })
    }

    /// 在指定应用数据目录下打开持久化缓存，失败时依次回退到内存缓存。
    pub fn open_default(data_dir: impl AsRef<Path>) -> Arc<Self> {
        let p = data_dir.as_ref();
        let _ = std::fs::create_dir_all(p);
        let cache = Self::new(p.join("cache.db"))
            .or_else(|_| Self::in_memory())
            .unwrap_or_else(|_| Self {
                backend: CacheBackend::Memory {
                    conn: Mutex::new(
                        Connection::open_in_memory().expect("内存耗尽，无法建立内存缓存连接"),
                    ),
                },
                l1: Mutex::new(L1Cache::new(4096)),
            });
        Arc::new(cache)
    }

    #[inline]
    fn with_reader<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Connection) -> Result<R>,
    {
        match &self.backend {
            CacheBackend::File { reader, .. } => {
                let guard = reader
                    .lock()
                    .map_err(|e| crate::Error::Internal(e.to_string()))?;
                f(&guard)
            }
            CacheBackend::Memory { conn } => {
                let guard = conn
                    .lock()
                    .map_err(|e| crate::Error::Internal(e.to_string()))?;
                f(&guard)
            }
        }
    }

    #[inline]
    fn with_writer<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Connection) -> Result<R>,
    {
        match &self.backend {
            CacheBackend::File { writer, .. } => {
                let mut guard = writer
                    .lock()
                    .map_err(|e| crate::Error::Internal(e.to_string()))?;
                f(&mut guard)
            }
            CacheBackend::Memory { conn } => {
                let mut guard = conn
                    .lock()
                    .map_err(|e| crate::Error::Internal(e.to_string()))?;
                f(&mut guard)
            }
        }
    }

    pub fn generate_key(text: &str, source_lang: &str, target_lang: &str, engine: &str) -> String {
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
        if let Ok(guard) = self.l1.lock() {
            if let Some(hit) = guard.get(&key) {
                return Ok(Some(hit));
            }
        }

        let db_res = self.with_reader(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT translated_text FROM translation_cache WHERE cache_key = ?1",
            )?;
            let mut rows = stmt.query(params![key])?;

            if let Some(row) = rows.next()? {
                let res: String = row.get(0)?;
                Ok(Some(res))
            } else {
                Ok(None)
            }
        })?;

        if let Some(ref val) = db_res {
            if let Ok(mut guard) = self.l1.lock() {
                guard.insert(key, val.clone());
            }
        }

        Ok(db_res)
    }

    /// 批量按 keys 读取翻译缓存（优先 L1 内存直出，未命中部分单次批量 SQL IN 查询并写回 L1）
    pub fn get_by_keys(
        &self,
        keys: &[String],
    ) -> Result<std::collections::HashMap<String, String>> {
        let mut result = std::collections::HashMap::with_capacity(keys.len());
        let mut missing_keys = Vec::new();

        if let Ok(guard) = self.l1.lock() {
            for k in keys {
                if let Some(val) = guard.get(k) {
                    result.insert(k.clone(), val);
                } else {
                    missing_keys.push(k.as_str());
                }
            }
        } else {
            missing_keys.extend(keys.iter().map(|s| s.as_str()));
        }

        if missing_keys.is_empty() {
            return Ok(result);
        }

        let mut fetched_from_db = Vec::new();
        self.with_reader(|conn| {
            for chunk in missing_keys.chunks(100) {
                let placeholders: Vec<String> = (1..=chunk.len()).map(|i| format!("?{i}")).collect();
                let sql = format!(
                    "SELECT cache_key, translated_text FROM translation_cache WHERE cache_key IN ({})",
                    placeholders.join(",")
                );
                let mut stmt = conn.prepare(&sql)?;
                let params: Vec<&dyn rusqlite::ToSql> =
                    chunk.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
                let mut rows = stmt.query(params.as_slice())?;
                while let Some(row) = rows.next()? {
                    let k: String = row.get(0)?;
                    let v: String = row.get(1)?;
                    fetched_from_db.push((k.clone(), v.clone()));
                    result.insert(k, v);
                }
            }
            Ok(())
        })?;

        if let Ok(mut guard) = self.l1.lock() {
            for (k, v) in fetched_from_db {
                guard.insert(k, v);
            }
        }

        Ok(result)
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

        // 必须先落库成功再更新 L1：若先写内存，SQLite 写入失败时
        // L1 会残留一条永不持久化的脏数据，后续读取命中它而数据库里根本没有，
        // 造成「重启后译文凭空消失」的诡异现象。
        self.with_writer(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO translation_cache 
                (cache_key, source_text, translated_text, source_lang, target_lang, engine, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![key, text, translated, source_lang, target_lang, engine, now],
            )?;
            Ok(())
        })?;

        if let Ok(mut guard) = self.l1.lock() {
            guard.insert(key, translated.to_string());
        }

        Ok(())
    }

    /// 在单个显式事务中批量保存或更新翻译缓存，大幅降低磁盘 I/O 争用与锁开销
    pub fn set_batch<'a, I>(&self, entries: I) -> Result<()>
    where
        I: IntoIterator<Item = (&'a str, &'a str, &'a str, &'a str, &'a str)>,
    {
        let items: Vec<(&'a str, &'a str, &'a str, &'a str, &'a str)> =
            entries.into_iter().collect();
        if items.is_empty() {
            return Ok(());
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // 事务提交成功之后才允许更新 L1（原因同 `set`）。
        // 此处按引用遍历 items，以便事务成功后还能再遍历一次写入 L1。
        self.with_writer(|conn| {
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare_cached(
                    r#"
                    INSERT OR REPLACE INTO translation_cache 
                    (cache_key, source_text, translated_text, source_lang, target_lang, engine, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                    "#,
                )?;

                for (text, translated, source_lang, target_lang, engine) in &items {
                    let key = Self::generate_key(text, source_lang, target_lang, engine);
                    stmt.execute(params![
                        key,
                        text,
                        translated,
                        source_lang,
                        target_lang,
                        engine,
                        now
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })?;

        if let Ok(mut guard) = self.l1.lock() {
            for (text, translated, source_lang, target_lang, engine) in &items {
                let key = Self::generate_key(text, source_lang, target_lang, engine);
                guard.insert(key, translated.to_string());
            }
        }

        Ok(())
    }

    /// 清理超过指定天数的过期缓存
    pub fn cleanup_older_than_days(&self, days: u32) -> Result<usize> {
        if let Ok(mut guard) = self.l1.lock() {
            guard.clear();
        }

        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            - (days as i64 * 86400);

        self.with_writer(|conn| {
            let deleted = conn.execute(
                "DELETE FROM translation_cache WHERE created_at < ?1",
                params![cutoff],
            )?;
            Ok(deleted)
        })
    }

    /// 限制缓存总条目数，保留最新的 max_items 条（基于 created_at 索引高效删除）
    pub fn trim_to_max_items(&self, max_items: usize) -> Result<usize> {
        if let Ok(mut guard) = self.l1.lock() {
            guard.clear();
        }

        self.with_writer(|conn| {
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
        })
    }

    /// 读取持久化保存的所有术语库规则 (source_term, target_term, match_case)
    pub fn get_all_glossary_terms(&self) -> Result<Vec<(String, String, bool)>> {
        self.with_reader(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT source_term, target_term, match_case FROM glossary ORDER BY created_at ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let match_case_val: i64 = row.get(2)?;
                Ok((row.get(0)?, row.get(1)?, match_case_val != 0))
            })?;

            let mut terms = Vec::new();
            for item in rows {
                terms.push(item?);
            }
            Ok(terms)
        })
    }

    /// 持久化保存或更新术语（可指定是否区分大小写）
    pub fn set_glossary_term(&self, source: &str, target: &str, match_case: bool) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.with_writer(|conn| {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO glossary (source_term, target_term, created_at, match_case)
                VALUES (?1, ?2, ?3, ?4)
                "#,
                params![source, target, now, if match_case { 1 } else { 0 }],
            )?;
            Ok(())
        })
    }

    /// 在单个显式事务中批量保存或更新术语库规则，消除逐条导入时的重复磁盘 I/O 与锁开销
    pub fn set_glossary_terms_batch<'a, I>(&self, entries: I) -> Result<usize>
    where
        I: IntoIterator<Item = (&'a str, &'a str, bool)>,
    {
        let items: Vec<(&'a str, &'a str, bool)> = entries
            .into_iter()
            .filter(|(src, tgt, _)| !src.trim().is_empty() && !tgt.trim().is_empty())
            .collect();
        if items.is_empty() {
            return Ok(0);
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.with_writer(|conn| {
            let tx = conn.transaction()?;
            {
                let mut stmt = tx.prepare_cached(
                    r#"
                    INSERT OR REPLACE INTO glossary (source_term, target_term, created_at, match_case)
                    VALUES (?1, ?2, ?3, ?4)
                    "#,
                )?;
                for (source, target, match_case) in &items {
                    stmt.execute(params![
                        source,
                        target,
                        now,
                        if *match_case { 1 } else { 0 }
                    ])?;
                }
            }
            tx.commit()?;
            Ok(items.len())
        })
    }

    /// 删除指定术语
    pub fn delete_glossary_term(&self, source: &str) -> Result<()> {
        self.with_writer(|conn| {
            conn.execute(
                "DELETE FROM glossary WHERE source_term = ?1",
                params![source],
            )?;
            Ok(())
        })
    }

    /// 读取持久化应用配置（非敏感配置项）
    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.with_reader(|conn| {
            let mut stmt = conn.prepare_cached("SELECT value FROM app_settings WHERE key = ?1")?;
            let mut rows = stmt.query(params![key])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        })
    }

    /// 保存或更新持久化应用配置
    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.with_writer(|conn| {
            conn.execute(
                r#"
                INSERT INTO app_settings (key, value) VALUES (?1, ?2)
                ON CONFLICT(key) DO UPDATE SET value = excluded.value
                "#,
                params![key, value],
            )?;
            Ok(())
        })
    }

    /// 删除持久化应用配置
    pub fn delete_setting(&self, key: &str) -> Result<()> {
        self.with_writer(|conn| {
            conn.execute("DELETE FROM app_settings WHERE key = ?1", params![key])?;
            Ok(())
        })
    }

    /// 清空所有翻译缓存，并在磁盘后端执行 WAL Checkpoint 与 VACUUM 回收磁盘物理碎片
    pub fn clear_all_cache(&self) -> Result<usize> {
        if let Ok(mut guard) = self.l1.lock() {
            guard.clear();
        }

        self.with_writer(|conn| {
            let deleted = conn.execute("DELETE FROM translation_cache", [])?;
            // 对文件数据库执行空间物理整理与 WAL 日志截断：先 VACUUM 重整表空间，再截断 WAL
            if matches!(self.backend, CacheBackend::File { .. }) {
                let _ = conn.execute_batch(
                    r#"
                    VACUUM;
                    PRAGMA wal_checkpoint(TRUNCATE);
                    "#,
                );
            }
            Ok(deleted)
        })
    }

    /// 按时间倒序分页读取翻译历史（最近优先）。
    pub fn list_recent(&self, limit: u32, offset: u32) -> Result<Vec<CacheEntry>> {
        self.with_reader(|conn| {
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
                    created_at: row.get::<_, i64>(6)? as u32,
                })
            })?;

            let mut entries = Vec::new();
            for item in rows {
                entries.push(item?);
            }
            Ok(entries)
        })
    }

    /// 根据关键词模糊检索翻译历史（匹配原文或译文）
    pub fn search(&self, keyword: &str, limit: u32, offset: u32) -> Result<Vec<CacheEntry>> {
        let trimmed = keyword.trim();
        if trimmed.is_empty() {
            return self.list_recent(limit, offset);
        }

        self.with_reader(|conn| {
            // LIKE 的 `%` / `_` 是通配符，用户输入的关键词若原样拼接会被当作模式：
            // 搜 "al_ha" 会误命中 "alXha"，搜 "100%pct" 中的 % 会匹配任意字符。
            // 搜索技术文档时 snake_case 标识符极常见，必须转义并声明 ESCAPE 字符。
            let escaped: String = trimmed
                .chars()
                .flat_map(|c| match c {
                    '\\' => vec!['\\', '\\'],
                    '%' => vec!['\\', '%'],
                    '_' => vec!['\\', '_'],
                    other => vec![other],
                })
                .collect();
            let pattern = format!("%{escaped}%");
            let mut stmt = conn.prepare_cached(
                r#"
                SELECT cache_key, source_text, translated_text, source_lang, target_lang, engine, created_at
                FROM translation_cache
                WHERE source_text LIKE ?1 ESCAPE '\' OR translated_text LIKE ?1 ESCAPE '\'
                ORDER BY created_at DESC, rowid DESC
                LIMIT ?2 OFFSET ?3
                "#,
            )?;

            let rows = stmt.query_map(params![pattern, limit as i64, offset as i64], |row| {
                Ok(CacheEntry {
                    cache_key: row.get(0)?,
                    source_text: row.get(1)?,
                    translated_text: row.get(2)?,
                    source_lang: row.get(3)?,
                    target_lang: row.get(4)?,
                    engine: row.get(5)?,
                    created_at: row.get::<_, i64>(6)? as u32,
                })
            })?;

            let mut entries = Vec::new();
            for item in rows {
                entries.push(item?);
            }
            Ok(entries)
        })
    }

    /// 按 `cache_key` 删除单条翻译历史。返回是否确实删除了记录。
    pub fn delete_entry(&self, cache_key: &str) -> Result<bool> {
        if let Ok(mut guard) = self.l1.lock() {
            guard.remove(cache_key);
        }

        self.with_writer(|conn| {
            let deleted = conn.execute(
                "DELETE FROM translation_cache WHERE cache_key = ?1",
                params![cache_key],
            )?;
            Ok(deleted > 0)
        })
    }
}

mod export;

#[cfg(test)]
use export::format_timestamp_iso;

#[cfg(test)]
mod tests;
