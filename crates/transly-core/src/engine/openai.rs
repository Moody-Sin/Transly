use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{TranslateEngine, TranslationRequest, TranslationResponse};
use crate::{Error, Result};

pub struct OpenAIEngine {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

impl OpenAIEngine {
    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Self {
        let base = base_url
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
            .trim_end_matches('/')
            .to_string();

        let model = model.unwrap_or_else(|| "gpt-4o-mini".to_string());

        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_key: api_key.into(),
            base_url: base,
            model,
        }
    }

    /// 针对 DeepSeek 预设的便捷构造函数
    pub fn deepseek(api_key: impl Into<String>) -> Self {
        Self::new(
            api_key,
            Some("https://api.deepseek.com".to_string()),
            Some("deepseek-chat".to_string()),
        )
    }
}

#[async_trait]
impl TranslateEngine for OpenAIEngine {
    fn id(&self) -> &'static str {
        "openai_compatible"
    }

    fn name(&self) -> &'static str {
        "LLM 智能翻译"
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        if self.api_key.trim().is_empty() {
            return Err(Error::MissingCredentials("API Key 不能为空".into()));
        }

        let system_prompt = format!(
            "You are a professional, accurate translator. Translate the given text directly into target language '{tl}' without any explanations, pleasantries, or markdown decoration. Keep original line breaks and formatting identical.",
            tl = req.target_lang
        );

        let body = ChatCompletionRequest {
            model: &self.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: &req.text,
                },
            ],
            temperature: 0.2,
        };

        let url = format!("{}/chat/completions", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("LLM API 响应错误 ({status}): {err_body}"),
            });
        }

        let data: ChatCompletionResponse = resp.json().await?;
        if let Some(choice) = data.choices.into_iter().next() {
            if let Some(content) = choice.message.content {
                return Ok(TranslationResponse {
                    text: content.trim().to_string(),
                    detected_source_lang: None,
                    engine: self.id().to_string(),
                });
            }
        }

        Err(Error::Engine {
            engine: self.id().to_string(),
            message: "LLM API 返回内容为空".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openai_engine_initialization() {
        let engine = OpenAIEngine::new("test-key", None, None);
        assert_eq!(engine.id(), "openai_compatible");
        assert_eq!(engine.base_url, "https://api.openai.com/v1");
        assert_eq!(engine.model, "gpt-4o-mini");
    }

    #[test]
    fn test_deepseek_preset() {
        let ds = OpenAIEngine::deepseek("ds-key");
        assert_eq!(ds.base_url, "https://api.deepseek.com");
        assert_eq!(ds.model, "deepseek-chat");
    }
}
