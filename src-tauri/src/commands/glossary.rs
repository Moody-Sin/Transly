//! 术语库命令
//!
//! 由 `commands/mod.rs` 拆分而来：命令函数保持在 crate 内的原路径不变
//! （`mod.rs` 统一 `pub use` 重导出），因此 `app_command_builder!` 与
//! `src/bindings.ts` 的生成结果不受影响。

use std::sync::Arc;

use tokio::sync::RwLock;
use transly_core::{Glossary, TranslationCache};

#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct GlossaryTermDto {
    pub source: String,
    pub target: String,
    pub match_case: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn add_glossary_term(
    source: String,
    target: String,
    match_case: Option<bool>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<(), String> {
    // SQLite 写入为同步阻塞 I/O，禁止占用 async 运行时工作线程
    let is_match_case = match_case.unwrap_or(true);
    let cache_ref = cache.inner().clone();
    let (write_source, write_target) = (source.clone(), target.clone());
    tokio::task::spawn_blocking(move || {
        cache_ref.set_glossary_term(&write_source, &write_target, is_match_case)
    })
    .await
    .map_err(|e| format!("术语写入任务调度异常: {e}"))?
    .map_err(|e| e.to_string())?;
    let mut g = glossary.write().await;
    g.insert_with_options(source, target, is_match_case);
    Ok(())
}

/// 在单个 SQLite 事务与单次内存写锁内批量导入术语库词条，避免 CSV 批量导入逐条 IPC 与 fsync。
#[tauri::command]
#[specta::specta]
pub async fn import_glossary_terms(
    terms: Vec<GlossaryTermDto>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<u32, String> {
    let valid_terms: Vec<GlossaryTermDto> = terms
        .into_iter()
        .filter_map(|t| {
            let src = t.source.trim().to_string();
            let tgt = t.target.trim().to_string();
            if src.is_empty() || tgt.is_empty() {
                None
            } else {
                Some(GlossaryTermDto {
                    source: src,
                    target: tgt,
                    match_case: t.match_case,
                })
            }
        })
        .collect();

    if valid_terms.is_empty() {
        return Ok(0);
    }

    let cache_ref = cache.inner().clone();
    let db_terms = valid_terms
        .iter()
        .map(|t| (t.source.clone(), t.target.clone(), t.match_case))
        .collect::<Vec<_>>();

    let count = tokio::task::spawn_blocking(move || {
        let refs = db_terms
            .iter()
            .map(|(s, t, m)| (s.as_str(), t.as_str(), *m));
        cache_ref.set_glossary_terms_batch(refs)
    })
    .await
    .map_err(|e| format!("术语批量写入调度异常: {e}"))?
    .map_err(|e| e.to_string())?;

    let mut g = glossary.write().await;
    for item in valid_terms {
        g.insert_with_options(item.source, item.target, item.match_case);
    }

    Ok(count as u32)
}

#[tauri::command]
#[specta::specta]
pub async fn get_glossary_terms(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<Vec<GlossaryTermDto>, String> {
    // SQLite 同步 I/O 必须移出主线程：全表查询在缓存膨胀时可达数十毫秒
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_all_glossary_terms()
            .map(|terms| {
                terms
                    .into_iter()
                    .map(|(source, target, match_case)| GlossaryTermDto {
                        source,
                        target,
                        match_case,
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("术语库读取任务调度异常: {e}")))
}

#[tauri::command]
#[specta::specta]
pub async fn delete_glossary_term(
    source: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<(), String> {
    let cache_ref = cache.inner().clone();
    let deleted_key = source.clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .delete_glossary_term(&deleted_key)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("术语删除任务调度异常: {e}"))?
    .map_err(|e| e.to_string())?;

    // 增量从内存术语库中移除该词条（自动标记 dirty 并使已编译缓存失效，杜绝全量查库与全量重建）
    let mut g = glossary.write().await;
    g.remove(&source);
    Ok(())
}
