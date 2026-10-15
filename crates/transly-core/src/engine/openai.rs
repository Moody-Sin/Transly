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

#[derive(Serialize, Deserialize)]
struct BatchJsonItem {
    id: usize,
    text: String,
}

pub(crate) fn extract_json_payload(raw: &str) -> &str {
    let trimmed = raw.trim();
    // 1. 若首尾直接为代码围栏
    if let Some(rest) = trimmed.strip_prefix("```json") {
        if let Some(inner) = rest.strip_suffix("```") {
            return inner.trim();
        }
    }
    if let Some(rest) = trimmed.strip_prefix("```") {
        if let Some(inner) = rest.strip_suffix("```") {
            return inner.trim();
        }
    }
    // 2. 检查文本内部是否包含代码围栏
    if let Some(start) = trimmed.find("```json") {
        let after = &trimmed[start + 7..];
        if let Some(end) = after.rfind("```") {
            return after[..end].trim();
        }
    }
    if let Some(start) = trimmed.find("```") {
        let after = &trimmed[start + 3..];
        if let Some(end) = after.rfind("```") {
            return after[..end].trim();
        }
    }
    // 3. 容错：查找最外层 JSON 数组 '[' 到 ']' 避免前置或后置说明文案干扰解析
    if let (Some(first_bracket), Some(last_bracket)) = (trimmed.find('['), trimmed.rfind(']')) {
        if first_bracket <= last_bracket {
            return trimmed[first_bracket..=last_bracket].trim();
        }
    }
    trimmed
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
    #[serde(default)]
    reasoning_content: Option<String>,
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
    #[serde(default)]
    reasoning_content: Option<String>,
}

#[derive(Deserialize)]
struct StreamChatCompletionResponse {
    choices: Vec<StreamChatChoice>,
}

/// 聚合批量失败的**性质**，决定是否可以安全降级为逐项翻译。
///
/// 区分二者是必须的：若聚合请求因鉴权失败、配额耗尽或限流（401/402/403/429）而失败，
/// 降级为逐项并发只会把 1 次请求放大成最多 30 次（每项内部还各自重试一次），
/// 在 429 场景下更会进一步恶化限流、招致服务商封禁 API Key 或 IP。
/// 只有内容层的格式异常（模型没按 JSON 输出）才值得换逐项方式重试。
enum AggregateFailure {
    /// 致命错误：重试逐项无意义且有害，必须立即向整批返回错误。
    Fatal(Error),
    /// 内容层格式错误：模型输出不符合约定，可降级为逐项翻译隔离故障项。
    Format,
}

/// 网络层错误（`send_with_retry` 已退避重试过一次）与序列化错误一律按致命处理：
/// 再拆成 30 项并发重发只会放大请求量，且必然同样失败。
impl From<reqwest::Error> for AggregateFailure {
    fn from(e: reqwest::Error) -> Self {
        AggregateFailure::Fatal(Error::Network(e))
    }
}

impl From<serde_json::Error> for AggregateFailure {
    fn from(e: serde_json::Error) -> Self {
        AggregateFailure::Fatal(Error::Serialization(e))
    }
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

    pub fn build_batch_system_prompt(&self, target_lang: &str) -> String {
        format!(
            "You are a professional translator. Direct target language: '{target_lang}'. {modifier} You will receive a JSON array of items: [{{\"id\": <number>, \"text\": \"<text>\"}}]. Translate the 'text' of each item and respond strictly with a valid JSON array of objects with 'id' and 'text'. Keep formatting and line breaks identical. Output ONLY valid JSON array with NO explanation, NO greeting, and NO markdown code fences.",
            modifier = self.style.prompt_modifier()
        )
    }

