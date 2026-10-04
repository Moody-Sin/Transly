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
        // 如果文本超过 1200 字符，按段落/换行切片翻译并合并，防止 GET 请求 URL 超长溢出 (HTTP 414)
        if req.text.chars().count() > 1200 {
            let mut full_translated = String::new();
            let mut detected_lang = None;
            for paragraph in req.text.split("\n\n") {
                if paragraph.trim().is_empty() {
                    full_translated.push_str("\n\n");
                    continue;
                }
                let res = self
                    .translate_single_chunk(paragraph, &req.source_lang, &req.target_lang)
                    .await?;
                if detected_lang.is_none() {
                    detected_lang = res.detected_source_lang;
                }
                if !full_translated.is_empty() && !full_translated.ends_with("\n\n") {
                    full_translated.push_str("\n\n");
                }
                full_translated.push_str(&res.text);
            }
            return Ok(TranslationResponse {
                text: full_translated,
                detected_source_lang: detected_lang,
                engine: self.id().to_string(),
            });
        }

        self.translate_single_chunk(&req.text, &req.source_lang, &req.target_lang)
            .await
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
