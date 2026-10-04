use std::sync::Arc;
use transly_core::{
    detect::detect_language, engine::GoogleWebEngine, TranslateEngine, TranslationRequest,
};

// 预初始化全局引擎实例
fn get_engine() -> Arc<dyn TranslateEngine> {
    Arc::new(GoogleWebEngine::new())
}

#[tauri::command]
pub async fn translate(
    text: String,
    source_lang: Option<String>,
    target_lang: String,
) -> Result<String, String> {
    let engine = get_engine();
    let sl = source_lang.unwrap_or_else(|| "auto".to_string());
    let req = TranslationRequest {
        text,
        source_lang: sl,
        target_lang,
    };

    match engine.translate(&req).await {
        Ok(res) => Ok(res.text),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn detect_lang(text: String) -> String {
    detect_language(&text)
}

#[tauri::command]
pub fn get_server_token() -> Result<String, String> {
    crate::secrets::get_or_create_server_token().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_api_key(engine: String, key: String) -> Result<(), String> {
    crate::secrets::set_secret(&engine, &key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_api_key(engine: String) -> Result<String, String> {
    crate::secrets::get_secret(&engine).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_api_key(engine: String) -> Result<(), String> {
    crate::secrets::delete_secret(&engine).map_err(|e| e.to_string())
}
