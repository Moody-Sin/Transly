//! 文档翻译 Tauri 命令封装

use std::sync::Arc;
use tauri::Emitter;
use transly_core::document::{
    translate_document_content_with_progress, DocumentFormat, DocumentProgressCallback,
};
use transly_core::engine::BatchPipelineDeps;
use transly_core::Error;

use super::{StreamTaskRegistry, TranslateErrorDto, TranslationDependencies};

/// 文档翻译执行结果 DTO
#[derive(serde::Serialize, serde::Deserialize, specta::Type)]
pub struct DocumentTranslateOutcomeDto {
    pub translated_content: String,
    pub total_chunks: u32,
    pub translated_chunks: u32,
    pub cache_hits: u32,
    pub engine: String,
}

/// 文档翻译实时进度广播载荷
#[derive(Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct DocumentProgressPayload {
    pub task_id: u32,
    pub completed_chunks: u32,
    pub total_chunks: u32,
    pub cache_hits: u32,
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
pub async fn translate_document(
    task_id: u32,
    content: String,
    format: DocumentFormat,
    source_lang: Option<String>,
    target_lang: String,
    engine_name: Option<String>,
    window: tauri::Window,
    deps: tauri::State<'_, Arc<TranslationDependencies>>,
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
        deps.pool.get_engine(name).await
    } else {
        deps.pool.default_engine()
    };

    let glossary_guard = deps.glossary.read().await;
    let cache_enabled = deps.cache_controller.is_enabled();

    let pipeline_deps = BatchPipelineDeps {
        engine: engine.as_ref(),
        cache: deps.cache.as_ref(),
        glossary: &glossary_guard,
        cache_enabled,
    };

    // 注册任务取消信号，支持中途主动打断
    let registry_key = (window.label().to_string(), task_id);
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
    if let Ok(mut map) = deps.stream_registry.0.lock() {
        map.insert(registry_key.clone(), cancel_tx);
    }

    // RAII 任务退出守卫，确保异常或正常完成均清理取消句柄
    struct Cleaner(Arc<StreamTaskRegistry>, (String, u32));
    impl Drop for Cleaner {
        fn drop(&mut self) {
            if let Ok(mut map) = self.0 .0.lock() {
                map.remove(&self.1);
            }
        }
    }
    let _cleaner = Cleaner(deps.stream_registry.clone(), registry_key);

    let win_clone = window.clone();
    let on_progress: DocumentProgressCallback = Box::new(move |done, total, hits| {
        let _ = win_clone.emit(
            "document-translate-progress",
            DocumentProgressPayload {
                task_id,
                completed_chunks: done,
                total_chunks: total,
                cache_hits: hits,
            },
        );
    });

    let outcome = translate_document_content_with_progress(
        pipeline_deps,
        &content,
        format,
        &sl,
        &target_lang,
        Some(on_progress),
        Some(&mut cancel_rx),
    )
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

/// 主动取消在途文档翻译任务
#[tauri::command]
#[specta::specta]
pub async fn cancel_document_task(
    task_id: u32,
    window: tauri::Window,
    registry: tauri::State<'_, Arc<StreamTaskRegistry>>,
) -> Result<(), String> {
    if let Ok(mut map) = registry.0.lock() {
        if let Some(tx) = map.remove(&(window.label().to_string(), task_id)) {
            let _ = tx.send(());
        }
    }
    Ok(())
}
