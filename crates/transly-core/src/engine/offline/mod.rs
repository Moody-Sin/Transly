//! 全离线混合翻译引擎（内置词库 + 本地模型扩展）
//!
//! 在完全断网或无外部网络服务时，提供毫秒级、高确定性的本地双向翻译保障。

pub mod lexicon;
pub mod model;

pub use lexicon::{get_lexicon_entries_count, translate_en_to_zh, translate_zh_to_en};
pub use model::{inspect_local_models, OfflineEngineStatusDto};

use async_trait::async_trait;
use std::path::PathBuf;

use super::{EngineCapabilities, TranslateEngine, TranslationRequest, TranslationResponse};
use crate::{detector::detect_language, Result};

/// 全离线轻量嵌入式翻译引擎
pub struct OfflineEngine {
    engine_id: String,
    models_dir: Option<PathBuf>,
}

impl Default for OfflineEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OfflineEngine {
    pub fn new() -> Self {
        Self {
            engine_id: "offline".to_string(),
            models_dir: None,
        }
    }

    pub fn with_models_dir(models_dir: PathBuf) -> Self {
        Self {
            engine_id: "offline".to_string(),
            models_dir: Some(models_dir),
        }
    }

    /// 查询当前离线引擎状态与模型配置
    pub fn get_status(&self) -> OfflineEngineStatusDto {
        let (has_local_model, local_model_name) = self
            .models_dir
            .as_deref()
            .map(inspect_local_models)
            .unwrap_or((false, None));

        OfflineEngineStatusDto {
            lexicon_entries_count: get_lexicon_entries_count(),
            has_local_model,
            local_model_name,
            model_dir: self
                .models_dir
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl TranslateEngine for OfflineEngine {
    fn id(&self) -> &str {
        &self.engine_id
    }

    fn name(&self) -> &str {
        "Offline (纯本地离线引擎)"
    }

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            id: self.id().to_string(),
            name: self.name().to_string(),
            max_batch_size: crate::MAX_BATCH_ITEMS,
            supports_batch: true,
            requires_api_key: false,
            supports_streaming: true,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        let detected = detect_language(&req.text).map(|s| s.to_string());
        let target_clean = req.target_lang.trim().to_lowercase();
        let source_clean = req.source_lang.trim().to_lowercase();

        // 若源语言与目标语言相同，直接原样输出
        if !source_clean.is_empty()
            && source_clean != "auto"
            && (source_clean == target_clean
                || (source_clean.starts_with("zh") && target_clean.starts_with("zh"))
                || (source_clean.starts_with("en") && target_clean.starts_with("en")))
        {
            return Ok(TranslationResponse {
                text: req.text.clone(),
                detected_source_lang: detected,
                engine: self.id().to_string(),
            });
        }

        // 依据目标语言执行混合词库与短语对转
        let text = if target_clean.starts_with("zh") {
            translate_en_to_zh(&req.text)
        } else if target_clean.starts_with("en") {
            translate_zh_to_en(&req.text)
        } else {
            // 其余语种先走英汉双向转换或原样保留
            req.text.clone()
        };

        Ok(TranslationResponse {
            text,
            detected_source_lang: detected,
            engine: self.id().to_string(),
        })
    }

    async fn translate_stream(
        &self,
        req: &TranslationRequest,
        on_chunk: super::StreamCallback,
    ) -> Result<TranslationResponse> {
        let res = self.translate(req).await?;
        on_chunk(&res.text);
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_offline_engine_capabilities() {
        let engine = OfflineEngine::new();
        let caps = engine.capabilities();
        assert_eq!(caps.id, "offline");
        assert!(caps.supports_batch);
        assert!(!caps.requires_api_key);
        assert!(caps.supports_streaming);
    }

    #[tokio::test]
    async fn test_offline_engine_translate() {
        let engine = OfflineEngine::new();
        let req = TranslationRequest {
            text: "Hello world, artificial intelligence is open source.".to_string(),
            source_lang: "en".to_string(),
            target_lang: "zh-CN".to_string(),
        };
        let res = engine.translate(&req).await.unwrap();
        assert_eq!(res.engine, "offline");
        assert!(res.text.contains("你好"));
        assert!(res.text.contains("人工智能"));
        assert!(res.text.contains("开源"));
    }

    #[tokio::test]
    async fn test_offline_engine_status() {
        let engine = OfflineEngine::new();
        let status = engine.get_status();
        assert!(status.lexicon_entries_count > 50);
        assert!(!status.has_local_model);
    }
}
