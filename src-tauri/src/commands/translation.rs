//! 翻译主链路命令：同步/流式翻译、流式取消与引擎能力探测相关 DTO。

use super::*;

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
