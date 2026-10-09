use std::sync::Arc;

use async_trait::async_trait;

use super::{TranslateEngine, TranslationRequest, TranslationResponse};
use crate::{Error, Result};

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

    fn capabilities(&self) -> super::EngineCapabilities {
        let mut caps = self.primary.capabilities();
        // 标定具备降级容灾能力与明确的备用引擎标识
        caps.name = format!("{}(容灾降级)", caps.name);
        caps.fallback_engine = Some(self.fallback.id().to_string());
        caps
    }

    async fn translate_stream(
        &self,
        req: &TranslationRequest,
        on_chunk: super::StreamCallback,
    ) -> Result<TranslationResponse> {
        match self.primary.translate_stream(req, on_chunk).await {
            Ok(res) => Ok(res),
            Err(primary_err) => {
                if matches!(primary_err, Error::MissingCredentials(_)) {
                    return Err(primary_err);
                }
                match self.fallback.translate(req).await {
                    Ok(mut res) => {
                        if !res.engine.ends_with("(fallback)") {
                            res.engine = format!("{}(fallback)", res.engine);
                        }
                        Ok(res)
                    }
                    Err(_) => Err(primary_err),
                }
            }
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        match self.primary.translate(req).await {
            Ok(res) => Ok(res),
            Err(primary_err) => {
                // 如果是缺失凭据/未配置 API Key，直接向上报错，禁止静默由备用引擎接管
                if matches!(primary_err, Error::MissingCredentials(_)) {
                    return Err(primary_err);
                }
                match self.fallback.translate(req).await {
                    Ok(mut res) => {
                        // 保留备用引擎标识或注明来源
                        if !res.engine.ends_with("(fallback)") {
                            res.engine = format!("{}(fallback)", res.engine);
                        }
                        Ok(res)
                    }
                    Err(_) => Err(primary_err),
                }
            }
        }
    }

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        match self.primary.translate_batch(requests).await {
            Ok(results) => Ok(results),
            Err(primary_err) => {
                // 如果是缺失凭据/未配置 API Key，直接向上报错，禁止静默由备用引擎接管
                if matches!(primary_err, Error::MissingCredentials(_)) {
                    return Err(primary_err);
                }
                match self.fallback.translate_batch(requests).await {
                    Ok(mut results) => {
                        for r in &mut results {
                            if !r.engine.ends_with("(fallback)") {
                                r.engine = format!("{}(fallback)", r.engine);
                            }
                        }
                        Ok(results)
                    }
                    Err(_) => Err(primary_err),
                }
            }
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
        // 缓存写入用的是剥离后缀的 actual_engine，否则降级路径缓存永久 Miss
        assert_eq!(res.actual_engine(), "backup");
    }

    #[tokio::test]
    async fn test_fallback_stream_when_primary_fails() {
        let primary = Arc::new(FailingEngine);
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);

        let req = TranslationRequest {
            text: "hello".to_string(),
            source_lang: "en".to_string(),
            target_lang: "zh-CN".to_string(),
        };

        let collected = Arc::new(std::sync::Mutex::new(Vec::new()));
        let cb_collected = collected.clone();
        let res = engine
            .translate_stream(
                &req,
                Box::new(move |chunk| {
                    cb_collected.lock().unwrap().push(chunk.to_string());
                }),
            )
            .await
            .unwrap();

        assert_eq!(res.text, "backup:hello");
        assert_eq!(res.engine, "backup(fallback)");
        assert_eq!(res.actual_engine(), "backup");
    }

    #[test]
    fn test_fallback_capabilities() {
        let primary = Arc::new(SuccessEngine("primary"));
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);
        let caps = engine.capabilities();
        assert_eq!(caps.id, "primary");
        assert!(caps.name.contains("容灾降级"));
    }

    #[tokio::test]
    async fn test_fallback_does_not_mask_missing_credentials() {
        struct UnconfiguredEngine;
        #[async_trait]
        impl TranslateEngine for UnconfiguredEngine {
            fn id(&self) -> &str {
                "unconfigured"
            }
            fn name(&self) -> &str {
                "Unconfigured"
            }
            async fn translate(&self, _req: &TranslationRequest) -> Result<TranslationResponse> {
                Err(Error::MissingCredentials("API Key 不能为空".into()))
            }
        }

        let primary = Arc::new(UnconfiguredEngine);
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);

        let req = TranslationRequest {
            text: "hello".to_string(),
            source_lang: "en".to_string(),
            target_lang: "zh-CN".to_string(),
        };

        let err = engine.translate(&req).await.unwrap_err();
        assert!(
            matches!(err, Error::MissingCredentials(_)),
            "未配置密钥时禁止回退到备用引擎"
        );
    }
}
