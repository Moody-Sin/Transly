use crate::engine_pool::EnginePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::task::classify_error;
use transly_core::{CacheEntry, Error, ErrorKind, Glossary, TranslationCache, TranslationRequest};

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

    // 1. 若开启缓存，优先查缓存（先查当前引擎，若具备 fallback 且未命中，允许命中降级引擎历史缓存）
    if cache_enabled {
        let cached_res = {
            let cache_ref = cache.inner().clone();
            let engine_clone = active_engine.clone();
            let t = text.clone();
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
            let applied = g.apply(&cached_text);
            return Ok(TranslateOutcomeDto {
                text: applied,
                engine: used_engine,
            });
        }
    }

    // 2. 缓存未命中或未开启，调用引擎翻译
    let req = TranslationRequest {
        text: text.clone(),
        source_lang: sl.clone(),
        target_lang: target_lang.clone(),
    };

    match active_engine.translate(&req).await {
        Ok(res) => {
            // 3. 异步写入缓存（在同步门禁锁内同时验证 generation 版本与开启状态，消除判断与写入之间的竞态）
            let cache_target_engine = res.actual_engine();
            if let Err(e) = cache_controller.set_if_current(start_gen, || {
                cache.set(&text, &res.text, &sl, &target_lang, cache_target_engine)
            }) {
                eprintln!("[Cache Write Warning] 桌面端翻译缓存写入失败: {e}");
            }
            let g = glossary.read().await;
            Ok(TranslateOutcomeDto {
                text: g.apply(&res.text),
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
    cache: tauri::State<'_, Arc<TranslationCache>>,
    pool: tauri::State<'_, Arc<EnginePool>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
    cache_controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
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
        pool.get_engine(name).await
    } else {
        pool.default_engine()
    };

    let (cache_enabled, start_gen) = cache_controller.snapshot();

    // 1. 若开启缓存，优先查缓存
    if cache_enabled {
        let cached_res = {
            let cache_ref = cache.inner().clone();
            let engine_clone = active_engine.clone();
            let t = text.clone();
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
            let applied = g.apply(&cached_text);
            // 命中缓存时直接分发完整 chunk，让打字机视图立即就绪
            let _ = window.emit(
                "translate-stream-chunk",
                StreamChunkPayload {
                    task_id,
                    chunk: applied.clone(),
                },
            );
            return Ok(TranslateOutcomeDto {
                text: applied,
                engine: used_engine,
            });
        }
    }

    // 2. 缓存未命中或未开启，调用流式引擎翻译
    let req = TranslationRequest {
        text: text.clone(),
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

    match active_engine.translate_stream(&req, on_chunk).await {
        Ok(res) => {
            let cache_target_engine = res.actual_engine();
            if let Err(e) = cache_controller.set_if_current(start_gen, || {
                cache.set(&text, &res.text, &sl, &target_lang, cache_target_engine)
            }) {
                eprintln!("[Cache Write Warning] 桌面端流式翻译缓存写入失败: {e}");
            }
            let g = glossary.read().await;
            Ok(TranslateOutcomeDto {
                text: g.apply(&res.text),
                engine: res.engine,
            })
        }
        Err(e) => Err(TranslateErrorDto::from_error(&e)),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn add_glossary_term(
    source: String,
    target: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<(), String> {
    // SQLite 写入为同步阻塞 I/O，禁止占用 async 运行时工作线程
    let cache_ref = cache.inner().clone();
    let (write_source, write_target) = (source.clone(), target.clone());
    tokio::task::spawn_blocking(move || cache_ref.set_glossary_term(&write_source, &write_target))
        .await
        .map_err(|e| format!("术语写入任务调度异常: {e}"))?
        .map_err(|e| e.to_string())?;
    let mut g = glossary.write().await;
    g.insert(source, target);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_glossary_terms(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<Vec<(String, String)>, String> {
    // SQLite 同步 I/O 必须移出主线程：全表查询在缓存膨胀时可达数十毫秒
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_all_glossary_terms()
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
    // SQLite 删除与全表回读均为同步阻塞 I/O，统一下沉到阻塞线程池
    let cache_ref = cache.inner().clone();
    let deleted_key = source.clone();
    let terms = tokio::task::spawn_blocking(move || -> Result<Vec<(String, String)>, String> {
        cache_ref
            .delete_glossary_term(&deleted_key)
            .map_err(|e| e.to_string())?;
        cache_ref
            .get_all_glossary_terms()
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("术语删除任务调度异常: {e}"))??;

    // 重新构建内存术语库
    let mut g = glossary.write().await;
    *g = Glossary::new();
    for (src, tgt) in terms {
        g.insert(src, tgt);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn clear_cache(cache: tauri::State<'_, Arc<TranslationCache>>) -> Result<u32, String> {
    // DELETE 全表在 10 万条缓存下是长事务，必须移出主线程避免 UI 硬卡
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .clear_all_cache()
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

#[tauri::command]
#[specta::specta]
pub async fn set_cache_enabled(
    enabled: bool,
    controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    // keyring 写入为同步阻塞 I/O，禁止占用主线程
    tokio::task::spawn_blocking(move || {
        crate::secrets::set_secret("cache_enabled", if enabled { "true" } else { "false" })
    })
    .await
    .map_err(|e| format!("配置写入任务调度异常: {e}"))?
    .map_err(|e| format!("写入系统配置失败: {e}"))?;

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
) -> Result<(), String> {
    // keyring 写入为同步阻塞 I/O，禁止占用主线程
    tokio::task::spawn_blocking(move || {
        crate::secrets::set_secret(
            "clipboard_monitor_enabled",
            if enabled { "true" } else { "false" },
        )
    })
    .await
    .map_err(|e| format!("凭据写入任务调度异常: {e}"))?
    .map_err(|e| format!("写入系统安全凭据失败: {e}"))?;
    flag.store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_clipboard_monitor() -> bool {
    // keyring 读取为同步阻塞 I/O，禁止占用主线程
    tokio::task::spawn_blocking(|| crate::secrets::get_secret("clipboard_monitor_enabled"))
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

fn validate_engine_name(engine: &str) -> Result<(), String> {
    match engine.to_lowercase().as_str() {
        "deepl" | "openai" | "deepseek" => Ok(()),
        other => Err(format!("不支持或未知的引擎凭据配置: {other}")),
    }
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
pub async fn is_api_key_configured(engine: String) -> bool {
    if validate_engine_name(&engine).is_err() {
        return false;
    }
    // keyring 读取为同步阻塞 I/O，禁止占用主线程
    tokio::task::spawn_blocking(move || crate::secrets::get_secret(&engine))
        .await
        .map(|res| res.map(|k| !k.trim().is_empty()).unwrap_or(false))
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
}
