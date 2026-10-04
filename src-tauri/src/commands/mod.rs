use crate::engine_pool::EnginePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::{detect::detect_language, Glossary, TranslationCache, TranslationRequest};

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
) -> Result<String, String> {
    let sl = source_lang.unwrap_or_else(|| "auto".to_string());

    // 从全局引擎池中快速复用长连接实例
    let active_engine = if let Some(ref name) = engine_name {
        pool.get_engine(name).await
    } else {
        pool.default_engine()
    };

    let engine_id = active_engine.id();

    // 1. 优先查缓存
    if let Ok(Some(cached_text)) = cache.get(&text, &sl, &target_lang, engine_id) {
        let g = glossary.read().await;
        return Ok(g.apply(&cached_text));
    }

    // 2. 缓存未命中，调用引擎翻译
    let req = TranslationRequest {
        text: text.clone(),
        source_lang: sl.clone(),
        target_lang: target_lang.clone(),
    };

    match active_engine.translate(&req).await {
        Ok(res) => {
            // 3. 异步写入缓存
            let _ = cache.set(&text, &res.text, &sl, &target_lang, engine_id);
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

use base64::{engine::general_purpose::STANDARD, Engine as _};

#[tauri::command]
#[specta::specta]
pub async fn ocr_image(
    image_base64: String,
    ocr: tauri::State<'_, Arc<transly_core::SystemOcrEngine>>,
) -> Result<String, String> {
    use transly_core::{OcrEngine, OcrRequest};
    let clean = image_base64.trim();
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
pub async fn save_api_key(
    engine: String,
    key: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
) -> Result<(), String> {
    crate::secrets::set_secret(&engine, &key).map_err(|e| e.to_string())?;
    pool.invalidate(&engine).await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_api_key(engine: String) -> Result<String, String> {
    crate::secrets::get_secret(&engine).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn delete_api_key(
    engine: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
) -> Result<(), String> {
    crate::secrets::delete_secret(&engine).map_err(|e| e.to_string())?;
    pool.invalidate(&engine).await;
    Ok(())
}
