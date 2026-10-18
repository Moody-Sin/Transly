//! 文档翻译 Tauri 命令封装

use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::document::{translate_document_content, DocumentFormat};
use transly_core::engine::BatchPipelineDeps;
use transly_core::{Error, Glossary, TranslationCache};

use super::TranslateErrorDto;
use crate::engine_pool::EnginePool;

/// 文档翻译执行结果 DTO
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct DocumentTranslateOutcomeDto {
    pub translated_content: String,
    pub total_chunks: u32,
    pub translated_chunks: u32,
    pub cache_hits: u32,
    pub engine: String,
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn translate_document(
    content: String,
    format: DocumentFormat,
    source_lang: Option<String>,
    target_lang: String,
    engine_name: Option<String>,
    cache: tauri::State<'_, Arc<TranslationCache>>,
    pool: tauri::State<'_, Arc<EnginePool>>,
    glossary: tauri::State<'_, Arc<RwLock<Glossary>>>,
    cache_controller: tauri::State<'_, Arc<crate::cache_controller::CacheController>>,
) -> Result<DocumentTranslateOutcomeDto, TranslateErrorDto> {
    let sl = source_lang.unwrap_or_else(|| "auto".to_string());

    transly_core::validate_language_pair(&sl, &target_lang)
        .map_err(|e| TranslateErrorDto::from_error(&e))?;

    if let Some(ref eng) = engine_name {
        if !transly_core::validate_engine_id(eng) {
            return Err(TranslateErrorDto::from_error(&Error::Input(format!(
                "不支持或未知的翻译引擎: {eng}"
            ))));
        }
    }

    let engine = if let Some(ref name) = engine_name {
        pool.get_engine(name).await
    } else {
        pool.default_engine()
    };

    let glossary_guard = glossary.read().await;
    let cache_enabled = cache_controller.is_enabled();

    let deps = BatchPipelineDeps {
        engine: engine.as_ref(),
        cache: cache.inner().as_ref(),
        glossary: &glossary_guard,
        cache_enabled,
    };

    let outcome = translate_document_content(deps, &content, format, &sl, &target_lang)
        .await
        .map_err(|e| TranslateErrorDto::from_error(&e))?;

    Ok(DocumentTranslateOutcomeDto {
        translated_content: outcome.translated_content,
        total_chunks: outcome.total_chunks,
        translated_chunks: outcome.translated_chunks,
        cache_hits: outcome.cache_hits,
        engine: outcome.engine,
    })
}
