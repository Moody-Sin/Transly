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
}

impl Default for GoogleWebEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TranslateEngine for GoogleWebEngine {
    fn id(&self) -> &'static str {
        "google_web"
    }

    fn name(&self) -> &'static str {
        "Google 网页翻译"
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        let sl = if req.source_lang.is_empty() {
            "auto"
        } else {
            &req.source_lang
        };
        let tl = &req.target_lang;

        let url = format!(
            "https://translate.googleapis.com/translate_a/single?client=gtx&sl={sl}&tl={tl}&dt=t&q={}",
            urlencoding::encode(&req.text)
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("HTTP 请求失败状态码: {}", resp.status()),
            });
        }

        let json: Value = resp.json().await?;

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
                engine: self.id().to_string(),
                message: "解析响应文本为空".to_string(),
            });
        }

        // 检测到的源语言通常位于 json.get(2)
        let detected = json.get(2).and_then(|v| v.as_str()).map(|s| s.to_string());

        Ok(TranslationResponse {
            text: translated_full,
            detected_source_lang: detected,
            engine: self.id().to_string(),
        })
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
    }
}
