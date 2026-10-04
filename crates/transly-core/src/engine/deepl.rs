use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{TranslateEngine, TranslationRequest, TranslationResponse};
use crate::{Error, Result};

pub struct DeepLEngine {
    client: Client,
    api_key: String,
    is_pro: bool,
}

#[derive(Serialize)]
struct DeepLApiRequest<'a> {
    text: &'a [&'a str],
    target_lang: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_lang: Option<&'a str>,
}

#[derive(Deserialize)]
struct DeepLApiTranslation {
    detected_source_language: Option<String>,
    text: String,
}

#[derive(Deserialize)]
struct DeepLApiResponse {
    translations: Vec<DeepLApiTranslation>,
}

impl DeepLEngine {
    pub fn new(api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        let is_pro = !key.ends_with(":fx");
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            api_key: key,
            is_pro,
        }
    }

    fn endpoint(&self) -> &'static str {
        if self.is_pro {
            "https://api.deepl.com/v2/translate"
        } else {
            "https://api-free.deepl.com/v2/translate"
        }
    }

    fn normalize_target_lang(lang: &str) -> &str {
        match lang.to_lowercase().as_str() {
            "zh" | "zh-cn" | "zh-hans" => "ZH",
            "en" | "en-us" => "EN-US",
            "en-gb" => "EN-GB",
            "ja" => "JA",
            "de" => "DE",
            "fr" => "FR",
            "es" => "ES",
            "ru" => "RU",
            "ko" => "KO",
            _ => lang,
        }
    }

    fn normalize_source_lang(lang: &str) -> Option<&str> {
        if lang.is_empty() || lang == "auto" {
            None
        } else {
            match lang.to_lowercase().as_str() {
                "zh" | "zh-cn" | "zh-hans" => Some("ZH"),
                "en" => Some("EN"),
                "ja" => Some("JA"),
                "de" => Some("DE"),
                "fr" => Some("FR"),
                "es" => Some("ES"),
                "ru" => Some("RU"),
                "ko" => Some("KO"),
                _ => Some(lang),
            }
        }
    }
}

#[async_trait]
impl TranslateEngine for DeepLEngine {
    fn id(&self) -> &'static str {
        "deepl"
    }

    fn name(&self) -> &'static str {
        "DeepL"
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        if self.api_key.trim().is_empty() {
            return Err(Error::MissingCredentials("DeepL API Key 不能为空".into()));
        }

        let target_lang = Self::normalize_target_lang(&req.target_lang);
        let source_lang = Self::normalize_source_lang(&req.source_lang);

        let body = DeepLApiRequest {
            text: &[&req.text],
            target_lang,
            source_lang,
        };

        let resp = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("DeepL API 错误 ({status}): {err_body}"),
            });
        }

        let data: DeepLApiResponse = resp.json().await?;
        if let Some(trans) = data.translations.into_iter().next() {
            Ok(TranslationResponse {
                text: trans.text,
                detected_source_lang: trans.detected_source_language,
                engine: self.id().to_string(),
            })
        } else {
            Err(Error::Engine {
                engine: self.id().to_string(),
                message: "DeepL 返回空翻译列表".to_string(),
            })
        }
    }

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        if self.api_key.trim().is_empty() {
            return Err(Error::MissingCredentials("DeepL API Key 不能为空".into()));
        }

        // 检查是否所有请求拥有相同的源语言与目标语言
        let first_target = &requests[0].target_lang;
        let first_source = &requests[0].source_lang;
        let is_homogeneous = requests
            .iter()
            .all(|r| &r.target_lang == first_target && &r.source_lang == first_source);

        if !is_homogeneous {
            // 若混合不同语言对，安全降级为逐项翻译
            let mut results = Vec::with_capacity(requests.len());
            for r in requests {
                results.push(self.translate(r).await?);
            }
            return Ok(results);
        }

        let target_lang = Self::normalize_target_lang(first_target);
        let source_lang = Self::normalize_source_lang(first_source);
        let texts: Vec<&str> = requests.iter().map(|r| r.text.as_str()).collect();

        let body = DeepLApiRequest {
            text: &texts,
            target_lang,
            source_lang,
        };

        let resp = self
            .client
            .post(self.endpoint())
            .header("Authorization", format!("DeepL-Auth-Key {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("DeepL API 批量错误 ({status}): {err_body}"),
            });
        }

        let data: DeepLApiResponse = resp.json().await?;
        let responses = data
            .translations
            .into_iter()
            .map(|t| TranslationResponse {
                text: t.text,
                detected_source_lang: t.detected_source_language,
                engine: self.id().to_string(),
            })
            .collect();

        Ok(responses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deepl_engine_endpoint() {
        let free_engine = DeepLEngine::new("test-key:fx");
        assert_eq!(
            free_engine.endpoint(),
            "https://api-free.deepl.com/v2/translate"
        );

        let pro_engine = DeepLEngine::new("test-key-pro");
        assert_eq!(pro_engine.endpoint(), "https://api.deepl.com/v2/translate");
    }

    #[test]
    fn test_deepl_lang_normalization() {
        assert_eq!(DeepLEngine::normalize_target_lang("zh-CN"), "ZH");
        assert_eq!(DeepLEngine::normalize_target_lang("en"), "EN-US");
        assert_eq!(DeepLEngine::normalize_source_lang("auto"), None);
        assert_eq!(DeepLEngine::normalize_source_lang("zh-cn"), Some("ZH"));
    }
}
