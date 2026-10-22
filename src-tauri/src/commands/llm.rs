//! LLM 配置 / API Key / 引擎能力 / 网络代理
//!
//! 由 `commands/mod.rs` 拆分而来：命令函数保持在 crate 内的原路径不变
//! （`mod.rs` 统一 `pub use` 重导出），因此 `app_command_builder!` 与
//! `src/bindings.ts` 的生成结果不受影响。

use std::sync::Arc;

use crate::engine_pool::EnginePool;
use transly_core::{ProxyConfig, ProxyMode, TranslationCache};

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
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let eng = engine.to_lowercase();
    if eng != "openai" && eng != "deepseek" {
        return Err("仅支持配置 OpenAI 或 DeepSeek 的端点与模型".to_string());
    }

    let url_key = format!("{eng}_base_url");
    let model_key = format!("{eng}_model");

    let url_val = base_url.unwrap_or_default().trim().to_string();
    let model_val = model.unwrap_or_default().trim().to_string();

    // 端点校验：密钥会被明文发往该地址，因此必须限定协议。
    // 允许任意 `http://` 意味着 API Key 可能以明文过网；
    // 唯一的例外是本地回环（Ollama / vLLM 等本机部署，不出网卡）。
    if !url_val.is_empty() {
        let is_loopback = transly_core::OpenAIEngine::is_local_endpoint(&url_val);
        if !url_val.starts_with("https://") && !is_loopback {
            return Err(
                "端点必须使用 https://（本地 Ollama 等本机服务可用 http://127.0.0.1 或 http://localhost）"
                    .to_string(),
            );
        }
    }

    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        if url_val.is_empty() {
            let _ = cache_ref.delete_setting(&url_key);
            let _ = crate::secrets::delete_secret(&url_key);
        } else {
            cache_ref.set_setting(&url_key, &url_val)?;
            let _ = crate::secrets::delete_secret(&url_key);
        }

        if model_val.is_empty() {
            let _ = cache_ref.delete_setting(&model_key);
            let _ = crate::secrets::delete_secret(&model_key);
        } else {
            cache_ref.set_setting(&model_key, &model_val)?;
            let _ = crate::secrets::delete_secret(&model_key);
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
pub async fn get_llm_config(
    engine: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<LlmConfigDto, String> {
    let eng = engine.to_lowercase();
    if eng != "openai" && eng != "deepseek" {
        return Err("仅支持读取 OpenAI 或 DeepSeek 的端点与模型".to_string());
    }

    let url_key = format!("{eng}_base_url");
    let model_key = format!("{eng}_model");
    let cache_ref = cache.inner().clone();

    tokio::task::spawn_blocking(move || {
        let base_url = cache_ref
            .get_setting(&url_key)
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret(&url_key).ok())
            .filter(|s| !s.trim().is_empty());
        let model = cache_ref
            .get_setting(&model_key)
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret(&model_key).ok())
            .filter(|s| !s.trim().is_empty());
        Ok(LlmConfigDto { base_url, model })
    })
    .await
    .unwrap_or_else(|e| Err(format!("配置读取调度异常: {e}")))
}

#[tauri::command]
#[specta::specta]
pub async fn save_llm_style(
    style: String,
    pool: tauri::State<'_, Arc<EnginePool>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let s = style.trim().to_lowercase();
    match s.as_str() {
        "general" | "academic" | "literary" | "business" => {}
        _ => return Err("无效的 LLM 翻译风格".to_string()),
    }

    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_ref.set_setting("llm_style", &s)?;
        let _ = crate::secrets::delete_secret("llm_style");
        Ok(())
    })
    .await
    .map_err(|e| format!("保存 LLM 风格任务调度异常: {e}"))?
    .map_err(|e| format!("写入 LLM 风格失败: {e}"))?;

    pool.invalidate("openai").await;
    pool.invalidate("deepseek").await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_preferred_replace_engine(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("preferred_replace_engine")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("preferred_replace_engine").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "google".to_string())
    })
    .await
    .map_err(|e| format!("读取划词原地替换引擎配置异常: {e}"))
}

pub fn validate_preferred_replace_engine(engine: &str) -> Result<String, String> {
    let clean = engine.trim().to_lowercase();
    match clean.as_str() {
        "google" | "google_web" => Ok("google".to_string()),
        "deepl" | "deepseek" | "openai" | "offline" => Ok(clean),
        _ => Err("无效或不受支持的划词替换引擎".to_string()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn save_preferred_replace_engine(
    engine: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let to_save = validate_preferred_replace_engine(&engine)?;
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_ref.set_setting("preferred_replace_engine", &to_save)?;
        let _ = crate::secrets::delete_secret("preferred_replace_engine");
        Ok(())
    })
    .await
    .map_err(|e| format!("保存划词原地替换引擎调度异常: {e}"))?
    .map_err(|e| format!("写入系统配置失败: {e}"))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_preferred_target_lang(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("preferred_target_lang")
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "zh-CN".to_string())
    })
    .await
    .map_err(|e| format!("读取默认目标语言配置异常: {e}"))
}

