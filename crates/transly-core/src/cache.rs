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
            CREATE INDEX IF NOT EXISTS idx_cache_created_at ON translation_cache(created_at);

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

    /// 流式导出翻译历史到任意 Write 流中（支持 csv 与 json），避免大记录集一次性内存加载
    pub fn export_history_to_writer<W: std::io::Write>(
        &self,
        writer: &mut W,
        format: &str,
        keyword: Option<&str>,
    ) -> Result<usize> {
        let is_csv = format.eq_ignore_ascii_case("csv");
        let kw = keyword.map(|s| s.trim()).filter(|s| !s.is_empty());

        // 导出全程独占一个**独立**只读连接，绝不占用共享 reader。
        //
        // 共享 `reader` 是全应用唯一的读连接互斥锁：缓存查询、划词查词、插件整页
        // 批量查询都走它。导出 10 万条会持续数秒并夹杂磁盘 I/O，若占用共享锁，
        // 期间上述链路全部排队阻塞（表现为界面与插件翻译假死）。
        // WAL 模式下独立只读连接与主读写连接互不干扰，因此这里单独开一个。
        let dedicated = match &self.backend {
            CacheBackend::File { path, .. } => {
                let conn = Connection::open_with_flags(
                    path,
                    rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                        | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                // 必须与主连接一致地设置 busy_timeout：默认 0ms 会让只读连接在
                // 写事务/WAL checkpoint 的竞争瞬间直接抛 `database is locked`，
                // 导致导出中途失败。query_only 则是只读连接的纵深防御。
                conn.execute_batch(
                    r#"
                    PRAGMA busy_timeout = 5000;
                    PRAGMA query_only = ON;
                    "#,
                )?;
                Some(conn)
            }
            CacheBackend::Memory { .. } => None,
        };

        let mut run_export = |conn: &Connection| -> Result<usize> {
            let mut count = 0usize;
            if is_csv {
                // 写入 UTF-8 BOM，确保 Excel 等软件打开中文不乱码
                writer
                    .write_all(b"\xEF\xBB\xBF")
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                writer
                    .write_all(b"Source Text,Translated Text,Source Lang,Target Lang,Engine,Time\n")
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            } else {
                writer
                    .write_all(b"[\n")
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            }

            let mut process_row = |row: &rusqlite::Row| -> std::io::Result<()> {
                let cache_key: String = row.get(0).unwrap_or_default();
                let source_text: String = row.get(1).unwrap_or_default();
                let translated_text: String = row.get(2).unwrap_or_default();
                let source_lang: String = row.get(3).unwrap_or_default();
                let target_lang: String = row.get(4).unwrap_or_default();
                let engine: String = row.get(5).unwrap_or_default();
                let created_at: i64 = row.get(6).unwrap_or_default();

                if is_csv {
                    let escape_field = |val: &str| -> String {
                        let guarded = if val.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                            format!("'{val}")
                        } else {
                            val.to_string()
                        };
                        format!("\"{}\"", guarded.replace('"', "\"\""))
                    };

                    let time_str = format_timestamp_iso(created_at);
                    let line = format!(
                        "{},{},{},{},{},{}\n",
                        escape_field(&source_text),
                        escape_field(&translated_text),
                        escape_field(&source_lang),
                        escape_field(&target_lang),
                        escape_field(&engine),
                        time_str
                    );
                    writer.write_all(line.as_bytes())?;
                } else {
                    let entry = CacheEntry {
                        cache_key,
                        source_text,
                        translated_text,
                        source_lang,
                        target_lang,
                        engine,
                        created_at: created_at as u32,
                    };
                    if count > 0 {
                        writer.write_all(b",\n")?;
                    }
                    let json_line = serde_json::to_string(&entry).unwrap_or_default();
                    writer.write_all(json_line.as_bytes())?;
                }
                count += 1;
                Ok(())
            };

            if let Some(keyword) = kw {
                let escaped: String = keyword
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
                    "#,
                )?;
                let mut rows = stmt.query(params![pattern])?;
                while let Some(row) = rows.next()? {
                    process_row(row)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                }
            } else {
                let mut stmt = conn.prepare_cached(
                    r#"
                    SELECT cache_key, source_text, translated_text, source_lang, target_lang, engine, created_at
                    FROM translation_cache
                    ORDER BY created_at DESC, rowid DESC
                    "#,
                )?;
                let mut rows = stmt.query([])?;
                while let Some(row) = rows.next()? {
                    process_row(row)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                }
            }

            if !is_csv {
                writer
                    .write_all(b"\n]")
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            }
            writer
                .flush()
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            Ok(count)
        };

        match dedicated.as_ref() {
            Some(conn) => run_export(conn),
            None => self.with_reader(run_export),
        }
    }
}

