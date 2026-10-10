use std::sync::atomic::{AtomicBool, Ordering};
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
        // StreamCallback 本身已是 Box<dyn Fn(&str) + Send + Sync>，
        // 直接用 Arc<dyn Fn(..)> 共享，避免 Box<dyn ..> 的双重间接层。
        let callback: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(on_chunk);

        // 追踪主引擎是否已经向调用方推送过内容。主备共用同一个回调，若主引擎
        // 推送一半后失败而备用引擎继续推送，界面会显示「残缺主译文 + 完整备用
        // 译文」的拼接结果。前端最终会以响应整体覆盖文本，因此正确做法是
        // 已经推送过时让降级结果静默返回，交由最终响应完成纠正。
        let primary_streamed = Arc::new(AtomicBool::new(false));
        let cb_primary = {
            let callback = Arc::clone(&callback);
            let primary_streamed = Arc::clone(&primary_streamed);
            Box::new(move |chunk: &str| {
                primary_streamed.store(true, Ordering::Relaxed);
                callback(chunk);
            })
        };

        match self.primary.translate_stream(req, cb_primary).await {
            Ok(res) => Ok(res),
            Err(primary_err) => {
                if matches!(primary_err, Error::MissingCredentials(_)) {
                    return Err(primary_err);
                }

                let sink: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(|_: &str| {});
                let target = if primary_streamed.load(Ordering::Relaxed) {
                    sink
                } else {
                    Arc::clone(&callback)
                };
                let cb_fallback = Box::new(move |chunk: &str| target(chunk));

                match self.fallback.translate_stream(req, cb_fallback).await {
                    Ok(mut res) => {
                        if !res.engine.ends_with("(fallback)") {
                            res.engine = format!("{}(fallback)", res.engine);
                        }
                        Ok(res)
                    }
                    // 保留主引擎原始错误为主因，同时附上备用引擎错误以免排障被误导
                    Err(fallback_err) => Err(Error::Engine {
                        engine: format!("{}+{}", self.primary.id(), self.fallback.id()),
                        message: format!("主引擎失败：{primary_err}；备用引擎失败：{fallback_err}"),
                    }),
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

    async fn translate_batch_tolerant(
        &self,
        requests: &[TranslationRequest],
    ) -> Vec<Result<TranslationResponse>> {
        let primary_results = self.primary.translate_batch_tolerant(requests).await;
        // 以 requests 为准迭代：主引擎返回的条数与之不一定相等
        // （少返回则剩余项走降级，多返回的部分必须被丢弃而非越界访问）。
        let mut primary_iter = primary_results.into_iter();
        let mut final_results = Vec::with_capacity(requests.len());

        for request in requests.iter() {
            // 主引擎缺失该条结果时，构造一个可降级的通用错误
            let primary_outcome = primary_iter.next().unwrap_or_else(|| {
                Err(Error::Engine {
                    engine: self.primary.id().to_string(),
                    message: "主引擎未返回该条翻译结果".to_string(),
                })
            });

            match primary_outcome {
                Ok(ok_res) => final_results.push(Ok(ok_res)),
                Err(primary_err) => {
                    // 缺失凭据属于配置问题，禁止静默由备用引擎接管
                    if matches!(primary_err, Error::MissingCredentials(_)) {
                        final_results.push(Err(primary_err));
                        continue;
                    }
                    match self.fallback.translate(request).await {
                        Ok(mut fb_res) => {
                            if !fb_res.engine.ends_with("(fallback)") {
                                fb_res.engine = format!("{}(fallback)", fb_res.engine);
                            }
                            final_results.push(Ok(fb_res));
                        }
                        Err(fallback_err) => final_results.push(Err(Error::Engine {
                            engine: format!("{}+{}", self.primary.id(), self.fallback.id()),
                            message: format!(
                                "主引擎失败：{primary_err}；备用引擎失败：{fallback_err}"
                            ),
                        })),
                    }
                }
            }
        }

        final_results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::StreamCallback;
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
        assert!(
            !collected.lock().unwrap().is_empty(),
            "降级至备用引擎时必须广播流式 chunk"
        );
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

    /// 畸形引擎：批量返回条数与请求数不一致（模拟上游网关返回超长/超短数组）。
    struct RaggedBatchEngine {
        id: &'static str,
        /// None 表示该项失败
        returns: Vec<Option<String>>,
    }

    #[async_trait]
    impl TranslateEngine for RaggedBatchEngine {
        fn id(&self) -> &str {
            self.id
        }
        fn name(&self) -> &str {
            self.id
        }
        async fn translate(&self, _req: &TranslationRequest) -> Result<TranslationResponse> {
            unreachable!("本测试只走批量路径")
        }
        async fn translate_batch(
            &self,
            _requests: &[TranslationRequest],
        ) -> Result<Vec<TranslationResponse>> {
            unreachable!("本测试只走 translate_batch_tolerant")
        }
        async fn translate_batch_tolerant(
            &self,
            _requests: &[TranslationRequest],
        ) -> Vec<Result<TranslationResponse>> {
            self.returns
                .iter()
                .map(|item| match item {
                    Some(text) => Ok(TranslationResponse {
                        text: text.clone(),
                        detected_source_lang: Some("en".to_string()),
                        engine: self.id.to_string(),
                    }),
                    None => Err(Error::Engine {
                        engine: self.id.to_string(),
                        message: "boom".to_string(),
                    }),
                })
                .collect()
        }
    }

    fn reqs(n: usize) -> Vec<TranslationRequest> {
        (0..n)
            .map(|i| TranslationRequest {
                text: format!("t{i}"),
                source_lang: "en".to_string(),
                target_lang: "zh-CN".to_string(),
            })
            .collect()
    }

    /// 回归：主引擎批量返回条数多于请求数时，旧实现以 primary_results 的下标
    /// 去索引 requests，必然越界 panic（tokio worker 线程崩溃）。
    #[tokio::test]
    async fn test_tolerant_batch_survives_overlong_primary_result() {
        let primary = Arc::new(RaggedBatchEngine {
            id: "primary",
            returns: vec![
                Some("a".into()),
                Some("b".into()),
                Some("c".into()),
                Some("d".into()),
                Some("e".into()),
                Some("f".into()),
            ],
        });
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);

        // 请求 3 项、引擎返回 6 项：不得 panic，且结果长度必须等于请求数
        let results = engine.translate_batch_tolerant(&reqs(3)).await;
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap().text, "a");
    }

    /// 回归：主引擎批量返回条数少于请求数时，缺失项应走降级而非整批丢弃。
    #[tokio::test]
    async fn test_tolerant_batch_survives_short_primary_result() {
        let primary = Arc::new(RaggedBatchEngine {
            id: "primary",
            returns: vec![Some("only".into())],
        });
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);

        let results = engine.translate_batch_tolerant(&reqs(4)).await;
        assert_eq!(results.len(), 4, "结果条数必须与请求条数一致");
        assert_eq!(
            results[0].as_ref().unwrap().text,
            "only",
            "已成功项必须保留"
        );
        for r in &results[1..] {
            let text = &r.as_ref().unwrap().text;
            assert!(
                text.starts_with("backup:"),
                "缺失项应降级到备用引擎，实际：{text}"
            );
        }
    }

    /// 主引擎推送过部分 chunk 后失败时，降级内容不得再追加推送，
    /// 否则界面会显示「残缺主译文 + 完整备用译文」的拼接结果。
    #[tokio::test]
    async fn test_fallback_stream_does_not_concat_after_partial_primary() {
        struct HalfwayThenFail;

        #[async_trait]
        impl TranslateEngine for HalfwayThenFail {
            fn id(&self) -> &str {
                "halfway"
            }
            fn name(&self) -> &str {
                "Halfway"
            }
            async fn translate(&self, _req: &TranslationRequest) -> Result<TranslationResponse> {
                unreachable!()
            }
            async fn translate_stream(
                &self,
                _req: &TranslationRequest,
                on_chunk: StreamCallback,
            ) -> Result<TranslationResponse> {
                on_chunk("PARTIAL-A ");
                on_chunk("PARTIAL-B");
                Err(Error::Engine {
                    engine: "halfway".to_string(),
                    message: "stream broke".to_string(),
                })
            }
        }

        let primary = Arc::new(HalfwayThenFail);
        let fallback = Arc::new(SuccessEngine("backup"));
        let engine = FallbackEngine::new(primary, fallback);

        let collected = Arc::new(std::sync::Mutex::new(String::new()));
        let sink = collected.clone();
        let res = engine
            .translate_stream(
                &TranslationRequest {
                    text: "hello".to_string(),
                    source_lang: "en".to_string(),
                    target_lang: "zh-CN".to_string(),
                },
                Box::new(move |chunk| {
                    sink.lock().unwrap().push_str(chunk);
                }),
            )
            .await
            .unwrap();

        assert_eq!(res.text, "backup:hello", "最终响应仍须是干净的备用译文");
        let streamed = collected.lock().unwrap().clone();
        assert_eq!(
            streamed, "PARTIAL-A PARTIAL-B",
            "主引擎已推送过内容后，降级译文必须静默返回而非追加推送"
        );
    }
}
