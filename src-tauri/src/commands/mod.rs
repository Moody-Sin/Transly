use crate::engine_pool::EnginePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::task::classify_error;
use transly_core::{
    CacheEntry, Error, ErrorKind, Glossary, ProxyConfig, ProxyMode, TranslationCache,
    TranslationRequest,
};

/// 翻译成功结果：携带实际执行引擎，供 UI 展示与降级识别
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TranslateOutcomeDto {
    pub text: String,
    pub engine: String,
}

/// 结构化翻译错误：kind 供 UI 分类展示，retryable 供 UI 决定是否提供重试入口
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TranslateErrorDto {
    pub kind: ErrorKind,
    pub message: String,
    pub retryable: bool,
}

impl TranslateErrorDto {
    pub fn from_error(err: &Error) -> Self {
        let t = classify_error(err);
        Self {
            kind: t.kind,
            message: t.message,
            retryable: t.retryable,
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn translate(
    text: String,
    source_lang: Option<String>,
    target_lang: String,
    engine_name: Option<String>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    pool: tauri::State<'_, Arc<EnginePool>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
    cache_controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
) -> Result<TranslateOutcomeDto, TranslateErrorDto> {
    let sl = source_lang.unwrap_or_else(|| "auto".to_string());

    // 统一输入参数防线校验：全部走同一个错误构造路径，
    // 避免手工拼 DTO 与 classify_error 的重试语义漂移
    transly_core::validate_translation_input(&text, &sl, &target_lang)
        .map_err(|e| TranslateErrorDto::from_error(&e))?;

    if let Some(ref eng) = engine_name {
        if !transly_core::validate_engine_id(eng) {
            return Err(TranslateErrorDto::from_error(&Error::Input(format!(
                "不支持或未知的翻译引擎: {eng}"
            ))));
        }
    }

    // 从全局引擎池中快速复用长连接实例
    let active_engine = if let Some(ref name) = engine_name {
        pool.get_engine(name).await
    } else {
        pool.default_engine()
    };

    // 检查缓存状态快照与 generation 版本号
    let (cache_enabled, start_gen) = cache_controller.snapshot();

    // 0. 提取代码块保护后再应用全局术语库预替换：
    //    - 先 stash 代码块，防止代码字面量被术语库误改；
    //    - 再对非代码正文应用术语库，确保外文源术语在进入翻译引擎和计算缓存 key 前
    //      就已经被替换为目标术语（避免引擎把源术语译成别的词并固化进 SQLite 缓存，
    //      导致用户后续新增或修改术语时因命中旧缓存而失效）。
    let preserved = transly_core::stash_code_blocks(&text);
    let glossary_applied_source = {
        let g = glossary.read().await;
        if g.is_empty() {
            preserved.text.clone()
        } else {
            g.apply(&preserved.text)
        }
    };
    let effective_source =
        transly_core::restore_code_blocks(&glossary_applied_source, &preserved.stash);

    // 1. 若开启缓存，优先查缓存（按已应用源端术语的 effective_source 计算 key）
    if cache_enabled {
        let cached_res = {
            let cache_ref = cache.inner().clone();
            let engine_clone = active_engine.clone();
            let t = effective_source.clone();
            let s = sl.clone();
            let tl = target_lang.clone();
            tokio::task::spawn_blocking(move || {
                transly_core::lookup_cache_with_fallback(
                    &cache_ref,
                    engine_clone.as_ref(),
                    &t,
                    &s,
                    &tl,
                )
            })
            .await
            .unwrap_or(None)
        };

        if let Some((cached_text, used_engine)) = cached_res {
            let g = glossary.read().await;
            let applied = g.apply_to_translation(&cached_text);
            return Ok(TranslateOutcomeDto {
                text: applied,
                engine: used_engine,
            });
        }
    }

    // 2. 缓存未命中或未开启，调用引擎翻译
    let req = TranslationRequest {
        text: glossary_applied_source,
        source_lang: sl.clone(),
        target_lang: target_lang.clone(),
    };

    match active_engine.translate(&req).await {
        Ok(res) => {
            let restored = transly_core::restore_code_blocks(&res.text, &preserved.stash);
            // 3. 异步写入缓存（在同步门禁锁内同时验证 generation 版本与开启状态，消除判断与写入之间的竞态）
            let cache_target_engine = res.engine.clone();
            let cache_arc = cache.inner().clone();
            let controller = cache_controller.inner().clone();
            // 译文在闭包后仍要交给术语库，故入闭包前先克隆一份
            let restored_for_cache = restored.clone();
            crate::cache_controller::write_cache_offloaded(controller, start_gen, move || {
                cache_arc.set(
                    &effective_source,
                    &restored_for_cache,
                    &sl,
                    &target_lang,
                    &cache_target_engine,
                )
            })
            .await;
            let g = glossary.read().await;
            Ok(TranslateOutcomeDto {
                text: g.apply_to_translation(&restored),
                engine: res.engine,
            })
        }
        Err(e) => Err(TranslateErrorDto::from_error(&e)),
    }
}

/// 流式分块传输载荷（通过 Tauri Event 广播至指定窗口）
#[derive(Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct StreamChunkPayload {
    pub task_id: u32,
    pub chunk: String,
}

#[derive(Clone)]
pub struct TranslationDependencies {
    pub cache: Arc<TranslationCache>,
    pub pool: Arc<EnginePool>,
    pub glossary: Arc<RwLock<Glossary>>,
    pub cache_controller: Arc<crate::cache_controller::CacheController>,
    pub stream_registry: Arc<StreamTaskRegistry>,
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn translate_stream(
    task_id: u32,
    text: String,
    source_lang: Option<String>,
    target_lang: String,
    engine_name: Option<String>,
    window: tauri::Window,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
) -> Result<TranslateOutcomeDto, TranslateErrorDto> {
    use tauri::Emitter;

    let sl = source_lang.unwrap_or_else(|| "auto".to_string());

    transly_core::validate_translation_input(&text, &sl, &target_lang)
        .map_err(|e| TranslateErrorDto::from_error(&e))?;

    if let Some(ref eng) = engine_name {
        if !transly_core::validate_engine_id(eng) {
            return Err(TranslateErrorDto::from_error(&Error::Input(format!(
                "不支持或未知的翻译引擎: {eng}"
            ))));
        }
    }

    let active_engine = if let Some(ref name) = engine_name {
        deps.pool.get_engine(name).await
    } else {
        deps.pool.default_engine()
    };

    let (cache_enabled, start_gen) = deps.cache_controller.snapshot();

    // 0. 先提取代码块保护，再对非代码正文应用全局术语库预替换
    let preserved = transly_core::stash_code_blocks(&text);
    let glossary_applied_source = {
        let g = deps.glossary.read().await;
        if g.is_empty() {
            preserved.text.clone()
        } else {
            g.apply(&preserved.text)
        }
    };
    let effective_source =
        transly_core::restore_code_blocks(&glossary_applied_source, &preserved.stash);

    // 1. 若开启缓存，优先查缓存
    if cache_enabled {
        let cached_res = {
            let cache_ref = deps.cache.clone();
            let engine_clone = active_engine.clone();
            let t = effective_source.clone();
            let s = sl.clone();
            let tl = target_lang.clone();
            tokio::task::spawn_blocking(move || {
                transly_core::lookup_cache_with_fallback(
                    &cache_ref,
                    engine_clone.as_ref(),
                    &t,
                    &s,
                    &tl,
                )
            })
            .await
            .unwrap_or(None)
        };

        if let Some((cached_text, used_engine)) = cached_res {
            let g = deps.glossary.read().await;
            let applied = g.apply_to_translation(&cached_text);
            // 命中缓存时直接由 IPC 返回完整译文，不再重复发送 translate-stream-chunk 事件，
            // 避免前端在异步事件与 IPC resolve 竞争时将完整译文拼接两次或产生状态闪烁。
            return Ok(TranslateOutcomeDto {
                text: applied,
                engine: used_engine,
            });
        }
    }

    // 2. 缓存未命中或未开启，调用流式引擎翻译
    let req = TranslationRequest {
        text: glossary_applied_source,
        source_lang: sl.clone(),
        target_lang: target_lang.clone(),
    };

    let win_clone = window.clone();
    let on_chunk: transly_core::engine::StreamCallback = Box::new(move |chunk_str| {
        let _ = win_clone.emit(
            "translate-stream-chunk",
            StreamChunkPayload {
                task_id,
                chunk: chunk_str.to_string(),
            },
        );
    });

    // 注册任务取消信号。
    //
    // 键必须带上窗口标签：task_id 只是前端每窗口各自从 0 开始的局部计数
    // （`useTaskRunner` 里是 `useRef(0)`），而主窗口与划词浮窗是两个独立窗口。
    // 早期实现用全局 `HashMap<u32, _>`，浮窗发起取消时会直接杀掉主窗口
    // 正在运行的同名任务，表现为毫无线索的「内部错误」。
    let registry_key = (window.label().to_string(), task_id);

    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();
    if let Ok(mut map) = deps.stream_registry.0.lock() {
        map.insert(registry_key.clone(), cancel_tx);
    }

    let stream_fut = active_engine.translate_stream(&req, on_chunk);

    // 取消信号：只有「用户主动取消」（`Ok(())`）才算数。
    //
    // `cancel_rx` 返回 `Err(RecvError)` 的时机是「同键 sender 被覆盖/丢弃」，
    // 那属于注册表内部事件，不是用户意图。旧实现在该分支直接 return 报错，
    // 把一次本可正常完成的翻译变成「取消通道已失效」的伪错误，
    // 与旁边注释声明的语义自相矛盾。
    //
    // 这里让该情形**永久挂起**：select 随之完全由引擎结果决定，
    // 任务照常跑完并返回真实译文。
    let cancel_signal = async move {
        match cancel_rx.await {
            Ok(()) => {}
            Err(_) => std::future::pending::<()>().await,
        }
    };

    let res = tokio::select! {
        outcome = stream_fut => {
            if let Ok(mut map) = deps.stream_registry.0.lock() {
                map.remove(&registry_key);
            }
            outcome
        }
        _ = cancel_signal => {
            if let Ok(mut map) = deps.stream_registry.0.lock() {
                map.remove(&registry_key);
            }
            return Err(TranslateErrorDto::from_error(&Error::Internal(
                "流式任务已被前端主动取消".into(),
            )));
        }
    };

    match res {
        Ok(res) => {
            let restored = transly_core::restore_code_blocks(&res.text, &preserved.stash);
            let cache_target_engine = res.engine.clone();
            let cache_arc = Arc::clone(&deps.cache);
            let controller = Arc::clone(&deps.cache_controller);
            // 译文在闭包后仍要交给术语库，故入闭包前先克隆一份
            let restored_for_cache = restored.clone();
            crate::cache_controller::write_cache_offloaded(controller, start_gen, move || {
                cache_arc.set(
                    &effective_source,
                    &restored_for_cache,
                    &sl,
                    &target_lang,
                    &cache_target_engine,
                )
            })
            .await;
            let g = deps.glossary.read().await;
            Ok(TranslateOutcomeDto {
                text: g.apply_to_translation(&restored),
                engine: res.engine,
            })
        }
        Err(e) => Err(TranslateErrorDto::from_error(&e)),
    }
}

/// 流式任务的注册键：`(窗口标签, 窗口内序号)`。
type StreamTaskKey = (String, u32);
type StreamTaskSender = tokio::sync::oneshot::Sender<()>;

/// 全局并发流式翻译任务取消注册表。
///
/// 键为 `(窗口标签, task_id)`：前端每个窗口的 `task_id` 都是各自从 0 开始的
/// 局部计数，不带窗口维度会让一个窗口的取消请求误杀另一个窗口的任务。
/// `tauri::Window` 属注入参数，不会出现在 specta 生成的前端签名中。
#[derive(Clone, Default)]
pub struct StreamTaskRegistry(
    pub Arc<std::sync::Mutex<std::collections::HashMap<StreamTaskKey, StreamTaskSender>>>,
);

#[tauri::command]
#[specta::specta]
pub async fn cancel_stream_task(
    task_id: u32,
    window: tauri::Window,
    registry: tauri::State<'_, Arc<StreamTaskRegistry>>,
) -> Result<(), String> {
    if let Ok(mut map) = registry.0.lock() {
        if let Some(tx) = map.remove(&(window.label().to_string(), task_id)) {
            let _ = tx.send(());
        }
    }
    Ok(())
}

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

pub mod screenshot;
pub mod selection;

// 划词/浮窗相关符号平移至 `commands::selection`，此处重导出：
// `app_command_builder!` 与 `main.rs` 的既有引用路径保持不变，避免连带改动。
pub use selection::{
    place_float_near_cursor, trigger_selection_replacement, trigger_selection_translation,
    ReplacementFailedPayload, SelectionContext, DEFAULT_REPLACE_SHORTCUT, DEFAULT_SHORTCUT,
};

#[tauri::command]
#[specta::specta]
pub async fn get_shortcut(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    Ok(tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("custom_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_SHORTCUT.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_shortcut(
    shortcut: String,
    app: tauri::AppHandle,
    selection_ctx: tauri::State<'_, Arc<SelectionContext>>,
    pending_slot: tauri::State<'_, Arc<PendingFloatRequests>>,
    shortcut_slot: tauri::State<'_, Arc<ShortcutStatusSlot>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let clean = shortcut.trim();
    if clean.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    // 读取当前生效的旧快捷键（优先读取 SQLite，旧版回退 Keyring）
    let cache_read = cache.inner().clone();
    let old_shortcut = tokio::task::spawn_blocking(move || {
        cache_read
            .get_setting("custom_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_SHORTCUT.to_string());

    // 尝试注销旧快捷键以及即将注册的键（防止用户未改动快捷键重复保存时触发 AlreadyRegistered 错误）
    let _ = app.global_shortcut().unregister(old_shortcut.as_str());
    if old_shortcut != clean {
        let _ = app.global_shortcut().unregister(clean);
    }

    // 立即注册新快捷键
    let app_h = app.clone();
    let sel_flag = selection_ctx.sel_flag.clone();
    let counter = selection_ctx.counter.clone();
    let slot = pending_slot.inner().clone();
    let register_res = app
        .global_shortcut()
        .on_shortcut(clean, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                trigger_selection_translation(
                    app_h.clone(),
                    sel_flag.clone(),
                    counter.clone(),
                    slot.clone(),
                );
            }
        });

    if let Err(e) = register_res {
        // 注册失败时原子回滚：重新注册旧快捷键，防止快捷键状态永久丢失
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_counter = selection_ctx.counter.clone();
            let rollback_slot = pending_slot.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_translation(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_counter.clone(),
                            rollback_slot.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("快捷键注册失败，可能按键格式不合法或被系统占用: {e}");
        shortcut_slot.set_translate_failed(err_msg.clone());
        return Err(err_msg);
    }

    // 持久化存储新快捷键到 SQLite 设置表
    let to_save = clean.to_string();
    let cache_write = cache.inner().clone();
    let persist_res = tokio::task::spawn_blocking(move || {
        let _ = crate::secrets::delete_secret("custom_shortcut");
        cache_write.set_setting("custom_shortcut", &to_save)
    })
    .await
    .map_err(|e| format!("快捷键调度异常: {e}"))?;

    if let Err(e) = persist_res {
        // 回滚：注销刚注册的新快捷键，恢复旧快捷键
        let _ = app.global_shortcut().unregister(clean);
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_counter = selection_ctx.counter.clone();
            let rollback_slot = pending_slot.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_translation(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_counter.clone(),
                            rollback_slot.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("保存快捷键失败（已回滚到原快捷键）: {e}");
        shortcut_slot.set_translate_failed(err_msg.clone());
        return Err(err_msg);
    }

    shortcut_slot.set_translate_ok();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_replace_shortcut(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    Ok(tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("custom_replace_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_replace_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REPLACE_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_REPLACE_SHORTCUT.to_string()))
}

#[tauri::command]
#[specta::specta]
pub async fn set_replace_shortcut(
    shortcut: String,
    app: tauri::AppHandle,
    selection_ctx: tauri::State<'_, Arc<SelectionContext>>,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
    shortcut_slot: tauri::State<'_, Arc<ShortcutStatusSlot>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let clean = shortcut.trim();
    if clean.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    let cache_ref = deps.cache.clone();
    let old_shortcut = tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("custom_replace_shortcut")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("custom_replace_shortcut").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REPLACE_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_REPLACE_SHORTCUT.to_string());

    let _ = app.global_shortcut().unregister(old_shortcut.as_str());
    if old_shortcut != clean {
        let _ = app.global_shortcut().unregister(clean);
    }

    let app_h = app.clone();
    let sel_flag = selection_ctx.sel_flag.clone();
    let deps_ref = deps.inner().clone();

    let register_res = app
        .global_shortcut()
        .on_shortcut(clean, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                trigger_selection_replacement(app_h.clone(), sel_flag.clone(), deps_ref.clone());
            }
        });

    if let Err(e) = register_res {
        // 注册失败时原子回滚：重新注册旧快捷键，防止快捷键状态永久丢失
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_deps = deps.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_replacement(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_deps.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("划词替换快捷键注册失败，可能按键格式不合法或被系统占用: {e}");
        shortcut_slot.set_replace_failed(err_msg.clone());
        return Err(err_msg);
    }

    let to_save = clean.to_string();
    let cache_write = deps.cache.clone();
    let persist_res = tokio::task::spawn_blocking(move || {
        let _ = crate::secrets::delete_secret("custom_replace_shortcut");
        cache_write.set_setting("custom_replace_shortcut", &to_save)
    })
    .await
    .map_err(|e| format!("快捷键调度异常: {e}"))?;

    // 与 set_shortcut 同理：注册成功但持久化失败时必须对称回滚，
    // 否则进程行为与 keyring 配置不一致，重启后快捷键回退。
    if let Err(e) = persist_res {
        let _ = app.global_shortcut().unregister(clean);
        if old_shortcut != clean {
            let rollback_h = app.clone();
            let rollback_sel_flag = selection_ctx.sel_flag.clone();
            let rollback_deps = deps.inner().clone();
            let _ = app.global_shortcut().on_shortcut(
                old_shortcut.as_str(),
                move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        trigger_selection_replacement(
                            rollback_h.clone(),
                            rollback_sel_flag.clone(),
                            rollback_deps.clone(),
                        );
                    }
                },
            );
        }
        let err_msg = format!("保存快捷键失败（已回滚到原快捷键）: {e}");
        shortcut_slot.set_replace_failed(err_msg.clone());
        return Err(err_msg);
    }

    shortcut_slot.set_replace_ok();
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn detect_text_language(text: String) -> Result<Option<String>, String> {
    Ok(transly_core::detect_language(&text).map(|s| s.to_string()))
}

/// 浮窗翻译请求负载：携带来源区分与请求 ID，供前端去重与提示
#[derive(Clone, serde::Serialize)]
pub struct FloatRequest {
    pub text: String,
    /// "selection" = 全局快捷键划词；"clipboard" = 系统剪贴板监听
    pub source: &'static str,
    /// 宿主单调递增；同一 ID 只翻译一次
    pub request_id: u64,
}

/// 暂存待处理的浮窗请求队列。
///
/// 浮窗 `visible: false` 时 Tauri 仍创建 webview，但页面 JS 尚未加载完成；
/// 此时 `emit` 的事件无人接收且不会重放。因此宿主先暂存，浮窗就绪后主动拉取，
/// 从根上消除 emit 时序竞态。
///
/// 必须是**队列**而不是单槽：划词（快捷键）与剪贴板监听是两条独立链路，
/// 共用单槽时先到的请求会被后到的覆盖，用户表现为“第一次划词没反应”。
/// 容量设上限并在超出时丢弃最旧项，防止浮窗长期不可用时无界增长。
#[derive(Default)]
pub struct PendingFloatRequests(pub std::sync::Mutex<std::collections::VecDeque<FloatRequest>>);

/// 待处理队列容量上限。正常交互下浮窗会在毫秒级就绪，超出此值说明浮窗已不可用。
const PENDING_FLOAT_CAPACITY: usize = 16;

impl PendingFloatRequests {
    /// 入队一条请求；超出容量时丢弃最旧项并返回 true 表示发生了丢弃。
    pub fn push(&self, req: FloatRequest) -> bool {
        let Ok(mut queue) = self.0.lock() else {
            return false;
        };
        let mut dropped = false;
        while queue.len() >= PENDING_FLOAT_CAPACITY {
            queue.pop_front();
            dropped = true;
        }
        queue.push_back(req);
        dropped
    }
}

/// 浮窗粘滞标志。必须用独立新类型：Tauri 按 TypeId 存 state，
/// 若与 `clipboard_monitor_enabled` 同为 `Arc<AtomicBool>` 会 panic（重复 manage）
#[derive(Debug, Default)]
pub struct FloatSticky(pub std::sync::atomic::AtomicBool);

/// 设置浮窗粘滞：翻译中/错误态时为 true，浮窗不失焦自动隐藏，
/// 保证错误提示与重试按钮来得及被用户看到并点击
#[tauri::command]
#[specta::specta]
pub fn set_float_sticky(
    enabled: bool,
    sticky: tauri::State<'_, Arc<FloatSticky>>,
) -> Result<(), String> {
    sticky.0.store(enabled, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// 浮窗拉取待处理请求。`has_request: false` 表示无待处理请求。
/// 用扁平字段而非 `Option<FloatRequest>`：specta 不支持嵌套 Option 作为 IPC 返回类型。
#[tauri::command]
#[specta::specta]
pub fn take_pending_float_request(
    pending: tauri::State<'_, Arc<PendingFloatRequests>>,
) -> Result<PendingFloatResponse, String> {
    let mut queue = pending
        .0
        .lock()
        .map_err(|e| format!("读取待处理浮窗请求失败: {e}"))?;
    Ok(match queue.pop_front() {
        Some(req) => PendingFloatResponse {
            has_request: true,
            text: req.text,
            source: req.source.to_string(),
            request_id: req.request_id as u32,
        },
        None => PendingFloatResponse {
            has_request: false,
            text: String::new(),
            source: String::new(),
            request_id: 0,
        },
    })
}

#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct PendingFloatResponse {
    pub has_request: bool,
    pub text: String,
    pub source: String,
    /// u32 而非 u64：specta 禁止导出 BigInt 类型以避免 JS 精度丢失
    pub request_id: u32,
}

#[tauri::command]
#[specta::specta]
pub async fn get_server_token() -> Result<String, String> {
    // keyring 读取（未命中时还会生成并写入）为同步阻塞 I/O，禁止占用主线程
    tokio::task::spawn_blocking(crate::secrets::get_or_create_server_token)
        .await
        .map_err(|e| format!("Token 读取任务调度异常: {e}"))?
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn rotate_server_token(
    current_token: tauri::State<'_, Arc<RwLock<String>>>,
) -> Result<String, String> {
    // keyring 写入为同步阻塞 I/O，禁止占用 async 运行时工作线程
    let new_token = tokio::task::spawn_blocking(crate::secrets::rotate_server_token)
        .await
        .map_err(|e| format!("Token 轮换任务调度异常: {e}"))?
        .map_err(|e| e.to_string())?;
    let mut lock = current_token.write().await;
    *lock = new_token.clone();
    Ok(new_token)
}

#[tauri::command]
#[specta::specta]
pub async fn approve_pairing(
    pairing_mgr: tauri::State<'_, Arc<crate::server::pairing::PairingManager>>,
    session_id: String,
) -> Result<(), String> {
    if pairing_mgr.approve(&session_id).await {
        Ok(())
    } else {
        Err("配对会话不存在或已过期".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn reject_pairing(
    pairing_mgr: tauri::State<'_, Arc<crate::server::pairing::PairingManager>>,
    session_id: String,
) -> Result<(), String> {
    if pairing_mgr.reject(&session_id).await {
        Ok(())
    } else {
        Err("配对会话不存在或已过期".to_string())
    }
}

/// 本地 HTTP 服务（供浏览器插件调用）的运行状态。
///
/// 服务在后台异步启动，失败只进 stderr 的话用户完全无感：
/// 扩展端会显示“桌面端未运行”，而桌面端毫无提示。此处把状态暴露给前端。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ServerStatus {
    /// 尚未启动完成（setup 中的异步任务还在进行）
    Starting,
    /// 已就绪
    Running,
    /// 未能启动，附原因（如端口被占用、系统凭据不可用）
    Failed { reason: String },
}

/// 本地服务状态槽位。独立新类型：Tauri 按 TypeId 存 state。
#[derive(Debug, Default)]
pub struct ServerStatusSlot(pub std::sync::Mutex<Option<ServerStatus>>);

impl ServerStatusSlot {
    pub fn set(&self, status: ServerStatus) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(status);
        }
    }

    pub fn get(&self) -> ServerStatus {
        self.0
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .unwrap_or(ServerStatus::Starting)
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_server_status(slot: tauri::State<'_, Arc<ServerStatusSlot>>) -> ServerStatus {
    slot.get()
}

/// 全局快捷键的注册状态。
///
/// `Alt+Shift+D` / `Alt+Shift+R` 是极易被其他软件占用的组合，注册失败（被占用 /
/// 格式非法 / 权限不足）属于高频场景。若把结果直接丢弃，用户只会看到「划词功能
/// 莫名其妙不工作」且没有任何线索。因此必须把结果暴露出来。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ShortcutStatusDto {
    /// 两个快捷键是否都注册成功
    pub ok: bool,
    /// 聚合失败原因（兼容旧字段，注册成功时为 None）
    pub reason: Option<String>,
    /// 划词翻译快捷键注册失败原因（注册成功时为 None）
    pub translate_error: Option<String>,
    /// 划词原地替换快捷键注册失败原因（注册成功时为 None）
    pub replace_error: Option<String>,
}

/// 快捷键注册状态槽位。与 `ServerStatusSlot` 同构（Tauri 按 TypeId 存 state）。
/// 分离维护划词翻译与划词原地替换的错误状态，避免二者状态相互覆盖。
#[derive(Debug, Default)]
pub struct ShortcutStatusSlot {
    translate_error: std::sync::Mutex<Option<String>>,
    replace_error: std::sync::Mutex<Option<String>>,
}

impl ShortcutStatusSlot {
    pub fn set_translate_ok(&self) {
        if let Ok(mut slot) = self.translate_error.lock() {
            *slot = None;
        }
    }

    pub fn set_translate_failed(&self, reason: String) {
        if let Ok(mut slot) = self.translate_error.lock() {
            *slot = Some(reason);
        }
    }

    pub fn set_replace_ok(&self) {
        if let Ok(mut slot) = self.replace_error.lock() {
            *slot = None;
        }
    }

    pub fn set_replace_failed(&self, reason: String) {
        if let Ok(mut slot) = self.replace_error.lock() {
            *slot = Some(reason);
        }
    }

    pub fn get(&self) -> ShortcutStatusDto {
        let t_err = self.translate_error.lock().ok().and_then(|g| g.clone());
        let r_err = self.replace_error.lock().ok().and_then(|g| g.clone());

        let reason = match (&t_err, &r_err) {
            (None, None) => None,
            (Some(t), None) => Some(t.clone()),
            (None, Some(r)) => Some(r.clone()),
            (Some(t), Some(r)) => Some(format!("{t}; {r}")),
        };

        ShortcutStatusDto {
            ok: reason.is_none(),
            reason,
            translate_error: t_err,
            replace_error: r_err,
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_shortcut_status(slot: tauri::State<'_, Arc<ShortcutStatusSlot>>) -> ShortcutStatusDto {
    slot.get()
}

fn validate_engine_name(engine: &str) -> Result<(), String> {
    match engine.to_lowercase().as_str() {
        "deepl" | "openai" | "deepseek" => Ok(()),
        other => Err(format!("不支持或未知的引擎凭据配置: {other}")),
    }
}

#[derive(serde::Serialize, serde::Deserialize, specta::Type, Default)]
pub struct LlmConfigDto {
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn save_llm_config(
    engine: String,
    base_url: Option<String>,
    model: Option<String>,
    pool: tauri::State<'_, Arc<EnginePool>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let eng = engine.to_lowercase();
    if eng != "openai" && eng != "deepseek" {
        return Err("仅支持配置 OpenAI 或 DeepSeek 的端点与模型".to_string());
    }

    let url_key = format!("{eng}_base_url");
    let model_key = format!("{eng}_model");

    let url_val = base_url.unwrap_or_default().trim().to_string();
    let model_val = model.unwrap_or_default().trim().to_string();

    // 端点校验：密钥会被明文发往该地址，因此必须限定协议。
    // 允许任意 `http://` 意味着 API Key 可能以明文过网；
    // 唯一的例外是本地回环（Ollama / vLLM 等本机部署，不出网卡）。
    if !url_val.is_empty() {
        let is_loopback = transly_core::OpenAIEngine::is_local_endpoint(&url_val);
        if !url_val.starts_with("https://") && !is_loopback {
            return Err(
                "端点必须使用 https://（本地 Ollama 等本机服务可用 http://127.0.0.1 或 http://localhost）"
                    .to_string(),
            );
        }
    }

    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if url_val.is_empty() {
            let _ = cache_ref.delete_setting(&url_key);
            let _ = crate::secrets::delete_secret(&url_key);
        } else {
            cache_ref.set_setting(&url_key, &url_val)?;
            let _ = crate::secrets::delete_secret(&url_key);
        }

        if model_val.is_empty() {
            let _ = cache_ref.delete_setting(&model_key);
            let _ = crate::secrets::delete_secret(&model_key);
        } else {
            cache_ref.set_setting(&model_key, &model_val)?;
            let _ = crate::secrets::delete_secret(&model_key);
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("配置保存调度异常: {e}"))?
    .map_err(|e| e.to_string())?;

    pool.invalidate(&engine).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_llm_config(
    engine: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<LlmConfigDto, String> {
    let eng = engine.to_lowercase();
    if eng != "openai" && eng != "deepseek" {
        return Err("仅支持读取 OpenAI 或 DeepSeek 的端点与模型".to_string());
    }

    let url_key = format!("{eng}_base_url");
    let model_key = format!("{eng}_model");
    let cache_ref = cache.inner().clone();

    tokio::task::spawn_blocking(move || {
        let base_url = cache_ref
            .get_setting(&url_key)
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret(&url_key).ok())
            .filter(|s| !s.trim().is_empty());
        let model = cache_ref
            .get_setting(&model_key)
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret(&model_key).ok())
            .filter(|s| !s.trim().is_empty());
        Ok(LlmConfigDto { base_url, model })
    })
    .await
    .unwrap_or_else(|e| Err(format!("配置读取调度异常: {e}")))
}

#[tauri::command]
#[specta::specta]
pub async fn save_llm_style(
    style: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let s = style.trim().to_lowercase();
    match s.as_str() {
        "general" | "academic" | "literary" | "business" => {}
        _ => return Err("无效的 LLM 翻译风格".to_string()),
    }

    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_ref.set_setting("llm_style", &s)?;
        let _ = crate::secrets::delete_secret("llm_style");
        Ok(())
    })
    .await
    .map_err(|e| format!("保存 LLM 风格任务调度异常: {e}"))?
    .map_err(|e| format!("写入 LLM 风格失败: {e}"))?;

    pool.invalidate("openai").await;
    pool.invalidate("deepseek").await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_preferred_replace_engine(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("preferred_replace_engine")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("preferred_replace_engine").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "google".to_string())
    })
    .await
    .map_err(|e| format!("读取划词原地替换引擎配置异常: {e}"))
}

pub fn validate_preferred_replace_engine(engine: &str) -> Result<String, String> {
    let clean = engine.trim().to_lowercase();
    match clean.as_str() {
        "google" | "google_web" => Ok("google".to_string()),
        "deepl" | "deepseek" | "openai" | "offline" => Ok(clean),
        _ => Err("无效或不受支持的划词替换引擎".to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn save_preferred_replace_engine(
    engine: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let to_save = validate_preferred_replace_engine(&engine)?;
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_ref.set_setting("preferred_replace_engine", &to_save)?;
        let _ = crate::secrets::delete_secret("preferred_replace_engine");
        Ok(())
    })
    .await
    .map_err(|e| format!("保存划词原地替换引擎调度异常: {e}"))?
    .map_err(|e| format!("写入系统配置失败: {e}"))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_llm_style(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("llm_style")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("llm_style").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "general".to_string())
    })
    .await
    .map_err(|e| format!("读取 LLM 风格任务调度异常: {e}"))
}

#[tauri::command]
#[specta::specta]
pub async fn save_api_key(
    engine: String,
    key: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
) -> Result<(), String> {
    validate_engine_name(&engine)?;
    if key.trim().is_empty() {
        return Err("API 密钥不能为空".to_string());
    }
    if key.len() > 512 {
        return Err("API 密钥长度超出限制".to_string());
    }
    // keyring 写入为同步阻塞 I/O，禁止占用主线程
    let key_owned = key.trim().to_string();
    let engine_owned = engine.clone();
    tokio::task::spawn_blocking(move || crate::secrets::set_secret(&engine_owned, &key_owned))
        .await
        .map_err(|e| format!("密钥写入任务调度异常: {e}"))?
        .map_err(|e| e.to_string())?;
    pool.invalidate(&engine).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn is_api_key_configured(engine: String, app: tauri::AppHandle) -> bool {
    use tauri::Manager;
    if validate_engine_name(&engine).is_err() {
        return false;
    }
    let cache_opt = app
        .try_state::<Arc<TranslationCache>>()
        .map(|c| c.inner().clone());
    tokio::task::spawn_blocking(move || {
        if engine == "openai" || engine == "deepseek" {
            let setting_key = format!("{engine}_base_url");
            let url_opt = cache_opt
                .as_ref()
                .and_then(|c| c.get_setting(&setting_key).ok().flatten())
                .or_else(|| crate::secrets::get_secret(&setting_key).ok());
            if let Some(url) = url_opt {
                if transly_core::OpenAIEngine::is_local_endpoint(&url) {
                    return true;
                }
            }
        }
        crate::secrets::get_secret(&engine)
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_api_key(
    engine: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
) -> Result<(), String> {
    validate_engine_name(&engine)?;
    // keyring 删除为同步阻塞 I/O，禁止占用主线程
    let engine_owned = engine.clone();
    tokio::task::spawn_blocking(move || crate::secrets::delete_secret(&engine_owned))
        .await
        .map_err(|e| format!("密钥删除任务调度异常: {e}"))?
        .map_err(|e| e.to_string())?;
    pool.invalidate(&engine).await;
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct EngineCapabilitiesDto {
    pub id: String,
    pub name: String,
    pub max_batch_size: u32,
    pub supports_batch: bool,
    pub requires_api_key: bool,
    pub supports_streaming: bool,
}

#[tauri::command]
#[specta::specta]
pub async fn get_engine_capabilities(
    engine: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
) -> Result<EngineCapabilitiesDto, String> {
    if !transly_core::validate_engine_id(&engine) {
        return Err(format!("不支持或未知的翻译引擎: {engine}"));
    }
    let eng = pool.get_engine(&engine).await;
    let caps = eng.capabilities();
    Ok(EngineCapabilitiesDto {
        id: caps.id,
        name: caps.name,
        max_batch_size: caps.max_batch_size as u32,
        supports_batch: caps.supports_batch,
        requires_api_key: caps.requires_api_key,
        supports_streaming: caps.supports_streaming,
    })
}

pub fn validate_proxy_config(config: &ProxyConfig) -> Result<(), String> {
    if config.mode == ProxyMode::Manual {
        match config.url {
            Some(ref u) if !u.trim().is_empty() => {
                if transly_core::normalize_proxy_url(u).is_none() {
                    return Err("无效的手动代理地址格式".to_string());
                }
            }
            _ => return Err("手动代理模式下必须填写代理地址".to_string()),
        }
    }
    Ok(())
}

pub fn get_proxy_config_internal(cache: &TranslationCache) -> Result<ProxyConfig, String> {
    let mode_str = cache
        .get_setting("proxy_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| "auto".to_string());
    let mode = ProxyMode::parse_str(&mode_str);
    let url = cache
        .get_setting("proxy_url")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty());
    Ok(ProxyConfig { mode, url })
}

pub async fn save_proxy_config_internal(
    config: ProxyConfig,
    pool: &EnginePool,
    cache: Arc<TranslationCache>,
) -> Result<(), String> {
    validate_proxy_config(&config)?;

    let mode_str = config.mode.as_str().to_string();
    let url_opt = config.url.clone();

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        cache
            .set_setting("proxy_mode", &mode_str)
            .map_err(|e| format!("写入系统配置失败: {e}"))?;

        if let Some(ref u) = url_opt {
            let trimmed = u.trim();
            if !trimmed.is_empty() {
                cache
                    .set_setting("proxy_url", trimmed)
                    .map_err(|e| format!("写入系统配置失败: {e}"))?;
            } else {
                cache
                    .delete_setting("proxy_url")
                    .map_err(|e| format!("清理系统配置失败: {e}"))?;
            }
        } else {
            cache
                .delete_setting("proxy_url")
                .map_err(|e| format!("清理系统配置失败: {e}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("保存网络代理配置调度异常: {e}"))??;

    pool.reload_proxy().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_proxy_config(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<ProxyConfig, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || get_proxy_config_internal(&cache_ref))
        .await
        .map_err(|e| format!("读取网络代理配置调度异常: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub async fn save_proxy_config(
    config: ProxyConfig,
    pool: tauri::State<'_, Arc<EnginePool>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let pool_ref = pool.inner().clone();
    let cache_ref = cache.inner().clone();
    save_proxy_config_internal(config, &pool_ref, cache_ref).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use transly_core::{Error, ErrorKind};

    fn req(text: &str, source: &'static str, id: u64) -> FloatRequest {
        FloatRequest {
            text: text.to_string(),
            source,
            request_id: id,
        }
    }

    #[test]
    fn test_pending_float_requests_are_fifo_not_overwritten() {
        // 回归：单槽实现下，划词与剪贴板两条链路的请求会互相覆盖，
        // 先到的请求静默丢失，用户表现为“第一次划词没反应”。
        let pending = PendingFloatRequests::default();
        assert!(!pending.push(req("first", "selection", 1)));
        assert!(!pending.push(req("second", "clipboard", 2)));

        let queue = pending.0.lock().unwrap();
        assert_eq!(queue.len(), 2, "两条请求都必须保留");
        assert_eq!(queue[0].text, "first", "必须保持先进先出顺序");
        assert_eq!(queue[1].text, "second");
    }

    #[test]
    fn test_pending_float_requests_bounded_capacity_drops_oldest() {
        let pending = PendingFloatRequests::default();
        for i in 0..PENDING_FLOAT_CAPACITY {
            assert!(!pending.push(req(&format!("t{i}"), "clipboard", i as u64)));
        }
        // 第 CAPACITY+1 条：应丢弃最旧项并上报丢弃
        assert!(pending.push(req("overflow", "clipboard", 999)));

        let queue = pending.0.lock().unwrap();
        assert_eq!(queue.len(), PENDING_FLOAT_CAPACITY, "容量必须有界");
        assert_eq!(queue.front().unwrap().text, "t1", "丢弃的是最旧项");
        assert_eq!(queue.back().unwrap().text, "overflow");
    }

    #[test]
    fn test_translate_error_dto_serializes_structured_kind() {
        let dto = TranslateErrorDto::from_error(&Error::Engine {
            engine: "deepl".into(),
            message: "rate limit".into(),
        });
        let json = serde_json::to_string(&dto).unwrap();
        // 前端按此 key 分类，禁止退化为裸字符串
        assert!(json.contains(r#""kind":"engine""#));
        assert!(json.contains(r#""retryable":true"#));
        // 不得泄露密钥或 Authorization 头
        assert!(!json.to_lowercase().contains("authorization"));
        assert!(!json.to_lowercase().contains("api_key\":\""));
    }

    #[test]
    fn test_translate_error_dto_auth_not_retryable() {
        let dto = TranslateErrorDto::from_error(&Error::MissingCredentials("deepl key".into()));
        assert_eq!(dto.kind, ErrorKind::Auth);
        assert!(!dto.retryable, "缺密钥时重试无意义，不得提供重试入口");
    }

    #[test]
    fn test_translate_error_dto_input_not_retryable() {
        let dto = TranslateErrorDto::from_error(&Error::Input("翻译文本过长".into()));
        assert_eq!(dto.kind, ErrorKind::Input);
        assert!(!dto.retryable);
    }

    #[tokio::test]
    async fn test_pairing_commands_approve_and_reject() {
        let manager = Arc::new(crate::server::pairing::PairingManager::new());
        let s1 = match manager.create_session("client-1".into(), None).await {
            crate::server::pairing::CreateSessionOutcome::Created(id, _) => id,
            o => panic!("{o:?}"),
        };
        let s2 = match manager.create_session("client-2".into(), None).await {
            crate::server::pairing::CreateSessionOutcome::Created(id, _) => id,
            o => panic!("{o:?}"),
        };

        // approve 测试
        assert!(manager.approve(&s1).await);
        // 重复 approve 应失败
        assert!(!manager.approve(&s1).await);

        // reject 测试
        assert!(manager.reject(&s2).await);
        // 重复 reject 应失败
        assert!(!manager.reject(&s2).await);
    }

    #[tokio::test]
    async fn test_detect_text_language_command() {
        let res = detect_text_language("你好世界".to_string()).await;
        assert_eq!(res, Ok(Some("zh-CN".to_string())));

        let res_en = detect_text_language("Hello world".to_string()).await;
        assert_eq!(res_en, Ok(Some("en".to_string())));

        let res_none = detect_text_language("".to_string()).await;
        assert_eq!(res_none, Ok(None));
    }

    #[test]
    fn test_shortcut_status_slot_transitions() {
        let slot = ShortcutStatusSlot::default();
        let initial = slot.get();
        assert!(initial.ok);
        assert_eq!(initial.translate_error, None);
        assert_eq!(initial.replace_error, None);

        slot.set_translate_failed("Ctrl+Shift+T: conflict".to_string());
        let failed = slot.get();
        assert!(!failed.ok);
        assert_eq!(
            failed.translate_error.as_deref(),
            Some("Ctrl+Shift+T: conflict")
        );
        assert_eq!(failed.replace_error, None);

        // 原地替换注册成功不应冲掉划词快捷键的失败
        slot.set_replace_ok();
        let still_failed = slot.get();
        assert!(!still_failed.ok);
        assert_eq!(
            still_failed.translate_error.as_deref(),
            Some("Ctrl+Shift+T: conflict")
        );

        // 原地替换也失败时应独立保留两者的原因并聚合 reason
        slot.set_replace_failed("Ctrl+Shift+X: conflict".to_string());
        let both_failed = slot.get();
        assert!(!both_failed.ok);
        assert_eq!(
            both_failed.translate_error.as_deref(),
            Some("Ctrl+Shift+T: conflict")
        );
        assert_eq!(
            both_failed.replace_error.as_deref(),
            Some("Ctrl+Shift+X: conflict")
        );
        assert_eq!(
            both_failed.reason.as_deref(),
            Some("Ctrl+Shift+T: conflict; Ctrl+Shift+X: conflict")
        );

        // 划词修复后只留原地替换失败
        slot.set_translate_ok();
        let replace_only = slot.get();
        assert!(!replace_only.ok);
        assert_eq!(replace_only.translate_error, None);
        assert_eq!(
            replace_only.replace_error.as_deref(),
            Some("Ctrl+Shift+X: conflict")
        );

        slot.set_replace_ok();
        let ok = slot.get();
        assert!(ok.ok);
        assert_eq!(ok.reason, None);
        assert_eq!(ok.translate_error, None);
        assert_eq!(ok.replace_error, None);
    }

    #[test]
    fn test_save_preferred_replace_engine_validation() {
        assert!(validate_preferred_replace_engine("invalid_eng").is_err());
        assert!(validate_preferred_replace_engine("").is_err());
        assert_eq!(
            validate_preferred_replace_engine("google"),
            Ok("google".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("google_web"),
            Ok("google".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("deepl"),
            Ok("deepl".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("deepseek"),
            Ok("deepseek".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("openai"),
            Ok("openai".to_string())
        );
        assert_eq!(
            validate_preferred_replace_engine("offline"),
            Ok("offline".to_string())
        );
    }

    #[tokio::test]
    async fn test_proxy_config_save_and_get() {
        let dir =
            std::env::temp_dir().join(format!("transly-proxy-test-{}", rand::random::<u64>()));
        let cache = TranslationCache::open_default(&dir);
        let pool = Arc::new(EnginePool::with_cache(cache.clone()));

        // 默认应为 auto
        let initial = cache.get_setting("proxy_mode").unwrap();
        assert_eq!(initial, None);

        // 验证手动代理没有 URL 时被拒绝
        let invalid_manual = ProxyConfig {
            mode: ProxyMode::Manual,
            url: None,
        };
        assert!(validate_proxy_config(&invalid_manual).is_err());
        assert!(
            save_proxy_config_internal(invalid_manual, &pool, cache.clone())
                .await
                .is_err()
        );

        // 保存合法手动代理
        let valid_manual = ProxyConfig {
            mode: ProxyMode::Manual,
            url: Some("127.0.0.1:7890".to_string()),
        };
        assert!(validate_proxy_config(&valid_manual).is_ok());
        assert!(
            save_proxy_config_internal(valid_manual, &pool, cache.clone())
                .await
                .is_ok()
        );

        assert_eq!(
            cache.get_setting("proxy_mode").unwrap().as_deref(),
            Some("manual")
        );
        assert_eq!(
            cache.get_setting("proxy_url").unwrap().as_deref(),
            Some("127.0.0.1:7890")
        );

        let retrieved = get_proxy_config_internal(&cache).unwrap();
        assert_eq!(retrieved.mode, ProxyMode::Manual);
        assert_eq!(retrieved.url.as_deref(), Some("127.0.0.1:7890"));

        // 切换为 none
        let none_cfg = ProxyConfig {
            mode: ProxyMode::None,
            url: None,
        };
        assert!(validate_proxy_config(&none_cfg).is_ok());
        assert!(save_proxy_config_internal(none_cfg, &pool, cache.clone())
            .await
            .is_ok());

        assert_eq!(
            cache.get_setting("proxy_mode").unwrap().as_deref(),
            Some("none")
        );
        assert_eq!(cache.get_setting("proxy_url").unwrap(), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_is_local_endpoint_support_for_llm_engines() {
        assert!(transly_core::OpenAIEngine::is_local_endpoint(
            "http://localhost:11434/v1"
        ));
        assert!(transly_core::OpenAIEngine::is_local_endpoint(
            "http://127.0.0.1:11434/v1"
        ));
        assert!(transly_core::OpenAIEngine::is_local_endpoint(
            "http://[::1]:11434/v1"
        ));
        assert!(!transly_core::OpenAIEngine::is_local_endpoint(
            "https://api.deepseek.com"
        ));
    }
}

#[cfg(test)]
mod cancel_semantics_tests {
    /// 回归：取消信号的三种语义必须严格区分。
    ///
    /// 复现的缺陷：`cancel_rx` 返回 `Err(RecvError)` 的时机是「同键 sender 被
    /// 覆盖/丢弃」，属注册表内部事件；旧实现却在该分支直接返回
    /// 「流式任务的取消通道已失效」，把一次本可正常完成的翻译变成伪错误，
    /// 与其相邻注释声明的语义自相矛盾。
    ///
    /// 现在该情形让取消分支永久挂起，select 完全由引擎结果决定。
    /// 本测试覆盖三种组合，防止后人「顺手简化」时改坏这段语义。
    async fn run_cancel_select(drop_sender: bool, engine_delay_ms: u64) -> &'static str {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        if drop_sender {
            drop(tx);
        } else {
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                let _ = tx.send(());
            });
        }
        let engine = async move {
            tokio::time::sleep(std::time::Duration::from_millis(engine_delay_ms)).await;
            "engine_result"
        };
        let cancel_signal = async move {
            match rx.await {
                Ok(()) => {}
                Err(_) => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            r = engine => r,
            _ = cancel_signal => "cancelled",
        }
    }

    #[tokio::test]
    async fn test_cancel_signal_distinguishes_user_cancel_from_channel_drop() {
        assert_eq!(
            run_cancel_select(true, 50).await,
            "engine_result",
            "sender 析构（同键任务覆盖）不得被当成用户取消，必须等引擎返回真实结果"
        );
        assert_eq!(
            run_cancel_select(false, 500).await,
            "cancelled",
            "用户主动取消必须立即生效，不能等引擎跑完"
        );
        assert_eq!(
            run_cancel_select(false, 5).await,
            "engine_result",
            "引擎先返回时应正常完成"
        );
    }
}
