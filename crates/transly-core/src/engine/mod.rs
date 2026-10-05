use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod deepl;
pub mod fallback;
pub mod google;
pub mod openai;
pub use deepl::DeepLEngine;
pub use fallback::FallbackEngine;
pub use google::GoogleWebEngine;
pub use openai::OpenAIEngine;

use crate::{error::ErrorKind, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationResponse {
    pub text: String,
    pub detected_source_lang: Option<String>,
    pub engine: String,
}

impl TranslationResponse {
    pub fn actual_engine(&self) -> &str {
        self.engine
            .strip_suffix("(fallback)")
            .unwrap_or(&self.engine)
    }

    pub fn is_fallback(&self) -> bool {
        self.engine.ends_with("(fallback)")
    }
}

/// 批量翻译中单项请求契约（保证输入与输出具备稳定映射）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchItemRequest {
    pub id: String,
    pub text: String,
}

/// 批量翻译中单项结果契约（支持逐项表达成功或部分失败）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BatchItemResult {
    Success {
        id: String,
        text: String,
        detected_source_lang: Option<String>,
        engine: String,
    },
    Failed {
        id: String,
        error_kind: ErrorKind,
        message: String,
    },
}

impl BatchItemResult {
    pub fn id(&self) -> &str {
        match self {
            BatchItemResult::Success { id, .. } => id,
            BatchItemResult::Failed { id, .. } => id,
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, BatchItemResult::Success { .. })
    }

    pub fn engine(&self) -> Option<&str> {
        match self {
            BatchItemResult::Success { engine, .. } => Some(engine.as_str()),
            BatchItemResult::Failed { .. } => None,
        }
    }
}

/// 翻译引擎能力描述符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub id: String,
    pub name: String,
    pub max_batch_size: usize,
    pub supports_streaming: bool,
    pub supports_batch: bool,
    pub requires_api_key: bool,
}

#[async_trait]
pub trait TranslateEngine: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            id: self.id().to_string(),
            name: self.name().to_string(),
            max_batch_size: 100,
            supports_streaming: false,
            supports_batch: true,
            requires_api_key: false,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse>;

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        let mut results = Vec::with_capacity(requests.len());
        for req in requests {
            results.push(self.translate(req).await?);
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_item_contracts() {
        let success_item = BatchItemResult::Success {
            id: "node_1".to_string(),
            text: "测试译文".to_string(),
            detected_source_lang: Some("en".to_string()),
            engine: "google_web".to_string(),
        };

        let failed_item = BatchItemResult::Failed {
            id: "node_2".to_string(),
            error_kind: ErrorKind::Timeout,
            message: "请求超时".to_string(),
        };

        assert_eq!(success_item.id(), "node_1");
        assert!(success_item.is_success());

        assert_eq!(failed_item.id(), "node_2");
        assert!(!failed_item.is_success());

        // 验证序列化格式
        let json_success = serde_json::to_string(&success_item).unwrap();
        assert!(json_success.contains(r#""status":"success""#));
        assert!(json_success.contains(r#""text":"测试译文""#));

        let json_failed = serde_json::to_string(&failed_item).unwrap();
        assert!(json_failed.contains(r#""status":"failed""#));
        assert!(json_failed.contains(r#""error_kind":"timeout""#));
    }
}
