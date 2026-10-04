use std::sync::Arc;

use async_trait::async_trait;

use super::{TranslateEngine, TranslationRequest, TranslationResponse};
use crate::Result;

/// 支持主备容灾降级的翻译引擎包装器
pub struct FallbackEngine {
    primary: Arc<dyn TranslateEngine>,
    fallback: Arc<dyn TranslateEngine>,
}

impl FallbackEngine {
    pub fn new(primary: Arc<dyn TranslateEngine>, fallback: Arc<dyn TranslateEngine>) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl TranslateEngine for FallbackEngine {
    fn id(&self) -> &str {
        self.primary.id()
    }

    fn name(&self) -> &str {
        self.primary.name()
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        match self.primary.translate(req).await {
            Ok(res) => Ok(res),
            Err(primary_err) => match self.fallback.translate(req).await {
                Ok(mut res) => {
                    // 保留备用引擎标识或注明来源
                    if !res.engine.ends_with("(fallback)") {
                        res.engine = format!("{}(fallback)", res.engine);
                    }
                    Ok(res)
                }
                Err(_) => Err(primary_err),
            },
        }
    }

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        match self.primary.translate_batch(requests).await {
            Ok(results) => Ok(results),
            Err(primary_err) => match self.fallback.translate_batch(requests).await {
                Ok(mut results) => {
                    for r in &mut results {
                        if !r.engine.ends_with("(fallback)") {
                            r.engine = format!("{}(fallback)", r.engine);
                        }
                    }
                    Ok(results)
                }
                Err(_) => Err(primary_err),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    struct SuccessEngine(&'static str);
    #[async_trait]
    impl TranslateEngine for SuccessEngine {
        fn id(&self) -> &str {
            self.0
        }
        fn name(&self) -> &str {
            self.0
        }
        async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
            Ok(TranslationResponse {
                text: format!("{}:{}", self.0, req.text),
                detected_source_lang: Some("en".to_string()),
                engine: self.0.to_string(),
            })
        }
    }

    struct FailingEngine;
    #[async_trait]
    impl TranslateEngine for FailingEngine {
        fn id(&self) -> &str {
            "failing"
        }
        fn name(&self) -> &str {
            "Failing"
        }
        async fn translate(&self, _req: &TranslationRequest) -> Result<TranslationResponse> {
            Err(Error::Engine {
                engine: "failing".to_string(),
                message: "network down".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_fallback_when_primary_fails() {
        let primary = Arc::new(FailingEngine);
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);

        let req = TranslationRequest {
            text: "hello".to_string(),
            source_lang: "en".to_string(),
            target_lang: "zh-CN".to_string(),
        };

        let res = engine.translate(&req).await.unwrap();
        assert_eq!(res.text, "backup:hello");
        assert_eq!(res.engine, "backup(fallback)");
    }

    #[tokio::test]
    async fn test_primary_used_when_successful() {
        let primary = Arc::new(SuccessEngine("primary"));
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);

        let req = TranslationRequest {
            text: "hello".to_string(),
            source_lang: "en".to_string(),
            target_lang: "zh-CN".to_string(),
        };

        let res = engine.translate(&req).await.unwrap();
        assert_eq!(res.text, "primary:hello");
        assert_eq!(res.engine, "primary");
    }
}
