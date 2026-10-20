//! 术语库、翻译历史、缓存开关、剪贴板识别与离线引擎状态命令。

use super::*;

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

#[tauri::command]
#[specta::specta]
pub async fn clear_cache(
    cache: tauri::State<'_, Arc<TranslationCache>>,
    cache_controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
) -> Result<u32, String> {
    // 必须走 CacheController 以递增 generation：否则在途请求会把刚清掉的记录写回。
    // DELETE 全表在 10 万条缓存下是长事务，必须移出主线程避免 UI 硬卡
    let cache_ref = cache.inner().clone();
    let controller = cache_controller.inner().clone();
    tokio::task::spawn_blocking(move || {
        controller
            .clear_only(&cache_ref)
            .map(|n| n as u32)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("缓存清理任务调度异常: {e}")))
}

/// 分页读取翻译历史（最近优先）。`limit` / `offset` 由前端控制。
#[tauri::command]
#[specta::specta]
pub async fn get_translation_history(
    limit: u32,
    offset: u32,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<Vec<CacheEntry>, String> {
    // SQLite 同步 I/O 禁止占用 async 工作线程
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .list_recent(limit, offset)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("翻译历史读取任务调度异常: {e}")))
}

/// 根据关键词检索翻译历史（支持模糊匹配原文与译文）
#[tauri::command]
#[specta::specta]
pub async fn search_translation_history(
    keyword: String,
    limit: u32,
    offset: u32,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<Vec<CacheEntry>, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .search(&keyword, limit, offset)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("翻译历史搜索任务调度异常: {e}")))
}

/// 按 `cache_key` 删除单条翻译历史。
#[tauri::command]
#[specta::specta]
pub async fn delete_history_entry(
    cache_key: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<bool, String> {
    // SQLite 同步 I/O 禁止占用 async 工作线程
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .delete_entry(&cache_key)
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(format!("翻译历史删除任务调度异常: {e}")))
}

#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ExportHistoryOutcome {
    pub count: u32,
    pub file_path: String,
}

fn resolve_downloads_dir() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(user) = std::env::var_os("USERPROFILE") {
            let p = std::path::PathBuf::from(user).join("Downloads");
            if p.is_dir() {
                return p;
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            let p = std::path::PathBuf::from(home).join("Downloads");
            if p.is_dir() {
                return p;
            }
        }
    }
    std::env::temp_dir()
}

/// 流式将翻译历史记录导出为本地文件（写入用户 Downloads 目录），零大内存 IPC 传输与 UI 阻塞
#[tauri::command]
#[specta::specta]
pub async fn export_translation_history(
    format: String,
    keyword: Option<String>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<ExportHistoryOutcome, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> Result<ExportHistoryOutcome, String> {
        let downloads_dir = resolve_downloads_dir();
        let ext = if format.eq_ignore_ascii_case("csv") {
            "csv"
        } else {
            "json"
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let filename = format!("transly-history-{now}.{ext}");
        let file_path = downloads_dir.join(&filename);
        let file = std::fs::File::create(&file_path)
            .map_err(|e| format!("创建导出文件失败 ({file_path:?}): {e}"))?;
        let mut writer = std::io::BufWriter::new(file);
        let count = cache_ref
            .export_history_to_writer(&mut writer, ext, keyword.as_deref())
            .map_err(|e| e.to_string())?;

        Ok(ExportHistoryOutcome {
            count: count as u32,
            file_path: file_path.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|e| format!("导出任务调度异常: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub async fn set_cache_enabled(
    enabled: bool,
    controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let cache_setting_ref = cache.inner().clone();
    let val = if enabled { "true" } else { "false" };
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_setting_ref
            .set_setting("cache_enabled", val)
            .map_err(|e| anyhow::anyhow!("配置保存失败: {e}"))?;
        let _ = crate::secrets::delete_secret("cache_enabled");
        Ok(())
    })
    .await
    .map_err(|e| format!("配置写入任务调度异常: {e}"))?
    .map_err(|e| e.to_string())?;

    if enabled {
        controller.enable();
        Ok(())
    } else {
        // 在同一门禁锁内递增 generation 并清空旧磁盘缓存，彻底原子闭环；
        // 清空为 SQLite 长事务，同样移出主线程
        let controller_ref = controller.inner().clone();
        let cache_ref = cache.inner().clone();
        tokio::task::spawn_blocking(move || controller_ref.disable_and_clear(&cache_ref))
            .await
            .map_err(|e| format!("缓存清空任务调度异常: {e}"))?
            .map(|_| ())
            .map_err(|e| format!("已禁用缓存，但清空历史缓存记录失败: {e}"))
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_cache_enabled(
    controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
) -> bool {
    controller.is_enabled()
}

#[tauri::command]
#[specta::specta]
pub async fn set_clipboard_monitor(
    enabled: bool,
    flag: tauri::State<'_, Arc<std::sync::atomic::AtomicBool>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let cache_ref = cache.inner().clone();
    let val = if enabled { "true" } else { "false" };
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_ref
            .set_setting("clipboard_monitor_enabled", val)
            .map_err(|e| anyhow::anyhow!("配置保存失败: {e}"))?;
        let _ = crate::secrets::delete_secret("clipboard_monitor_enabled");
        Ok(())
    })
    .await
    .map_err(|e| format!("配置保存调度异常: {e}"))?
    .map_err(|e| e.to_string())?;
    flag.store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_clipboard_monitor(app: tauri::AppHandle) -> bool {
    use tauri::Manager;
    let cache_ref = match app.try_state::<Arc<TranslationCache>>() {
        Some(c) => c.inner().clone(),
        None => return false,
    };
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("clipboard_monitor_enabled")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("clipboard_monitor_enabled").ok())
    })
    .await
    .map(|res| res.map(|v| v == "true").unwrap_or(false))
    .unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
pub async fn ocr_clipboard_image(
    target_lang: String,
    app: tauri::AppHandle,
) -> Result<String, transly_core::OcrErrorDto> {
    crate::ocr::recognize_clipboard_image(&app, &target_lang).await
}

#[tauri::command]
#[specta::specta]
pub async fn get_offline_engine_status(
    app: tauri::AppHandle,
) -> Result<transly_core::OfflineEngineStatusDto, String> {
    let (data_dir, _) = crate::storage::resolve_data_dir(&app.config().identifier);
    let models_dir = data_dir.join("models");
    let _ = std::fs::create_dir_all(&models_dir);

    let (has_local_model, local_model_name) =
        transly_core::engine::offline::inspect_local_models(&models_dir);

    Ok(transly_core::OfflineEngineStatusDto {
        lexicon_entries_count: transly_core::engine::offline::get_lexicon_entries_count(),
        has_local_model,
        local_model_name,
        model_dir: models_dir.to_string_lossy().to_string(),
    })
}