    async fn try_translate_batch_aggregated(
        &self,
        requests: &[TranslationRequest],
    ) -> std::result::Result<Vec<TranslationResponse>, AggregateFailure> {
        if self.api_key.trim().is_empty() && !Self::is_local_endpoint(&self.base_url) {
            return Err(AggregateFailure::Fatal(Error::MissingCredentials(
                "API Key 不能为空".into(),
            )));
        }

        let first_target = &requests[0].target_lang;
        let is_homogeneous = requests.iter().all(|r| &r.target_lang == first_target);
        if !is_homogeneous {
            return Err(AggregateFailure::Fatal(Error::Input(
                "批量请求目标语言不一致".into(),
            )));
        }

        let items: Vec<BatchJsonItem> = requests
            .iter()
            .enumerate()
            .map(|(idx, req)| BatchJsonItem {
                id: idx,
                text: req.text.clone(),
            })
            .collect();

        let json_input = serde_json::to_string(&items).map_err(|e| {
            AggregateFailure::Fatal(Error::Internal(format!("序列化批量输入失败: {e}")))
        })?;

        let system_prompt = self.build_batch_system_prompt(first_target);

        let body = ChatCompletionRequest {
            model: &self.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: &json_input,
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
            return Err(AggregateFailure::Fatal(Error::Engine {
                engine: self.id().to_string(),
                message: format!("LLM API 聚合批量响应错误 ({status})"),
            }));
        }

        let data: ChatCompletionResponse = resp.json().await?;
        let content = data
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or(AggregateFailure::Format)?;

        let cleaned = extract_json_payload(&content);
        let parsed: Vec<BatchJsonItem> =
            serde_json::from_str(cleaned).map_err(|_| AggregateFailure::Format)?;

        if parsed.len() != requests.len() {
            return Err(AggregateFailure::Format);
        }

        let mut item_map: std::collections::HashMap<usize, String> =
            std::collections::HashMap::with_capacity(parsed.len());
        for item in parsed {
            item_map.insert(item.id, item.text);
        }

        let mut ordered_responses = Vec::with_capacity(requests.len());
        for idx in 0..requests.len() {
            if let Some(text) = item_map.remove(&idx) {
                ordered_responses.push(TranslationResponse {
                    text,
                    detected_source_lang: None,
                    engine: self.id().to_string(),
                });
            } else {
                return Err(AggregateFailure::Format);
            }
        }

        Ok(ordered_responses)
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
                if !content.trim().is_empty() {
                    return Ok(TranslationResponse {
                        text: content.trim().to_string(),
                        detected_source_lang: None,
                        engine: self.id().to_string(),
                    });
                }
            }
            // 容错兜底：若接入推理模型（如 DeepSeek-R1 / o1）且正式 content 为空，回退消费 reasoning_content
            if let Some(reasoning) = choice.message.reasoning_content {
                if !reasoning.trim().is_empty() {
                    return Ok(TranslationResponse {
                        text: reasoning.trim().to_string(),
                        detected_source_lang: None,
                        engine: self.id().to_string(),
                    });
                }
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
        let mut reasoning_text = String::new();
        let mut byte_buffer = Vec::new();

        'stream_loop: while let Some(item) = stream.next().await {
            let bytes = item?;
            byte_buffer.extend_from_slice(&bytes);

            // 逐行解析 SSE 数据帧，保持 O(n) 的单趟扫描与单次内存缩容，杜绝每行重复 drain。
            // 末尾未闭合的残余字节保留在 byte_buffer 中，留待下个 TCP 报文/分包抵达后拼成完整行。
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
                            } else if let Some(reasoning) = choice.delta.reasoning_content {
                                if !reasoning.is_empty() {
                                    reasoning_text.push_str(&reasoning);
                                }
                            }
                        }
                    }
                }
            }

            // 单趟结束后一次性移走已消费的完整行字节，保留末尾未闭合的残余分包字节
            if cursor > 0 {
                byte_buffer.drain(..cursor);
            }
        }

        // 处理流结束时若存在未带末尾换行的最后一行残余数据帧
        if !byte_buffer.is_empty() {
            let line_trimmed = if byte_buffer.ends_with(b"\r") {
                &byte_buffer[..byte_buffer.len() - 1]
            } else {
                &byte_buffer[..]
            };
            if let Ok(line) = std::str::from_utf8(line_trimmed) {
                let line = line.trim();
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data != "[DONE]" {
                        if let Ok(parsed) =
                            serde_json::from_str::<StreamChatCompletionResponse>(data)
                        {
                            for choice in parsed.choices {
                                if let Some(content) = choice.delta.content {
                                    if !content.is_empty() {
                                        on_chunk(&content);
                                        full_text.push_str(&content);
                                    }
                                } else if let Some(reasoning) = choice.delta.reasoning_content {
                                    if !reasoning.is_empty() {
                                        reasoning_text.push_str(&reasoning);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            byte_buffer.clear();
        }

        if full_text.trim().is_empty() {
            if !reasoning_text.trim().is_empty() {
                let fallback = reasoning_text.trim().to_string();
                on_chunk(&fallback);
                return Ok(TranslationResponse {
                    text: fallback,
                    detected_source_lang: None,
                    engine: self.id().to_string(),
                });
            }
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

        // 优先尝试单次聚合批量 Prompting 优化（消除 90%+ 网络往返与 Token 浪费）。
        //
        // 失败时**不能**无差别回退逐项：聚合若因鉴权失效、额度耗尽或限流失败，
        // 拆成 30 项并发重发会把 1 次请求放大成最多 61 次（每项内部还各自退避重试一次），
        // 在 429 场景下更会加剧限流、招致服务商封禁 API Key 或 IP。
        // 因此只有内容层格式异常才降级，致命错误直接向整批返回。
        let aggregate = self.try_translate_batch_aggregated(requests).await;
        match aggregate {
            Ok(aggregated) => return aggregated.into_iter().map(Ok).collect(),
            Err(AggregateFailure::Fatal(e)) => {
                // 整批同一原因失败：逐项重试只会放大请求量且必然同样失败。
                // 错误不可 Clone，故逐项构造同一语义的错误（保留原始 message 供 UI 展示）。
                let message = e.to_string();
                return (0..requests.len())
                    .map(|_| {
                        Err(Error::Engine {
                            engine: self.id().to_string(),
                            message: message.clone(),
                        })
                    })
                    .collect();
            }
            Err(AggregateFailure::Format) => {
                // 模型没按约定输出 JSON：降级为逐项翻译，隔离出真正有问题的项
            }
        }

        // 容错降级：逐项并发隔离翻译
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

    /// 回归：致命错误（401/429 等）绝不可降级为逐项并发翻译。
    ///
    /// 复现的缺陷：旧实现在聚合失败时无差别回退逐项并发（并发度 5，每项内部还各自
    /// 退避重试一次），把 1 次请求放大为最多 30 项 × 2 = 61 次 HTTP 请求。
    /// 若失败原因是 429 限流，进一步倾泻请求会加剧限流，可能导致 API Key 或 IP 被封禁。
    ///
    /// 判定必须**统计真实 HTTP 请求数**：仅断言「结果都是 Err」无法区分两条路径
    /// （无 Key 等场景下逐项翻译同样不会发请求），那样的测试对缺陷毫无拦截力。
    /// 这里启动一个返回 429 的本地桩服务，直接数它被调用了多少次。
    #[tokio::test]
    async fn test_fatal_aggregate_failure_does_not_fan_out_to_per_item_requests() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let hits = Arc::new(AtomicUsize::new(0));
        let hits_srv = Arc::clone(&hits);

        // 极简 HTTP 桩：任何请求一律回 429（可重试的限流状态），并计数
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("应能绑定本地回环端口");
        let addr = listener.local_addr().expect("应能取得本地地址");

        tokio::spawn(async move {
            loop {
                let Ok((mut sock, _)) = listener.accept().await else {
                    break;
                };
                let hits = Arc::clone(&hits_srv);
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 4096];
                    // 读掉请求（不解析内容，只需确保客户端已发完）
                    let _ = sock.read(&mut buf).await;
                    hits.fetch_add(1, Ordering::SeqCst);
                    let body = r#"{"error":{"message":"rate limited"}}"#;
                    let resp = format!(
                        "HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.flush().await;
                });
            }
        });

        // 注意：base_url 指向本地桩，故 is_local_endpoint 为真 → 不要求 API Key，
        // 请求会真实发出并拿到 429。
        let engine = OpenAIEngine::new(
            "",
            Some(format!("http://127.0.0.1:{}", addr.port())),
            Some("stub-model".to_string()),
        );

        let requests: Vec<TranslationRequest> = (0..10)
            .map(|i| TranslationRequest {
                text: format!("text {i}"),
                source_lang: "en".to_string(),
                target_lang: "zh-CN".to_string(),
            })
            .collect();

        let results = engine.translate_batch_tolerant(&requests).await;

        // 结果条数必须与请求一一对应
        assert_eq!(results.len(), 10, "必须为每项返回结果");
        assert!(
            results.iter().all(|r| r.is_err()),
            "429 属致命错误，整批都应失败"
        );

        // 关键断言：429 是致命错误，**不得**降级为逐项并发。
        // 旧实现会产生 1 次聚合 + 10 项 × 2 次（含重试）= 21 次请求。
        let total = hits.load(Ordering::SeqCst);
        assert!(
            total <= 2,
            "429 致命错误下不得降级为逐项翻译（旧实现会放大到 21 次），实际请求 {total} 次"
        );
    }

    #[tokio::test]
    async fn test_extract_json_payload() {
        let raw = r#"[{"id":0,"text":"你好"}]"#;
        assert_eq!(extract_json_payload(raw), raw);

        let wrapped = format!("```json\n{raw}\n```");
        assert_eq!(extract_json_payload(&wrapped), raw);

        let wrapped_no_lang = format!("```\n{raw}\n```");
        assert_eq!(extract_json_payload(&wrapped_no_lang), raw);

        let chatty = format!("Here is the translation:\n```json\n{raw}\n```\nHope this helps!");
        assert_eq!(extract_json_payload(&chatty), raw);

        let bare_chatty = format!("Here is the translation:\n{raw}\nEnjoy!");
        assert_eq!(extract_json_payload(&bare_chatty), raw);
    }

    #[test]
    fn test_build_batch_system_prompt() {
        let engine = OpenAIEngine::new("key", None, None);
        let prompt = engine.build_batch_system_prompt("zh-CN");
        assert!(prompt.contains("JSON array of items"));
        assert!(prompt.contains("'id' and 'text'"));
    }

    #[test]
    fn test_stream_sse_fragment_reassembly() {
        // 模拟 TCP 分包将完整的 data 行在 JSON 字符甚至前缀中间拆开到达
        let chunks = vec![
            "data: {\"choices\":[{\"delta\":{\"c".as_bytes(),
            "ontent\":\"Hello \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"World\"}}]}"
                .as_bytes(),
            "\n\ndata: [DONE]\n\n".as_bytes(),
        ];

        let mut byte_buffer = Vec::new();
        let mut full_text = String::new();

        'stream_loop: for bytes in chunks {
            byte_buffer.extend_from_slice(bytes);
            let mut cursor = 0;
            while let Some(relative_nl) = byte_buffer[cursor..].iter().position(|&b| b == b'\n') {
                let newline_pos = cursor + relative_nl;
                let line_bytes = &byte_buffer[cursor..newline_pos];
                cursor = newline_pos + 1;

                let line_trimmed = if line_bytes.ends_with(b"\r") {
                    &line_bytes[..line_bytes.len() - 1]
                } else {
                    line_bytes
                };

                if line_trimmed.is_empty() {
                    continue;
                }

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
                                    full_text.push_str(&content);
                                }
                            }
                        }
                    }
                }
            }

            if cursor > 0 {
                byte_buffer.drain(..cursor);
            }
        }

        assert_eq!(full_text, "Hello World");
    }
}
