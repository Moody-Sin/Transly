use transly_core::detect::detect_language;

#[tauri::command]
pub async fn translate(text: String, target_lang: String) -> Result<String, String> {
    // 基础骨架占位，后续统一路由至 transly-core 引擎调度
    Ok(format!("[{target_lang}] {text}"))
}

#[tauri::command]
pub fn detect_lang(text: String) -> String {
    detect_language(&text)
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
