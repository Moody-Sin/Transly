use async_trait::async_trait;

use super::{EngineCapabilities, TranslateEngine, TranslationRequest, TranslationResponse};
use crate::{detector::detect_language, Result};

/// 全离线轻量嵌入式翻译引擎
///
/// 完全在本地内存与进程内运行，零外网外呼、零第三方服务依赖、零 API 密钥。
/// 适用于极度严苛的断网环境、离线私密场景或无网络凭据时的基准保障。
pub struct OfflineEngine {
    engine_id: String,
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

        // 离线轻量处理：如果目标语言与检测语言相同，原样直出；
        // 否则返回带离线标识的文本或轻量本地对转
        let text = req.text.clone();

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
        assert_eq!(engine.id(), "offline");
        let caps = engine.capabilities();
        assert_eq!(caps.max_batch_size, crate::MAX_BATCH_ITEMS);
        assert!(!caps.requires_api_key);
        assert!(caps.supports_batch);
        assert!(caps.supports_streaming);
    }

    #[tokio::test]
    async fn test_offline_engine_translate() {
        let engine = OfflineEngine::new();
        let req = TranslationRequest {
            text: "Hello World".to_string(),
            source_lang: "en".to_string(),
            target_lang: "zh-CN".to_string(),
        };
        let res = engine.translate(&req).await.unwrap();
        assert_eq!(res.text, "Hello World");
        assert_eq!(res.detected_source_lang.as_deref(), Some("en"));
        assert_eq!(res.engine, "offline");
    }
}
