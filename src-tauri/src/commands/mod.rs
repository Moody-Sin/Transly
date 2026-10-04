use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::{
    detect::detect_language, DeepLEngine, FallbackEngine, Glossary, GoogleWebEngine, OpenAIEngine,
    TranslateEngine, TranslationCache, TranslationRequest,
};

/// 根据引擎名称和当前系统凭据动态构造引擎实例（支持主备降级）
pub fn resolve_engine(engine_id: &str) -> Arc<dyn TranslateEngine> {
    let google = Arc::new(GoogleWebEngine::new());
    match engine_id {
        "deepl" => {
            if let Ok(key) = crate::secrets::get_secret("deepl") {
                if !key.is_empty() {
                    let deepl = Arc::new(DeepLEngine::new(key));
                    return Arc::new(FallbackEngine::new(deepl, google));
                }
            }
            google
        }
        "openai" | "deepseek" => {
            if let Ok(key) = crate::secrets::get_secret(engine_id) {
                if !key.is_empty() {
                    let llm = if engine_id == "deepseek" {
                        Arc::new(OpenAIEngine::deepseek(key))
                    } else {
                        Arc::new(OpenAIEngine::new(key, None, None))
                    };
                    return Arc::new(FallbackEngine::new(llm, google));
                }
            }
            google
        }
        _ => google,
    }
}

#[tauri::command]
#[specta::specta]
pub async fn translate(
    text: String,
    source_lang: Option<String>,
    target_lang: String,
    engine_name: Option<String>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    default_engine: tauri::State<'_, Arc<dyn TranslateEngine>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<String, String> {
    let sl = source_lang.unwrap_or_else(|| "auto".to_string());

    // 如果指定了引擎且不同于默认，则动态解析并附加 Fallback 降级
    let active_engine: Arc<dyn TranslateEngine> = if let Some(ref name) = engine_name {
        resolve_engine(name)
    } else {
        default_engine.inner().clone()
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
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
) -> Result<(), String> {
    let mut g = glossary.write().await;
    g.insert(source, target);
    Ok(())
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
