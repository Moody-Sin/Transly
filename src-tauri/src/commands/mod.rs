use crate::engine_pool::EnginePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::{detect::detect_language, Glossary, TranslationCache, TranslationRequest};

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
) -> Result<String, String> {
    let sl = source_lang.unwrap_or_else(|| "auto".to_string());

    // 统一输入参数防线校验
    transly_core::validate_translation_input(&text, &sl, &target_lang)
        .map_err(|e| e.to_string())?;

    if let Some(ref eng) = engine_name {
        if !transly_core::validate_engine_id(eng) {
            return Err(format!("不支持或未知的翻译引擎: {eng}"));
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
            return Ok(g.apply(&cached_text));
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
            let cache_target_engine = if res.engine.ends_with("(fallback)") {
                "google_web"
            } else {
                engine_id
            };
            if let Err(e) = cache_controller.set_if_current(start_gen, || {
                cache.set(&text, &res.text, &sl, &target_lang, cache_target_engine)
            }) {
                eprintln!("[Cache Write Warning] 桌面端翻译缓存写入失败: {e}");
            }
            let g = glossary.read().await;
            Ok(g.apply(&res.text))
        }
        Err(e) => Err(e.to_string()),
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

use base64::{engine::general_purpose::STANDARD, Engine as _};

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
pub async fn ocr_image(
    image_base64: String,
    ocr: tauri::State<'_, Arc<transly_core::SystemOcrEngine>>,
) -> Result<String, String> {
    use transly_core::{OcrEngine, OcrRequest};
    let clean = image_base64.trim();
    // 限制 Base64 图像大小不超过 10MB
    if clean.len() > 10 * 1024 * 1024 {
        return Err("图像数据超出最大限制 (10MB)".to_string());
    }
    // 兼容可能传入的 data:image/...;base64, 前缀
    let raw_b64 = if let Some(idx) = clean.find(',') {
        &clean[idx + 1..]
    } else {
        clean
    };

    let bytes = match STANDARD.decode(raw_b64.as_bytes()) {
        Ok(b) => b,
        Err(_) => return Err("Base64 图像解析失败".to_string()),
    };

    let req = OcrRequest {
        image_bytes: bytes,
        lang: Some("auto".to_string()),
    };

    match ocr.recognize(&req).await {
        Ok(res) => Ok(res.text),
        Err(e) => Err(e.to_string()),
    }
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
