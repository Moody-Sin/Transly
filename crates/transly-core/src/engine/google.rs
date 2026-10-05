use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;

use super::{TranslateEngine, TranslationRequest, TranslationResponse};
use crate::{Error, Result};

pub struct GoogleWebEngine {
    client: Client,
}

impl GoogleWebEngine {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }
    pub fn parse_gtx_response(json: &Value, engine_id: &str) -> Result<TranslationResponse> {
        // Google GTX 响应结构示例: [[["译文1", "原文1", ...], ["译文2", "原文2", ...]], ...]
        let mut translated_full = String::new();
        if let Some(sentences) = json.get(0).and_then(|v| v.as_array()) {
            for s in sentences {
                if let Some(part) = s.get(0).and_then(|v| v.as_str()) {
                    translated_full.push_str(part);
                }
            }
        }

        if translated_full.is_empty() {
            return Err(Error::Engine {
                engine: engine_id.to_string(),
                message: "解析响应文本为空".to_string(),
            });
        }

        // 检测到的源语言通常位于 json.get(2)
        let detected = json.get(2).and_then(|v| v.as_str()).map(|s| s.to_string());

        Ok(TranslationResponse {
            text: translated_full,
            detected_source_lang: detected,
            engine: engine_id.to_string(),
        })
    }

    async fn translate_single_chunk(
        &self,
        text: &str,
        source_lang: &str,
        target_lang: &str,
    ) -> Result<TranslationResponse> {
        let sl = if source_lang.is_empty() {
            "auto"
        } else {
            source_lang
        };
        let tl = target_lang;

        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl={sl}&tl={tl}&dt=t&q={}",
            urlencoding::encode(text)
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("HTTP 请求失败状态码: {}", resp.status()),
            });
        }

        let json: Value = resp.json().await?;
        Self::parse_gtx_response(&json, self.id())
    }
}

impl Default for GoogleWebEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TranslateEngine for GoogleWebEngine {
    fn id(&self) -> &str {
        "google_web"
    }

    fn name(&self) -> &str {
        "Google 网页翻译"
    }

    fn capabilities(&self) -> super::EngineCapabilities {
        super::EngineCapabilities {
            id: self.id().to_string(),
            name: self.name().to_string(),
            max_batch_size: 50,
            supports_streaming: false,
            supports_batch: true,
            requires_api_key: false,
            fallback_engine: None,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        // 如果文本超过 1000 字符，按段落和固定长度双重切片，彻底防止超长单行溢出
        if req.text.chars().count() > 1000 {
            let mut full_translated = String::new();
            let mut detected_lang = None;

            for paragraph in req.text.split('\n') {
                if paragraph.trim().is_empty() {
                    full_translated.push('\n');
                    continue;
                }

                // 若单段仍超出 800 字符，按句/固定窗口切块
                let chars: Vec<char> = paragraph.chars().collect();
                for chunk in chars.chunks(800) {
                    let chunk_str: String = chunk.iter().collect();
                    let res = self
                        .translate_single_chunk(&chunk_str, &req.source_lang, &req.target_lang)
                        .await?;
                    if detected_lang.is_none() {
                        detected_lang = res.detected_source_lang;
                    }
                    full_translated.push_str(&res.text);
                    if !res.text.ends_with(' ') {
                        full_translated.push(' ');
                    }
                }
                full_translated.push('\n');
            }

            return Ok(TranslationResponse {
                text: full_translated.trim().to_string(),
                detected_source_lang: detected_lang,
                engine: self.id().to_string(),
            });
        }

        self.translate_single_chunk(&req.text, &req.source_lang, &req.target_lang)
            .await
    }

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        if requests.len() == 1 {
            return Ok(vec![self.translate(&requests[0]).await?]);
        }

        // 使用受控并发（并发度 4）并发执行单项翻译，避免串行阻塞过慢
        let chunk_size = 4;
        let mut all_results = Vec::with_capacity(requests.len());

        for chunk in requests.chunks(chunk_size) {
            let futures: Vec<_> = chunk.iter().map(|req| self.translate(req)).collect();
            let chunk_results = futures::future::join_all(futures).await;
            for res in chunk_results {
                all_results.push(res?);
            }
        }

        Ok(all_results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_info() {
        let engine = GoogleWebEngine::new();
        assert_eq!(engine.id(), "google_web");
        assert_eq!(engine.name(), "Google 网页翻译");
        let caps = engine.capabilities();
        assert_eq!(caps.max_batch_size, 50);
        assert!(caps.supports_batch);
        assert!(!caps.requires_api_key);
    }

    #[tokio::test]
    async fn test_translate_batch_empty() {
        let engine = GoogleWebEngine::new();
        let res = engine.translate_batch(&[]).await.unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn test_parse_gtx_response_success() {
        let sample: Value = serde_json::json!([
            [
                ["你好", "hello", null, null, 1],
                ["，世界", " world", null, null, 1]
            ],
            null,
            "en"
        ]);

        let res = GoogleWebEngine::parse_gtx_response(&sample, "google_web").unwrap();
        assert_eq!(res.text, "你好，世界");
        assert_eq!(res.detected_source_lang, Some("en".to_string()));
        assert_eq!(res.engine, "google_web");
    }

    #[test]
    fn test_parse_gtx_response_empty_or_malformed() {
        let empty_sentences: Value = serde_json::json!([[], null, "en"]);
        let err = GoogleWebEngine::parse_gtx_response(&empty_sentences, "google_web").unwrap_err();
        assert!(err.to_string().contains("解析响应文本为空"));

        let null_root: Value = serde_json::json!(null);
        let err2 = GoogleWebEngine::parse_gtx_response(&null_root, "google_web").unwrap_err();
        assert!(err2.to_string().contains("解析响应文本为空"));
    }
}
