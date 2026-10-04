use std::sync::Arc;
use transly_core::{
    detect::detect_language, engine::GoogleWebEngine, TranslateEngine, TranslationCache,
    TranslationRequest,
};

// 预初始化全局引擎实例
fn get_engine() -> Arc<dyn TranslateEngine> {
    Arc::new(GoogleWebEngine::new())
}

#[tauri::command]
#[specta::specta]
pub async fn translate(
    text: String,
    source_lang: Option<String>,
    target_lang: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let engine = get_engine();
    let sl = source_lang.unwrap_or_else(|| "auto".to_string());
    let engine_id = engine.id();

    // 1. 优先查缓存
    if let Ok(Some(cached_text)) = cache.get(&text, &sl, &target_lang, engine_id) {
        return Ok(cached_text);
    }

    // 2. 缓存未命中，调用引擎翻译
    let req = TranslationRequest {
        text: text.clone(),
        source_lang: sl.clone(),
        target_lang: target_lang.clone(),
    };

    match engine.translate(&req).await {
        Ok(res) => {
            // 3. 异步写入缓存
            let _ = cache.set(&text, &res.text, &sl, &target_lang, engine_id);
            Ok(res.text)
        }
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
pub fn save_api_key(engine: String, key: String) -> Result<(), String> {
    crate::secrets::set_secret(&engine, &key).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_api_key(engine: String) -> Result<String, String> {
    crate::secrets::get_secret(&engine).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn delete_api_key(engine: String) -> Result<(), String> {
    crate::secrets::delete_secret(&engine).map_err(|e| e.to_string())
}
