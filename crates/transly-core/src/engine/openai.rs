use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use super::{TranslateEngine, TranslationRequest, TranslationResponse};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranslationStyle {
    #[default]
    General,
    Academic,
    Literary,
    Business,
}

impl TranslationStyle {
    pub fn prompt_modifier(&self) -> &'static str {
        match self {
            Self::General => "Translate the given text directly and accurately without any explanations.",
            Self::Academic => "Adopt a formal, academic tone. Preserve technical terminology, mathematical notation, and precise phrasing rigorously without any explanations.",
            Self::Literary => "Adopt an expressive, natural, and idiomatic literary style. Preserve nuances, flow, and cultural context while maintaining fidelity without any explanations.",
            Self::Business => "Adopt a professional, polite, and executive business tone suitable for formal workplace correspondence without any explanations.",
        }
    }
}

pub struct OpenAIEngine {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    engine_id: String,
    style: TranslationStyle,
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
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
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

#[derive(Deserialize)]
struct StreamChatChoice {
    delta: StreamDelta,
}

#[derive(Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChatCompletionResponse {
    choices: Vec<StreamChatChoice>,
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
        let engine_id = format!("openai:{model}");

        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            api_key: api_key.into(),
            base_url: base,
            model,
            engine_id,
            style: TranslationStyle::General,
        }
    }

    pub fn set_style(&mut self, style: TranslationStyle) {
        self.style = style;
    }

    pub fn with_style(mut self, style: TranslationStyle) -> Self {
        self.style = style;
        self
    }

    pub fn style(&self) -> TranslationStyle {
        self.style
    }

    pub fn set_engine_id(&mut self, id: impl Into<String>) {
        self.engine_id = id.into();
    }

    /// 针对 DeepSeek 预设的便捷构造函数
    pub fn deepseek(api_key: impl Into<String>) -> Self {
        let mut engine = Self::new(
            api_key,
            Some("https://api.deepseek.com".to_string()),
            Some("deepseek-chat".to_string()),
        );
        engine.engine_id = "deepseek:deepseek-chat".to_string();
        engine
    }

    pub fn build_system_prompt(&self, target_lang: &str) -> String {
        format!(
            "You are a professional translator. Direct target language: '{target_lang}'. {modifier} Do not explain, greet, or wrap in markdown fences. Keep original line breaks and formatting identical.",
            modifier = self.style.prompt_modifier()
        )
    }
}

#[async_trait]
impl TranslateEngine for OpenAIEngine {
    fn id(&self) -> &str {
        &self.engine_id
    }

    fn name(&self) -> &'static str {
        "LLM 智能翻译"
    }

    fn capabilities(&self) -> super::EngineCapabilities {
        super::EngineCapabilities {
            id: self.id().to_string(),
            name: self.name().to_string(),
            max_batch_size: 1,
            supports_batch: false,
            requires_api_key: true,
            supports_streaming: true,
            fallback_engine: None,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        if self.api_key.trim().is_empty() {
            return Err(Error::MissingCredentials("API Key 不能为空".into()));
        }

        let system_prompt = self.build_system_prompt(&req.target_lang);

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
            temperature: match self.style {
                TranslationStyle::Academic => 0.1,
                TranslationStyle::General | TranslationStyle::Business => 0.2,
                TranslationStyle::Literary => 0.4,
            },
            stream: false,
        };

        let url = format!("{}/chat/completions", self.base_url);
        let resp = super::send_with_retry(1, std::time::Duration::from_millis(300), || {
            self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&body)
                .send()
        })
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

    async fn translate_stream(
        &self,
        req: &TranslationRequest,
        on_chunk: super::StreamCallback,
    ) -> Result<TranslationResponse> {
        if self.api_key.trim().is_empty() {
            return Err(Error::MissingCredentials("API Key 不能为空".into()));
        }

        let system_prompt = self.build_system_prompt(&req.target_lang);

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
            temperature: match self.style {
                TranslationStyle::Academic => 0.1,
                TranslationStyle::General | TranslationStyle::Business => 0.2,
                TranslationStyle::Literary => 0.4,
            },
            stream: true,
        };

        let url = format!("{}/chat/completions", self.base_url);
        let resp = super::send_with_retry(1, std::time::Duration::from_millis(300), || {
            self.client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.api_key))
                .json(&body)
                .send()
        })
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("LLM API 响应错误 ({status}): {err_body}"),
            });
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut full_text = String::new();
        let mut buffer = String::new();

        while let Some(item) = stream.next().await {
            let bytes = item?;
            let chunk_str = String::from_utf8_lossy(&bytes);
            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer.drain(..=newline_pos);

                if line.is_empty() {
                    continue;
                }

                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(parsed) = serde_json::from_str::<StreamChatCompletionResponse>(data) {
                        for choice in parsed.choices {
                            if let Some(content) = choice.delta.content {
                                if !content.is_empty() {
                                    on_chunk(&content);
                                    full_text.push_str(&content);
                                }
                            }
                        }
                    }
                }
            }
        }

        if full_text.trim().is_empty() {
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: "LLM API 流式返回内容为空".to_string(),
            });
        }

        Ok(TranslationResponse {
            text: full_text.trim().to_string(),
            detected_source_lang: None,
            engine: self.id().to_string(),
        })
    }

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        // 当请求项较少（<= 1 条）直接单次处理
        if requests.len() == 1 {
            return Ok(vec![self.translate(&requests[0]).await?]);
        }

        // 使用受控并发（并发度 4）并发执行单项翻译，避免串行循环过度耗时与整体超时
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
    fn test_openai_engine_initialization() {
        let engine = OpenAIEngine::new("test-key", None, None);
        assert_eq!(engine.id(), "openai:gpt-4o-mini");
        assert_eq!(engine.base_url, "https://api.openai.com/v1");
        assert_eq!(engine.model, "gpt-4o-mini");
        assert!(engine.capabilities().supports_streaming);
    }

    #[test]
    fn test_deepseek_preset() {
        let ds = OpenAIEngine::deepseek("ds-key");
        assert_eq!(ds.base_url, "https://api.deepseek.com");
        assert_eq!(ds.model, "deepseek-chat");
        assert_eq!(ds.id(), "deepseek:deepseek-chat");
    }

    #[test]
    fn test_translation_style_prompts() {
        let engine = OpenAIEngine::new("key", None, None);
        assert_eq!(engine.style(), TranslationStyle::General);

        let general_prompt = engine.build_system_prompt("zh-CN");
        assert!(general_prompt.contains("Translate the given text directly and accurately"));

        let academic = engine.with_style(TranslationStyle::Academic);
        let academic_prompt = academic.build_system_prompt("zh-CN");
        assert!(academic_prompt.contains("formal, academic tone"));

        let literary = academic.with_style(TranslationStyle::Literary);
        let literary_prompt = literary.build_system_prompt("en");
        assert!(literary_prompt.contains("expressive, natural, and idiomatic"));

        let business = literary.with_style(TranslationStyle::Business);
        let business_prompt = business.build_system_prompt("ja");
        assert!(business_prompt.contains("professional, polite, and executive business tone"));
    }
}
