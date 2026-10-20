//! 翻译历史流式导出（CSV / JSON）与时间戳格式化。
//!
//! 拆出独立文件只为控制 `cache` 模块体积，逻辑仍属于 `TranslationCache`。

use super::*;

impl TranslationCache {
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

pub(super) fn format_timestamp_iso(ts: i64) -> String {
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