fn format_timestamp_iso(ts: i64) -> String {
    let ts = ts.max(0);
    let secs = ts % 86400;
    let days = ts / 86400;
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    // Howard Hinnant 民用历法算法（days-from-civil 的逆运算）。
    // 三个修正除数分别对应「4 年一闰」「100 年不闰」「400 年再闰」，
    // 必须是 1460 / 36524 / 146096，写成近似的 1024 / 1461 / 142401 会让 `yoe` 偏大 1，
    // 进而在 `doy = doe - (365*yoe + yoe/4 - yoe/100)` 触发下溢：
    // debug 构建 panic，release 构建产出 `1970-28071671-3435973839` 这类畸形时间戳。
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    format!("{y:04}-{m:02}-{d:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
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
    fn test_set_batch_atomic_transaction() {
        let cache = TranslationCache::in_memory().unwrap();
        let entries = vec![
            ("one", "一", "en", "zh-CN", "google"),
            ("two", "二", "en", "zh-CN", "google"),
            ("three", "三", "en", "zh-CN", "deepl"),
        ];

        cache.set_batch(entries).unwrap();

        assert_eq!(
            cache.get("one", "en", "zh-CN", "google").unwrap(),
            Some("一".to_string())
        );
        assert_eq!(
            cache.get("two", "en", "zh-CN", "google").unwrap(),
            Some("二".to_string())
        );
        assert_eq!(
            cache.get("three", "en", "zh-CN", "deepl").unwrap(),
            Some("三".to_string())
        );
    }

    /// 回归：SQLite 写入失败时绝不允许污染 L1 内存缓存。
    /// 旧实现先写 L1 再落库，事务失败后 L1 残留一条永不持久化的脏数据：
    /// 进程内读取会命中它，而数据库里根本没有对应记录，重启后译文凭空消失。
    #[test]
    fn test_set_batch_does_not_pollute_l1_when_transaction_fails() {
        let cache = TranslationCache::in_memory().unwrap();

        // 破坏表结构，使后续写入事务必然失败
        cache
            .with_writer(|conn| {
                conn.execute_batch("DROP TABLE translation_cache;")?;
                Ok(())
            })
            .unwrap();

        let entries = vec![
            ("one", "一", "en", "zh-CN", "google"),
            ("two", "二", "en", "zh-CN", "google"),
        ];
        assert!(
            cache.set_batch(entries).is_err(),
            "表被破坏后 set_batch 必须返回 Err，而不是静默成功"
        );

        assert!(
            cache.l1.lock().unwrap().map.is_empty(),
            "写入事务失败后 L1 内存缓存必须保持为空，不得残留脏数据"
        );
    }

    /// 回归：LIKE 模式中的 `%` 与 `_` 是通配符，用户输入的关键词若原样拼接会被
    /// 当作模式。搜索技术文档时 snake_case 标识符极常见，误命中稳定复现。
    #[test]
    fn test_search_escapes_like_wildcards() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.set("alpha", "阿尔法", "en", "zh-CN", "m").unwrap();
        cache.set("alXha", "假命中", "en", "zh-CN", "m").unwrap();
        cache.set("pct", "100% 完成", "en", "zh-CN", "m").unwrap();
        cache.set("al_ha", "精确命中", "en", "zh-CN", "m").unwrap();

        // `_` 必须按字面量匹配，不得把 alXha 也带出来
        let hits = cache.search("al_ha", 10, 0).unwrap();
        assert_eq!(hits.len(), 1, "下划线不得作为单字符通配符");
        assert_eq!(hits[0].source_text, "al_ha");

        // `%` 必须按字面量匹配，不得匹配空格
        let hits = cache.search("100%pct", 10, 0).unwrap();
        assert!(hits.is_empty(), "百分号不得作为任意字符通配符：{hits:?}");

        // 普通关键词仍走子串模糊匹配
        let hits = cache.search("alpha", 10, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source_text, "alpha");

        // 反斜杠本身也被正确转义，不得引发 SQL 语义变化
        let hits = cache.search("a\\lpha", 10, 0).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn test_clear_all_cache_retains_glossary() {
        let cache = TranslationCache::in_memory().unwrap();
        // 1. 插入翻译缓存与术语库
        cache.set("apple", "苹果", "en", "zh-CN", "m").unwrap();
        cache.set_glossary_term("LLM", "大模型", true).unwrap();

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
        assert_eq!(
            glossary_terms[0],
            ("LLM".to_string(), "大模型".to_string(), true)
        );
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
    fn test_search_translation_history() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.set("apple", "苹果", "en", "zh-CN", "m").unwrap();
        cache.set("banana", "香蕉", "en", "zh-CN", "m").unwrap();
        cache.set("pear", "梨子", "en", "zh-CN", "m").unwrap();

        // 原文搜索
        let res_source = cache.search("app", 10, 0).unwrap();
        assert_eq!(res_source.len(), 1);
        assert_eq!(res_source[0].source_text, "apple");

        // 译文搜索
        let res_target = cache.search("香蕉", 10, 0).unwrap();
        assert_eq!(res_target.len(), 1);
        assert_eq!(res_target[0].source_text, "banana");

        // 空搜索回落全部
        let res_empty = cache.search("", 10, 0).unwrap();
        assert_eq!(res_empty.len(), 3);
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
    fn test_concurrent_read_write_separation() {
        let temp_dir = std::env::temp_dir().join(format!(
            "transly_test_rw_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("cache.db");

        let cache = TranslationCache::new(&db_path).unwrap();
        cache.set("pre", "先验", "en", "zh-CN", "google").unwrap();

        let val = cache.get("pre", "en", "zh-CN", "google").unwrap();
        assert_eq!(val, Some("先验".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    /// 回归：文件后端下导出必须走**独立只读连接**，不得占用共享 reader。
    ///
    /// 复现的缺陷：导出循环整个包在 `with_reader` 闭包内，全程独占全局唯一的读连接
    /// 互斥锁；导出十万条期间缓存查询、划词查词与插件整页翻译全部排队阻塞。
    /// 内存后端不经过该分支，故必须在文件后端下覆盖。
    ///
    /// 判定方式必须是**确定性**的，不能靠计时：这里注入一个受控 Writer，
    /// 让导出在「持锁期间」停住不动，再从另一线程发起一次必定落到 SQLite 的读取
    /// （L1 为空，见下方说明）。若导出仍占用共享 reader，该读取会被阻塞到超时；
    /// 走独立连接则能立即返回。
    #[test]
    fn test_export_uses_dedicated_connection_without_blocking_reads() {
        use std::io::Write;
        use std::sync::mpsc;

        let dir = std::env::temp_dir().join(format!(
            "transly_test_export_dedicated_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("cache.db");

        // 用独立实例写入数据，使被测实例的 L1 保持为空 —— 否则 `get()` 会命中
        // L1 内存缓存而根本不碰 reader 锁，测试就永远无法反映真实争用。
        {
            let writer_cache = TranslationCache::new(&db_path).unwrap();
            for i in 0..20 {
                writer_cache
                    .set(
                        &format!("src{i}"),
                        &format!("译文{i}"),
                        "en",
                        "zh-CN",
                        "google",
                    )
                    .unwrap();
            }
        }

        let cache = std::sync::Arc::new(TranslationCache::new(&db_path).unwrap());

        // 受控 Writer：写出表头后即通知主线程「导出已持锁」，并等待放行信号。
        struct BlockingWriter {
            started: Option<mpsc::Sender<()>>,
            release: Option<mpsc::Receiver<()>>,
            buf: Vec<u8>,
        }
        impl Write for BlockingWriter {
            fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
                self.buf.extend_from_slice(data);
                // 只在**首次**写出时握手并停住，把导出钉在持锁区间内；
                // 后续写出必须畅通，否则导出永远走不到结束。
                if let (Some(tx), Some(rx)) = (self.started.take(), self.release.take()) {
                    let _ = tx.send(());
                    let _ = rx.recv();
                }
                Ok(data.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let mut writer = BlockingWriter {
            started: Some(started_tx),
            release: Some(release_rx),
            buf: Vec::new(),
        };

        let export_cache = std::sync::Arc::clone(&cache);
        let exporter = std::thread::spawn(move || {
            let n = export_cache
                .export_history_to_writer(&mut writer, "csv", None)
                .unwrap();
            (n, writer.buf)
        });

        // 等导出真正进入持锁状态
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("导出应已开始并写出表头");

        // 此时导出仍停在持锁区间内，发起一次必定落到 SQLite 的读取
        let (read_tx, read_rx) = mpsc::channel();
        let read_cache = std::sync::Arc::clone(&cache);
        std::thread::spawn(move || {
            let v = read_cache.get("src7", "en", "zh-CN", "google");
            let _ = read_tx.send(v);
        });

        let read_result = read_rx.recv_timeout(std::time::Duration::from_secs(3));

        // 无论结果如何都要放行导出，避免测试失败时挂死线程
        let _ = release_tx.send(());

        let read_value = read_result
            .expect("导出进行期间读取不得被阻塞（共享 reader 被独占即会超时）")
            .expect("读取不应报错");
        assert_eq!(
            read_value,
            Some("译文7".to_string()),
            "导出期间共享读连接必须仍然可用且数据正确"
        );

        let (count, buf) = exporter.join().unwrap();
        assert_eq!(count, 20, "导出必须覆盖全部条目");
        let csv = String::from_utf8(buf).unwrap();
        assert!(csv.contains("\"src7\",\"译文7\""), "导出内容必须完整");

        let _ = std::fs::remove_dir_all(&dir);
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

    #[test]
    fn test_app_settings_persistence_and_update() {
        let cache = TranslationCache::in_memory().unwrap();
        // 初始为空
        assert_eq!(cache.get_setting("custom_shortcut").unwrap(), None);

        // 写入并读取
        cache.set_setting("custom_shortcut", "Alt+Shift+T").unwrap();
        assert_eq!(
            cache.get_setting("custom_shortcut").unwrap().as_deref(),
            Some("Alt+Shift+T")
        );

        // 覆盖更新 (upsert)
        cache.set_setting("custom_shortcut", "Alt+Shift+X").unwrap();
        assert_eq!(
            cache.get_setting("custom_shortcut").unwrap().as_deref(),
            Some("Alt+Shift+X")
        );

        // 删除设置
        cache.delete_setting("custom_shortcut").unwrap();
        assert_eq!(cache.get_setting("custom_shortcut").unwrap(), None);
    }

    #[test]
    fn test_migrate_legacy_glossary_table_adds_match_case() {
        let dir = std::env::temp_dir().join(format!(
            "transly_test_legacy_glossary_{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let db_path = dir.join("legacy_cache.db");

        // 1. 模拟旧版本（缺少 match_case 列）的 SQLite 数据库
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE glossary (
                    source_term TEXT PRIMARY KEY,
                    target_term TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                INSERT INTO glossary VALUES ('apple', '苹果', 1000);
                "#,
            )
            .unwrap();
        }

        // 2. 用新版本 TranslationCache 打开，验证自动迁移 match_case 列
        let cache = TranslationCache::new(&db_path).unwrap();
        let terms = cache.get_all_glossary_terms().unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0], ("apple".to_string(), "苹果".to_string(), true));

        // 3. 验证新插入带 match_case 的词条
        cache.set_glossary_term("banana", "香蕉", false).unwrap();
        let updated = cache.get_all_glossary_terms().unwrap();
        assert_eq!(updated.len(), 2);
        assert_eq!(
            updated[1],
            ("banana".to_string(), "香蕉".to_string(), false)
        );

        drop(cache);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_get_by_keys_and_l1_cache() {
        let cache = TranslationCache::in_memory().unwrap();
        let k1 = TranslationCache::generate_key("hello", "en", "zh-CN", "google");
        let k2 = TranslationCache::generate_key("world", "en", "zh-CN", "google");
        let k3 = TranslationCache::generate_key("transly", "en", "zh-CN", "google");

        cache.set("hello", "你好", "en", "zh-CN", "google").unwrap();
        cache.set("world", "世界", "en", "zh-CN", "google").unwrap();

        // 批量查询存在的 k1, k2 以及不存在的 k3
        let batch_hits = cache
            .get_by_keys(&[k1.clone(), k2.clone(), k3.clone()])
            .unwrap();
        assert_eq!(batch_hits.len(), 2);
        assert_eq!(batch_hits.get(&k1).map(|s| s.as_str()), Some("你好"));
        assert_eq!(batch_hits.get(&k2).map(|s| s.as_str()), Some("世界"));
        assert_eq!(batch_hits.get(&k3), None);

        // 验证 L1 内存直出：再次查询时无需访问底层 DB
        let l1_hits = cache.get_by_keys(std::slice::from_ref(&k1)).unwrap();
        assert_eq!(l1_hits.get(&k1).map(|s| s.as_str()), Some("你好"));

        // 清空缓存后 L1 亦同步失效
        cache.clear_all_cache().unwrap();
        let empty_hits = cache.get_by_keys(std::slice::from_ref(&k1)).unwrap();
        assert!(empty_hits.is_empty());
    }

    #[test]
    fn test_export_history_to_writer_csv_and_json() {
        let cache = TranslationCache::in_memory().unwrap();
        cache.set("apple", "苹果", "en", "zh-CN", "google").unwrap();
        cache
            .set("banana", "香蕉", "en", "zh-CN", "google")
            .unwrap();

        // 1. 测试 CSV 流式导出
        let mut csv_buf = Vec::new();
        let count_csv = cache
            .export_history_to_writer(&mut csv_buf, "csv", None)
            .unwrap();
        assert_eq!(count_csv, 2);
        let csv_str = String::from_utf8(csv_buf).unwrap();
        assert!(csv_str.starts_with("\u{feff}Source Text,Translated Text"));
        assert!(csv_str.contains("\"apple\",\"苹果\""));
        assert!(csv_str.contains("\"banana\",\"香蕉\""));

        // 2. 测试 JSON 流式导出
        let mut json_buf = Vec::new();
        let count_json = cache
            .export_history_to_writer(&mut json_buf, "json", None)
            .unwrap();
        assert_eq!(count_json, 2);
        let json_str = String::from_utf8(json_buf).unwrap();
        assert!(json_str.starts_with("[\n"));
        assert!(json_str.ends_with("\n]"));
        assert!(json_str.contains("\"source_text\":\"apple\""));

        // 3. 测试带关键词过滤导出
        let mut filtered_buf = Vec::new();
        let count_filter = cache
            .export_history_to_writer(&mut filtered_buf, "csv", Some("apple"))
            .unwrap();
        assert_eq!(count_filter, 1);
        let filtered_str = String::from_utf8(filtered_buf).unwrap();
        assert!(filtered_str.contains("\"apple\""));
        assert!(!filtered_str.contains("\"banana\""));
        assert!(
            filtered_str.contains("T") && filtered_str.contains("Z\n"),
            "CSV 导出必须包含合法的 ISO-8601 时间戳: {filtered_str}"
        );

        // 4. 验证 format_timestamp_iso 边界与非负钳制
        assert_eq!(format_timestamp_iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_timestamp_iso(-100), "1970-01-01T00:00:00Z");
    }

    /// 时间戳格式化必须与权威 UTC 日历一致。
    ///
    /// 期望值由独立可信来源（JS `Date.prototype.toISOString`）生成，
    /// 而非从本函数当前输出反推——后者正是 LESSONS.md 第 11 条所警示的
    /// 「迎合实现的假测试」。
    #[test]
    fn test_format_timestamp_iso_matches_calendar() {
        // (unix 秒, 期望的 ISO-8601 UTC 串)；覆盖闰年、闰世纪、非闰世纪与日常日期
        let cases = [
            (0i64, "1970-01-01T00:00:00Z"),
            (86_400, "1970-01-02T00:00:00Z"),
            (777_600, "1970-01-10T00:00:00Z"),
            (951_782_400, "2000-02-29T00:00:00Z"), // 闰世纪（400 年一闰）
            (1_046_476_800, "2003-03-01T00:00:00Z"),
            (1_708_992_000, "2024-02-27T00:00:00Z"), // 普通闰年 2 月
            (1_709_942_400, "2024-03-09T00:00:00Z"),
            (1_767_225_600, "2026-01-01T00:00:00Z"),
            (1_769_990_399, "2026-02-01T23:59:59Z"), // 带时分秒
            (1_798_761_600, "2027-01-01T00:00:00Z"),
            (1_893_456_000, "2030-01-01T00:00:00Z"),
        ];
        for (ts, expected) in cases {
            assert_eq!(
                format_timestamp_iso(ts),
                expected,
                "时间戳 {ts} 的日历换算错误"
            );
        }
        // 负数一律钳制到 epoch，绝不产生负年份
        assert_eq!(format_timestamp_iso(-1), "1970-01-01T00:00:00Z");
        assert_eq!(format_timestamp_iso(i64::MIN), "1970-01-01T00:00:00Z");
    }

    /// 连续 60 年逐日扫描：确保没有任何一天产出越界/下溢的畸形字段。
    ///
    /// 只测几个边界值无法捕获民用历法修正除数写错的缺陷
    /// （错误实现下 `0` 与 `-100` 恰好也对），必须做全量扫描。
    #[test]
    fn test_format_timestamp_iso_no_malformed_field_over_decades() {
        for day in 0..(60 * 365 + 15) {
            let ts = day * 86_400;
            let out = format_timestamp_iso(ts);
            let date_part = &out[..10];
            let (y, m, d) = (
                date_part[0..4].parse::<u32>().expect("年份必须可解析"),
                date_part[5..7].parse::<u32>().expect("月份必须可解析"),
                date_part[8..10].parse::<u32>().expect("日必须可解析"),
            );
            assert!((1970..=2031).contains(&y), "年份越界: ts={ts} out={out}");
            assert!((1..=12).contains(&m), "月份越界: ts={ts} out={out}");
            assert!((1..=31).contains(&d), "日越界: ts={ts} out={out}");
            assert!(out.contains("T00:00:00Z"), "时分秒段异常: {out}");
        }
    }

    #[test]
    fn test_set_glossary_terms_batch_transaction() {
        let cache = TranslationCache::in_memory().unwrap();
        let count = cache
            .set_glossary_terms_batch([
                ("Rust", "锈语言", true),
                ("LLM", "大语言模型", false),
                ("   ", "无效空源", true),
            ])
            .unwrap();
        assert_eq!(count, 2);

        let all = cache.get_all_glossary_terms().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&("Rust".to_string(), "锈语言".to_string(), true)));
        assert!(all.contains(&("LLM".to_string(), "大语言模型".to_string(), false)));
    }
}
