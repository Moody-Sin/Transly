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
    // SQLite 删除与全表回读均为同步阻塞 I/O，统一下沉到阻塞线程池
    let cache_ref = cache.inner().clone();
    let deleted_key = source.clone();
    let terms =
        tokio::task::spawn_blocking(move || -> Result<Vec<(String, String, bool)>, String> {
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
    for (src, tgt, match_case) in terms {
        g.insert_with_options(src, tgt, match_case);
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

pub const DEFAULT_SHORTCUT: &str = "Alt+Shift+D";
pub const DEFAULT_REPLACE_SHORTCUT: &str = "Alt+Shift+R";

/// 全局剪贴板自动化互斥锁，确保划词悬浮呼出与原地替换模拟流程不发生并发抢占
static CLIPBOARD_SIMULATION_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub struct SelectionContext {
    pub sel_flag: Arc<std::sync::atomic::AtomicBool>,
    pub counter: Arc<std::sync::atomic::AtomicU64>,
}

/// 触发划词原地翻译并替换工作流：
/// 1. 模拟复制 (Ctrl+C / Cmd+C) 获取当前选区文本
/// 2. 使用离线检测推断智能目标语言并执行翻译
/// 3. 将译文写入剪贴板并模拟粘贴 (Ctrl+V / Cmd+V) 覆盖原选区
/// 4. 延迟后恢复用户原有剪贴板，全程置 `sel_flag` 抑制剪贴板监听器自触发
pub fn trigger_selection_replacement(
    handle: tauri::AppHandle,
    sel_flag: Arc<std::sync::atomic::AtomicBool>,
    pool: Arc<EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
    use std::sync::atomic::Ordering;
    use tauri_plugin_clipboard_manager::ClipboardExt;

    tauri::async_runtime::spawn(async move {
        // 获取互斥锁排队，避免划词悬浮与原地替换同时模拟读写剪贴板发生数据竞态
        let _guard = CLIPBOARD_SIMULATION_MUTEX.lock().await;

        // 0. 备份原剪贴板
        let old_clipboard = handle.clipboard().read_text().ok();

        // 抑制剪贴板监听
        sel_flag.store(true, Ordering::SeqCst);

        // 1. 模拟 Ctrl+C
        if let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) {
            #[cfg(target_os = "macos")]
            let modifier = Key::Meta;
            #[cfg(not(target_os = "macos"))]
            let modifier = Key::Control;

            let _ = enigo.key(modifier, Direction::Press);
            let _ = enigo.key(Key::Unicode('c'), Direction::Click);
            let _ = enigo.key(modifier, Direction::Release);
        }

        // 2. 自适应轮询读取选中文本（单次 15ms，长文本自适应上限放宽至 300ms，内容变化立即放行）
        let clipboard_reader = {
            let h = handle.clone();
            move || h.clipboard().read_text().ok()
        };
        let detected_text = crate::clipboard_gate::poll_clipboard_change(
            old_clipboard.as_deref(),
            clipboard_reader,
            std::time::Duration::from_millis(15),
            std::time::Duration::from_millis(300),
        )
        .await;

        if let Some(text) = detected_text {
            let clean = text.trim();
            if !clean.is_empty() {
                // 3. 智能推断目标语言（默认目标若为中文，中文对转英文；非中文对转中文）
                let target_lang = transly_core::resolve_smart_target_lang(clean, "zh-CN");
                let engine = pool.default_engine();
                let req = TranslationRequest {
                    text: clean.to_string(),
                    source_lang: "auto".to_string(),
                    target_lang,
                };

                if let Ok(res) = engine.translate(&req).await {
                    let g = glossary.read().await;
                    let final_text = g.apply(&res.text);

                    // 4. 将译文写入剪贴板并模拟粘贴覆盖原选区
                    if handle.clipboard().write_text(final_text).is_ok() {
                        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                        if let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) {
                            #[cfg(target_os = "macos")]
                            let modifier = Key::Meta;
                            #[cfg(not(target_os = "macos"))]
                            let modifier = Key::Control;

                            let _ = enigo.key(modifier, Direction::Press);
                            let _ = enigo.key(Key::Unicode('v'), Direction::Click);
                            let _ = enigo.key(modifier, Direction::Release);
                        }
                        // 留出应用接收粘贴事件的窗口时间
                        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                    }
                }
            }
        }

        // 5. 恢复用户原有剪贴板内容
        if let Some(old_clipboard) = old_clipboard {
            if let Err(e) = handle.clipboard().write_text(old_clipboard) {
                eprintln!("[Clipboard Warning] 原地替换后恢复原剪贴板失败: {e}");
            }
        }

        sel_flag.store(false, Ordering::SeqCst);
    });
}

pub fn trigger_selection_translation(
    handle: tauri::AppHandle,
    sel_flag: Arc<std::sync::atomic::AtomicBool>,
    counter: Arc<std::sync::atomic::AtomicU64>,
    pending_slot: Arc<PendingFloatRequests>,
) {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};
    use tauri_plugin_clipboard_manager::ClipboardExt;

    tauri::async_runtime::spawn(async move {
        // 获取互斥锁排队，避免划词悬浮与原地替换同时模拟读写剪贴板发生数据竞态
        let _guard = CLIPBOARD_SIMULATION_MUTEX.lock().await;

        // 0. 备份旧剪贴板内容
        let old_clipboard = handle.clipboard().read_text().ok();

        // 模拟复制期间置抑制标志，防止剪贴板监听对同一次复制自触发翻译
        sel_flag.store(true, Ordering::SeqCst);

        // 1. 模拟按键发送 Ctrl+C (Windows/Linux) 或 Cmd+C (macOS)
        if let Ok(mut enigo) = Enigo::new(&EnigoSettings::default()) {
            #[cfg(target_os = "macos")]
            let modifier = Key::Meta;
            #[cfg(not(target_os = "macos"))]
            let modifier = Key::Control;

            let _ = enigo.key(modifier, Direction::Press);
            let _ = enigo.key(Key::Unicode('c'), Direction::Click);
            let _ = enigo.key(modifier, Direction::Release);
        }

        // 2. 自适应短轮询检测系统剪贴板更新（单次 15ms，最多等 180ms，内容变化立即放行）
        let clipboard_reader = {
            let h = handle.clone();
            move || h.clipboard().read_text().ok()
        };
        let detected_text = crate::clipboard_gate::poll_clipboard_change(
            old_clipboard.as_deref(),
            clipboard_reader,
            std::time::Duration::from_millis(15),
            std::time::Duration::from_millis(180),
        )
        .await;

        // 3. 提取剪贴板文本并唤起轻量悬浮窗
        if let Some(text) = detected_text {
            if !text.trim().is_empty() {
                let rid = counter.fetch_add(1, Ordering::SeqCst);
                let req = FloatRequest {
                    text,
                    source: "selection",
                    request_id: rid,
                };
                // 先暂存再广播：浮窗未就绪时靠主动拉取兜底
                if pending_slot.push(req.clone()) {
                    eprintln!("浮窗请求队列已满，丢弃最旧请求");
                }
                if let Some(float_win) = handle.get_webview_window("float") {
                    let _ = float_win.show();
                    let _ = float_win.set_focus();
                    let _ = float_win.emit_to("float", "float-translate", req);
                }
            }
        }

        // 4. 恢复原剪贴板内容，避免污染用户复制历史
        if let Some(old_clipboard) = old_clipboard {
            if let Err(e) = handle.clipboard().write_text(old_clipboard) {
                eprintln!("[Clipboard Warning] 划词后恢复原剪贴板失败（可能被外部锁占用）: {e}");
            }
        }

        // 无论成败均释放抑制标志，避免剪贴板监听被永久抑制
        sel_flag.store(false, Ordering::SeqCst);
    });
}

