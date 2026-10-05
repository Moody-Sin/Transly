use crate::engine_pool::EnginePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::task::classify_error;
use transly_core::{
    detect::detect_language, Error, ErrorKind, Glossary, TranslationCache, TranslationRequest,
};

/// 翻译成功结果：携带实际执行引擎与缓存命中标记，供 UI 展示与降级识别
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TranslateOutcomeDto {
    pub text: String,
    pub engine: String,
    pub cached: bool,
    pub detected_source_lang: Option<String>,
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

impl From<Error> for TranslateErrorDto {
    fn from(err: Error) -> Self {
        Self::from_error(&err)
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

    // 统一输入参数防线校验：参数非法属于不可重试的 Input 类
    transly_core::validate_translation_input(&text, &sl, &target_lang).map_err(|e| {
        TranslateErrorDto {
            kind: ErrorKind::Input,
            message: e.to_string(),
            retryable: false,
        }
    })?;

    if let Some(ref eng) = engine_name {
        if !transly_core::validate_engine_id(eng) {
            return Err(TranslateErrorDto {
                kind: ErrorKind::Input,
                message: format!("不支持或未知的翻译引擎: {eng}"),
                retryable: false,
            });
        }
    }

    // 从全局引擎池中快速复用长连接实例
    let active_engine = if let Some(ref name) = engine_name {
        pool.get_engine(name).await
    } else {
        pool.default_engine()
    };

    let engine_id = active_engine.id();

    // 检查缓存状态快照与 generation 版本号
    let (cache_enabled, start_gen) = cache_controller.snapshot();

    // 1. 若开启缓存，优先查缓存
    if cache_enabled {
        if let Ok(Some(cached_text)) = cache.get(&text, &sl, &target_lang, engine_id) {
            let g = glossary.read().await;
            let applied = g.apply(&cached_text);
            return Ok(TranslateOutcomeDto {
                text: applied,
                engine: engine_id.to_string(),
                cached: true,
                detected_source_lang: None,
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
                cached: false,
                detected_source_lang: res.detected_source_lang,
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
    cache
        .set_glossary_term(&source, &target)
        .map_err(|e| e.to_string())?;
    let mut g = glossary.write().await;
    g.insert(source, target);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_glossary_terms(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<Vec<(String, String)>, String> {
    cache.get_all_glossary_terms().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_glossary_term(
    source: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<(), String> {
    cache
        .delete_glossary_term(&source)
        .map_err(|e| e.to_string())?;
    // 重新构建内存术语库
    let terms = cache.get_all_glossary_terms().map_err(|e| e.to_string())?;
    let mut g = glossary.write().await;
    *g = Glossary::new();
    for (src, tgt) in terms {
        g.insert(src, tgt);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn clear_cache(cache: tauri::State<'_, Arc<TranslationCache>>) -> Result<u32, String> {
    cache
        .clear_all_cache()
        .map(|n| n as u32)
        .map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn set_cache_enabled(
    enabled: bool,
    controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    crate::secrets::set_secret("cache_enabled", if enabled { "true" } else { "false" })
        .map_err(|e| format!("写入系统配置失败: {e}"))?;

    if enabled {
        controller.enable();
        Ok(())
    } else {
        // 在同一门禁锁内递增 generation 并清空旧磁盘缓存，彻底原子闭环
        match controller.disable_and_clear(cache.inner()) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("已禁用缓存，但清空历史缓存记录失败: {e}")),
        }
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
pub fn set_clipboard_monitor(
    enabled: bool,
    flag: tauri::State<'_, Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(), String> {
    crate::secrets::set_secret(
        "clipboard_monitor_enabled",
        if enabled { "true" } else { "false" },
    )
    .map_err(|e| format!("写入系统安全凭据失败: {e}"))?;
    flag.store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_clipboard_monitor(_flag: tauri::State<'_, Arc<std::sync::atomic::AtomicBool>>) -> bool {
    crate::secrets::get_secret("clipboard_monitor_enabled")
        .map(|v| v == "true")
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

/// 暂存最近一次浮窗请求。
///
/// 浮窗 `visible: false` 时 Tauri 仍创建 webview，但页面 JS 尚未加载完成；
/// 此时 `emit` 的事件无人接收且不会重放。因此宿主先暂存，浮窗就绪后主动拉取，
/// 从根上消除 emit 时序竞态。
#[derive(Default)]
pub struct PendingFloatRequest(pub std::sync::Mutex<Option<FloatRequest>>);

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
    pending: tauri::State<'_, Arc<PendingFloatRequest>>,
) -> Result<PendingFloatResponse, String> {
    let mut slot = pending
        .0
        .lock()
        .map_err(|e| format!("读取待处理浮窗请求失败: {e}"))?;
    Ok(match slot.take() {
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
pub fn detect_lang(text: String) -> String {
    detect_language(&text)
}

#[tauri::command]
#[specta::specta]
pub fn get_server_token() -> Result<String, String> {
    crate::secrets::get_or_create_server_token().map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn rotate_server_token(
    current_token: tauri::State<'_, Arc<RwLock<String>>>,
) -> Result<String, String> {
    let new_token = crate::secrets::rotate_server_token().map_err(|e| e.to_string())?;
    let mut lock = current_token.write().await;
    *lock = new_token.clone();
    Ok(new_token)
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
    crate::secrets::set_secret(&engine, key.trim()).map_err(|e| e.to_string())?;
    pool.invalidate(&engine).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn is_api_key_configured(engine: String) -> bool {
    if validate_engine_name(&engine).is_err() {
        return false;
    }
    crate::secrets::get_secret(&engine)
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_api_key(
    engine: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
) -> Result<(), String> {
    validate_engine_name(&engine)?;
    crate::secrets::delete_secret(&engine).map_err(|e| e.to_string())?;
    pool.invalidate(&engine).await;
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct EngineCapabilitiesDto {
    pub id: String,
    pub name: String,
    pub max_batch_size: u32,
    pub supports_streaming: bool,
    pub supports_batch: bool,
    pub requires_api_key: bool,
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
        supports_streaming: caps.supports_streaming,
        supports_batch: caps.supports_batch,
        requires_api_key: caps.requires_api_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use transly_core::{Error, ErrorKind};

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