#[tauri::command]
#[specta::specta]
pub async fn save_preferred_target_lang(
    target_lang: String,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let clean = target_lang.trim().to_string();
    if !transly_core::validate_language_code(&clean) || clean.eq_ignore_ascii_case("auto") {
        return Err("无效或不受支持的目标语言代码".to_string());
    }
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        cache_ref.set_setting("preferred_target_lang", &clean)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("保存默认目标语言调度异常: {e}"))?
    .map_err(|e| format!("写入系统配置失败: {e}"))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_llm_style(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<String, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || {
        cache_ref
            .get_setting("llm_style")
            .ok()
            .flatten()
            .or_else(|| crate::secrets::get_secret("llm_style").ok())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "general".to_string())
    })
    .await
    .map_err(|e| format!("读取 LLM 风格任务调度异常: {e}"))
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
pub async fn is_api_key_configured(engine: String, app: tauri::AppHandle) -> bool {
    use tauri::Manager;
    if validate_engine_name(&engine).is_err() {
        return false;
    }
    let cache_opt = app
        .try_state::<Arc<TranslationCache>>()
        .map(|c| c.inner().clone());
    tokio::task::spawn_blocking(move || {
        if engine == "openai" || engine == "deepseek" {
            let setting_key = format!("{engine}_base_url");
            let url_opt = cache_opt
                .as_ref()
                .and_then(|c| c.get_setting(&setting_key).ok().flatten())
                .or_else(|| crate::secrets::get_secret(&setting_key).ok());
            if let Some(url) = url_opt {
                if transly_core::OpenAIEngine::is_local_endpoint(&url) {
                    return true;
                }
            }
        }
        crate::secrets::get_secret(&engine)
            .map(|k| !k.trim().is_empty())
            .unwrap_or(false)
    })
    .await
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

pub fn validate_proxy_config(config: &ProxyConfig) -> Result<(), String> {
    if config.mode == ProxyMode::Manual {
        match config.url {
            Some(ref u) if !u.trim().is_empty() => {
                if transly_core::normalize_proxy_url(u).is_none() {
                    return Err("无效的手动代理地址格式".to_string());
                }
            }
            _ => return Err("手动代理模式下必须填写代理地址".to_string()),
        }
    }
    Ok(())
}

pub fn get_proxy_config_internal(cache: &TranslationCache) -> Result<ProxyConfig, String> {
    let mode_str = cache
        .get_setting("proxy_mode")
        .ok()
        .flatten()
        .unwrap_or_else(|| "auto".to_string());
    let mode = ProxyMode::parse_str(&mode_str);
    let url = cache
        .get_setting("proxy_url")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty());
    Ok(ProxyConfig { mode, url })
}

pub async fn save_proxy_config_internal(
    config: ProxyConfig,
    pool: &EnginePool,
    cache: Arc<TranslationCache>,
) -> Result<(), String> {
    validate_proxy_config(&config)?;

    let mode_str = config.mode.as_str().to_string();
    let url_opt = config.url.clone();

    tokio::task::spawn_blocking(move || -> Result<(), String> {
        cache
            .set_setting("proxy_mode", &mode_str)
            .map_err(|e| format!("写入系统配置失败: {e}"))?;

        if let Some(ref u) = url_opt {
            let trimmed = u.trim();
            if !trimmed.is_empty() {
                cache
                    .set_setting("proxy_url", trimmed)
                    .map_err(|e| format!("写入系统配置失败: {e}"))?;
            } else {
                cache
                    .delete_setting("proxy_url")
                    .map_err(|e| format!("清理系统配置失败: {e}"))?;
            }
        } else {
            cache
                .delete_setting("proxy_url")
                .map_err(|e| format!("清理系统配置失败: {e}"))?;
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("保存网络代理配置调度异常: {e}"))??;

    pool.reload_proxy().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_proxy_config(
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<ProxyConfig, String> {
    let cache_ref = cache.inner().clone();
    tokio::task::spawn_blocking(move || get_proxy_config_internal(&cache_ref))
        .await
        .map_err(|e| format!("读取网络代理配置调度异常: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub async fn save_proxy_config(
    config: ProxyConfig,
    pool: tauri::State<'_, Arc<EnginePool>>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
) -> Result<(), String> {
    let pool_ref = pool.inner().clone();
    let cache_ref = cache.inner().clone();
    save_proxy_config_internal(config, &pool_ref, cache_ref).await
}
