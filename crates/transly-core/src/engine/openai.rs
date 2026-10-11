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
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Academic => "academic",
            Self::Literary => "literary",
            Self::Business => "business",
        }
    }

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
    prefix_id: String,
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
    fn refresh_engine_id(&mut self) {
        self.engine_id = format!("{}:{}", self.prefix_id, self.style.as_str());
    }

    pub fn is_local_endpoint(url: &str) -> bool {
        let trimmed = url.trim();
        if trimmed.is_empty() {
            return false;
        }
        let to_parse = if !trimmed.contains("://") {
            format!("http://{trimmed}")
        } else {
            trimmed.to_string()
        };
        if let Ok(parsed) = reqwest::Url::parse(&to_parse) {
            if let Some(host) = parsed.host_str() {
                let clean_host = host.trim_start_matches('[').trim_end_matches(']');
                return matches!(clean_host, "localhost" | "127.0.0.1" | "::1");
            }
        }
        false
    }

    pub fn normalize_base_url(url: &str) -> String {
        let trimmed = url.trim().trim_end_matches('/');
        if let Some(stripped) = trimmed.strip_suffix("/chat/completions") {
            stripped.trim_end_matches('/').to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn new(
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: Option<String>,
    ) -> Self {
        Self::new_with_proxy(api_key, base_url, model, None, false)
    }

    pub fn new_with_proxy(
        api_key: impl Into<String>,
        base_url: Option<String>,
        model: Option<String>,
        proxy_url: Option<&str>,
        no_proxy: bool,
    ) -> Self {
        let base = base_url
            .map(|u| Self::normalize_base_url(&u))
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let model = model.unwrap_or_else(|| "gpt-4o-mini".to_string());
        let prefix_id = format!("openai:{model}");
        let style = TranslationStyle::General;
        let engine_id = format!("{prefix_id}:{}", style.as_str());

        Self {
            client: super::build_timed_client_with_proxy(
                Client::builder(),
                std::time::Duration::from_secs(30),
                proxy_url,
                no_proxy,
            ),
            api_key: api_key.into(),
            base_url: base,
            model,
            prefix_id,
            engine_id,
            style,
        }
    }

    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    pub fn set_style(&mut self, style: TranslationStyle) {
        self.style = style;
        self.refresh_engine_id();
    }

    pub fn with_style(mut self, style: TranslationStyle) -> Self {
        self.style = style;
        self.refresh_engine_id();
        self
    }

    pub fn style(&self) -> TranslationStyle {
        self.style
    }

    pub fn set_engine_id(&mut self, id: impl Into<String>) {
        self.prefix_id = id.into();
        self.refresh_engine_id();
    }

    /// 针对 DeepSeek 预设的便捷构造函数
    pub fn deepseek(api_key: impl Into<String>) -> Self {
        let mut engine = Self::new(
            api_key,
            Some("https://api.deepseek.com".to_string()),
            Some("deepseek-chat".to_string()),
        );
        engine.set_engine_id("deepseek:deepseek-chat");
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
            max_batch_size: crate::MAX_BATCH_ITEMS,
            supports_batch: true,
            requires_api_key: !Self::is_local_endpoint(&self.base_url),
            supports_streaming: true,
            fallback_engine: None,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        if self.api_key.trim().is_empty() && !Self::is_local_endpoint(&self.base_url) {
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
            let mut req_builder = self.client.post(&url).json(&body);
            let key = self.api_key.trim();
            if !key.is_empty() {
                req_builder = req_builder.header("Authorization", format!("Bearer {key}"));
            } else if Self::is_local_endpoint(&self.base_url) {
                req_builder = req_builder.header("Authorization", "Bearer ollama");
            }
            req_builder.send()
        })
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            // 上游错误体**不得**原样外传。
            //
            // OpenAI 兼容网关的 401 响应形如
            // `{"error":{"message":"Incorrect API key provided: sk-proj-XXXX..."}}`，
            // 即上游会回显密钥前缀。该字符串会一路进入扩展页面 DOM、Tauri 前端状态
            // 与任何记录 message 的日志，等于把密钥片段泄露到用户界面与日志里。
            // 因此只保留可诊断的状态码，响应体内容不落任何日志。
            eprintln!(
                "[Engine Error] {} 返回 {status}，响应体 {} 字节（内容不记录，避免密钥回显）",
                self.id(),
                err_body.len()
            );
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("LLM API 响应错误 ({status})"),
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
        if self.api_key.trim().is_empty() && !Self::is_local_endpoint(&self.base_url) {
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
            let mut req_builder = self.client.post(&url).json(&body);
            let key = self.api_key.trim();
            if !key.is_empty() {
                req_builder = req_builder.header("Authorization", format!("Bearer {key}"));
            } else if Self::is_local_endpoint(&self.base_url) {
                req_builder = req_builder.header("Authorization", "Bearer ollama");
            }
            req_builder.send()
        })
        .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            eprintln!(
                "[Engine Error] {} 返回 {status}，响应体 {} 字节（内容不记录，避免密钥回显）",
                self.id(),
                err_body.len()
            );
            return Err(Error::Engine {
                engine: self.id().to_string(),
                message: format!("LLM API 响应错误 ({status})"),
            });
        }

        use futures::StreamExt;
        let mut stream = resp.bytes_stream();
        let mut full_text = String::new();
        let mut byte_buffer = Vec::new();

        'stream_loop: while let Some(item) = stream.next().await {
            let bytes = item?;
            byte_buffer.extend_from_slice(&bytes);

            // 逐行解析 SSE 数据帧，保持 O(n) 的单趟扫描与单次内存缩容，杜绝每行重复 drain
            let mut cursor = 0;
            while let Some(relative_nl) = byte_buffer[cursor..].iter().position(|&b| b == b'\n') {
                let newline_pos = cursor + relative_nl;
                let line_bytes = &byte_buffer[cursor..newline_pos];
                cursor = newline_pos + 1;

                // 移除 CRLF 尾部 \r
                let line_trimmed = if line_bytes.ends_with(b"\r") {
                    &line_bytes[..line_bytes.len() - 1]
                } else {
                    line_bytes
                };

                if line_trimmed.is_empty() {
                    continue;
                }

                // 安全 UTF-8 解析：由于单行以 \n 严格切分，完整的 SSE 文本行必定自包含 UTF-8 字符边界
                let line = match std::str::from_utf8(line_trimmed) {
                    Ok(s) => s.trim(),
                    Err(_) => continue,
                };

                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        break 'stream_loop;
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

            // 单趟结束后一次性移走已消费的字节，消除 O(n^2) 搬移
            if cursor > 0 {
                byte_buffer.drain(..cursor);
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

    async fn translate_batch_tolerant(
        &self,
        requests: &[TranslationRequest],
    ) -> Vec<Result<TranslationResponse>> {
        if requests.is_empty() {
            return Vec::new();
        }
        if requests.len() == 1 {
            return vec![self.translate(&requests[0]).await];
        }

        use futures::stream::{self, StreamExt};
        let concurrency = 5;
        let futures: Vec<_> = requests
            .iter()
            .enumerate()
            .map(|(idx, req)| async move {
                let res = self.translate(req).await;
                (idx, res)
            })
            .collect();

        let stream = stream::iter(futures).buffer_unordered(concurrency);

        let mut results_with_idx = stream.collect::<Vec<_>>().await;
        results_with_idx.sort_by_key(|(idx, _)| *idx);
        results_with_idx.into_iter().map(|(_, res)| res).collect()
    }

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        let tolerant = self.translate_batch_tolerant(requests).await;
        tolerant.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_base_url_strips_chat_completions() {
        assert_eq!(
            OpenAIEngine::normalize_base_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            OpenAIEngine::normalize_base_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            OpenAIEngine::normalize_base_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1"
        );
        assert_eq!(
            OpenAIEngine::normalize_base_url("https://api.openai.com/v1/chat/completions/"),
            "https://api.openai.com/v1"
        );
    }

    #[test]
    fn test_openai_engine_initialization() {
        let engine = OpenAIEngine::new("test-key", None, None);
        assert_eq!(engine.id(), "openai:gpt-4o-mini:general");
        assert_eq!(engine.base_url, "https://api.openai.com/v1");
        assert_eq!(engine.model, "gpt-4o-mini");
        assert!(engine.capabilities().supports_streaming);
        assert!(engine.capabilities().supports_batch);
    }

    #[test]
    fn test_deepseek_preset() {
        let ds = OpenAIEngine::deepseek("ds-key");
        assert_eq!(ds.base_url, "https://api.deepseek.com");
        assert_eq!(ds.model, "deepseek-chat");
        assert_eq!(ds.id(), "deepseek:deepseek-chat:general");
    }

    #[test]
    fn test_style_changes_engine_id_for_cache_isolation() {
        let engine = OpenAIEngine::new("key", None, None);
        assert_eq!(engine.id(), "openai:gpt-4o-mini:general");

        let academic = engine.with_style(TranslationStyle::Academic);
        assert_eq!(academic.id(), "openai:gpt-4o-mini:academic");

        let literary = academic.with_style(TranslationStyle::Literary);
        assert_eq!(literary.id(), "openai:gpt-4o-mini:literary");

        let business = literary.with_style(TranslationStyle::Business);
        assert_eq!(business.id(), "openai:gpt-4o-mini:business");

        // 验证配合 TranslationCache 时不同风格产生不同的缓存键并隔离命中
        let cache = crate::TranslationCache::in_memory().unwrap();
        let general_engine = OpenAIEngine::new("key", None, None);
        let academic_engine =
            OpenAIEngine::new("key", None, None).with_style(TranslationStyle::Academic);

        cache
            .set("hello", "你好（通用）", "en", "zh-CN", general_engine.id())
            .unwrap();

        // 学术风格查询同一词句不应命中通用缓存
        let hit_academic =
            crate::lookup_cache_with_fallback(&cache, &academic_engine, "hello", "en", "zh-CN");
        assert_eq!(hit_academic, None, "学术风格不应误命中通用风格缓存");

        // 写入学术缓存后，各自能独立命中
        cache
            .set("hello", "您好（学术）", "en", "zh-CN", academic_engine.id())
            .unwrap();
        let hit_academic2 =
            crate::lookup_cache_with_fallback(&cache, &academic_engine, "hello", "en", "zh-CN");
        assert_eq!(
            hit_academic2,
            Some(("您好（学术）".to_string(), academic_engine.id().to_string()))
        );

        let hit_general =
            crate::lookup_cache_with_fallback(&cache, &general_engine, "hello", "en", "zh-CN");
        assert_eq!(
            hit_general,
            Some(("你好（通用）".to_string(), general_engine.id().to_string()))
        );
    }

    #[test]
    fn test_is_local_endpoint_and_credentials_requirement() {
        assert!(OpenAIEngine::is_local_endpoint("http://localhost:11434/v1"));
        assert!(OpenAIEngine::is_local_endpoint("http://127.0.0.1:11434/v1"));
        assert!(OpenAIEngine::is_local_endpoint("http://[::1]:11434/v1"));
        // 伪装与恶意子域名必须严格被识别为非本地端点
        assert!(!OpenAIEngine::is_local_endpoint(
            "http://localhost.evil.com:11434"
        ));
        assert!(!OpenAIEngine::is_local_endpoint(
            "http://evil.com/localhost"
        ));
        assert!(!OpenAIEngine::is_local_endpoint(
            "http://127.0.0.1.evil.com"
        ));
        assert!(!OpenAIEngine::is_local_endpoint(
            "https://api.openai.com/v1"
        ));
        assert!(!OpenAIEngine::is_local_endpoint("https://api.deepseek.com"));

        let remote = OpenAIEngine::new("key", Some("https://api.openai.com/v1".to_string()), None);
        assert!(remote.capabilities().requires_api_key);

        let local_ollama = OpenAIEngine::new(
            "",
            Some("http://localhost:11434/v1".to_string()),
            Some("qwen2.5:7b".to_string()),
        );
        assert!(!local_ollama.capabilities().requires_api_key);
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