#[tauri::command]
#[specta::specta]
pub async fn get_shortcut() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        crate::secrets::get_secret("custom_shortcut")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_SHORTCUT.to_string())
    .pipe(Ok)
}

#[tauri::command]
#[specta::specta]
pub async fn set_shortcut(
    shortcut: String,
    app: tauri::AppHandle,
    selection_ctx: tauri::State<'_, Arc<SelectionContext>>,
    pending_slot: tauri::State<'_, Arc<PendingFloatRequests>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let clean = shortcut.trim();
    if clean.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    // 读取当前生效的旧快捷键
    let old_shortcut = tokio::task::spawn_blocking(|| {
        crate::secrets::get_secret("custom_shortcut")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_SHORTCUT.to_string());

    // 尝试注销旧快捷键（若与新快捷键不同）
    if old_shortcut != clean {
        let _ = app.global_shortcut().unregister(old_shortcut.as_str());
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
        return Err(format!(
            "快捷键注册失败，可能按键格式不合法或被系统占用: {e}"
        ));
    }

    // 持久化存储新快捷键
    let to_save = clean.to_string();
    tokio::task::spawn_blocking(move || crate::secrets::set_secret("custom_shortcut", &to_save))
        .await
        .map_err(|e| format!("快捷键调度异常: {e}"))?
        .map_err(|e| format!("保存快捷键失败: {e}"))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_replace_shortcut() -> Result<String, String> {
    tokio::task::spawn_blocking(|| {
        crate::secrets::get_secret("custom_replace_shortcut")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REPLACE_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_REPLACE_SHORTCUT.to_string())
    .pipe(Ok)
}

#[tauri::command]
#[specta::specta]
pub async fn set_replace_shortcut(
    shortcut: String,
    app: tauri::AppHandle,
    selection_ctx: tauri::State<'_, Arc<SelectionContext>>,
    pool: tauri::State<'_, Arc<EnginePool>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

    let clean = shortcut.trim();
    if clean.is_empty() {
        return Err("快捷键不能为空".to_string());
    }

    let old_shortcut = tokio::task::spawn_blocking(|| {
        crate::secrets::get_secret("custom_replace_shortcut")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_REPLACE_SHORTCUT.to_string())
    })
    .await
    .unwrap_or_else(|_| DEFAULT_REPLACE_SHORTCUT.to_string());

    if old_shortcut != clean {
        let _ = app.global_shortcut().unregister(old_shortcut.as_str());
    }

    let app_h = app.clone();
    let sel_flag = selection_ctx.sel_flag.clone();
    let pool_ref = pool.inner().clone();
    let glossary_ref = glossary.inner().clone();

    let register_res = app
        .global_shortcut()
        .on_shortcut(clean, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                trigger_selection_replacement(
                    app_h.clone(),
                    sel_flag.clone(),
                    pool_ref.clone(),
                    glossary_ref.clone(),
                );
            }
        });

    if let Err(e) = register_res {
        return Err(format!(
            "划词替换快捷键注册失败，可能按键格式不合法或被系统占用: {e}"
        ));
    }

    let to_save = clean.to_string();
    tokio::task::spawn_blocking(move || {
        crate::secrets::set_secret("custom_replace_shortcut", &to_save)
    })
    .await
    .map_err(|e| format!("快捷键调度异常: {e}"))?
    .map_err(|e| format!("保存快捷键失败: {e}"))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn detect_text_language(text: String) -> Result<Option<String>, String> {
    Ok(transly_core::detect_language(&text).map(|s| s.to_string()))
}

trait Pipe: Sized {
    fn pipe<F, R>(self, f: F) -> R
    where
        F: FnOnce(Self) -> R,
    {
        f(self)
    }
}

impl<T> Pipe for T {}

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
) -> Result<(), String> {
    let eng = engine.to_lowercase();
    if eng != "openai" && eng != "deepseek" {
        return Err("仅支持配置 OpenAI 或 DeepSeek 的端点与模型".to_string());
    }

    let url_key = format!("{eng}_base_url");
    let model_key = format!("{eng}_model");

    let url_val = base_url.unwrap_or_default().trim().to_string();
    let model_val = model.unwrap_or_default().trim().to_string();

    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if url_val.is_empty() {
            let _ = crate::secrets::delete_secret(&url_key);
        } else {
            crate::secrets::set_secret(&url_key, &url_val)?;
        }

        if model_val.is_empty() {
            let _ = crate::secrets::delete_secret(&model_key);
        } else {
            crate::secrets::set_secret(&model_key, &model_val)?;
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
pub async fn get_llm_config(engine: String) -> Result<LlmConfigDto, String> {
    let eng = engine.to_lowercase();
    if eng != "openai" && eng != "deepseek" {
        return Err("仅支持读取 OpenAI 或 DeepSeek 的端点与模型".to_string());
    }

    let url_key = format!("{eng}_base_url");
    let model_key = format!("{eng}_model");

    tokio::task::spawn_blocking(move || {
        let base_url = crate::secrets::get_secret(&url_key)
            .ok()
            .filter(|s| !s.trim().is_empty());
        let model = crate::secrets::get_secret(&model_key)
            .ok()
            .filter(|s| !s.trim().is_empty());
        Ok(LlmConfigDto { base_url, model })
    })
    .await
    .unwrap_or_else(|e| Err(format!("配置读取调度异常: {e}")))
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

    #[tokio::test]
    async fn test_pairing_commands_approve_and_reject() {
        let manager = Arc::new(crate::server::pairing::PairingManager::new());
        let (s1, _) = manager.create_session("client-1".into(), None).await;
        let (s2, _) = manager.create_session("client-2".into(), None).await;

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
}
