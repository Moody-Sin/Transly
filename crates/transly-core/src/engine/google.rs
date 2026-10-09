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

/// 单次 GET 请求 `q` 参数编码后的长度上限。
///
/// `urlencoding` 会把每个非 ASCII 字节编码为 `%XX`（UTF-8 中文 3 字节 → 9 字符），
/// 因此仅按“字符数”切块会让中文长段落生成数万字符的超长 URL 并直接请求失败。
const MAX_ENCODED_QUERY_CHARS: usize = 4_000;

/// 单个切块的最大字符数（西文按空白边界切分时的上限）
const MAX_CHUNK_CHARS: usize = 800;

/// 估算片段经 urlencoding 后的长度：ASCII 记 1，其余按最坏情况 9 计
fn encoded_len(s: &str) -> usize {
    s.chars().map(|c| if c.is_ascii() { 1 } else { 9 }).sum()
}

/// 是否需要先切块再发请求。
///
/// 判定必须与切块上限同量纲（编码后长度），不能按字符数：
/// 中文 1 字编码后为 9 字符，445~1000 字的中文段落字符数看似不大，
/// 编码后却已越过 `MAX_ENCODED_QUERY_CHARS`。按字符数判定会放这段文本
/// 绕过 `chunk_paragraph` 直连，生成超出自身声明上限的 URL。
fn needs_chunking(text: &str) -> bool {
    encoded_len(text) > MAX_ENCODED_QUERY_CHARS
}

/// 将单个段落切成可安全放入 GET URL 的片段。
///
/// 策略：
/// 1. 优先按空白边界聚合，避免切断西文单词；
/// 2. 无空白的长片段（中文长段落、长 URL、Base64 串）按字符窗口硬切；
/// 3. 双重上限：字符数与编码后长度，取先到者。
fn chunk_paragraph(paragraph: &str) -> Vec<String> {
    fn flush(chunks: &mut Vec<String>, current: &mut String) {
        let trimmed = current.trim();
        if !trimmed.is_empty() {
            chunks.push(trimmed.to_string());
        }
        current.clear();
    }

    /// 按字符窗口硬切超长片段（无空白可切时）
    fn hard_split(word: &str, chunks: &mut Vec<String>) {
        let mut buf = String::new();
        let mut buf_chars = 0usize;
        let mut buf_encoded = 0usize;
        for ch in word.chars() {
            let ch_encoded = if ch.is_ascii() { 1 } else { 9 };
            if buf_chars >= MAX_CHUNK_CHARS || buf_encoded + ch_encoded > MAX_ENCODED_QUERY_CHARS {
                chunks.push(std::mem::take(&mut buf));
                buf_chars = 0;
                buf_encoded = 0;
            }
            buf.push(ch);
            buf_chars += 1;
            buf_encoded += ch_encoded;
        }
        if !buf.is_empty() {
            chunks.push(buf);
        }
    }

    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_chars = 0usize;
    let mut current_encoded = 0usize;

    for word in paragraph.split_whitespace() {
        let word_chars = word.chars().count();
        let word_encoded = encoded_len(word);

        // 自身就超限的片段（无空白可切）先落盘当前块，再硬切
        if word_chars > MAX_CHUNK_CHARS || word_encoded > MAX_ENCODED_QUERY_CHARS {
            flush(&mut chunks, &mut current);
            current_chars = 0;
            current_encoded = 0;
            hard_split(word, &mut chunks);
            continue;
        }

        let separator = usize::from(current_chars > 0);
        if current_chars > 0
            && (current_chars + separator + word_chars > MAX_CHUNK_CHARS
                || current_encoded + separator + word_encoded > MAX_ENCODED_QUERY_CHARS)
        {
            flush(&mut chunks, &mut current);
            current_chars = 0;
            current_encoded = 0;
        }
        if current_chars > 0 {
            current.push(' ');
            current_chars += 1;
            current_encoded += 1;
        }
        current.push_str(word);
        current_chars += word_chars;
        current_encoded += word_encoded;
    }
    flush(&mut chunks, &mut current);
    chunks
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
            max_batch_size: 1,
            supports_batch: false,
            requires_api_key: false,
            supports_streaming: false,
            fallback_engine: None,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        // 超限文本按段落与字符窗口双重切片，彻底防止超长单行溢出。
        // 门槛按编码后长度判定，与 chunk_paragraph 内部上限保持同一量纲
        if needs_chunking(&req.text) {
            let mut full_translated = String::new();
            let mut detected_lang = None;

            for paragraph in req.text.split('\n') {
                if paragraph.trim().is_empty() {
                    full_translated.push('\n');
                    continue;
                }

                // 段落内再按空白边界与字符窗口双重上限切块（含无空格的中文长段落）
                for chunk_str in chunk_paragraph(paragraph) {
                    let res = self
                        .translate_single_chunk(&chunk_str, &req.source_lang, &req.target_lang)
                        .await?;
                    if detected_lang.is_none() {
                        detected_lang = res.detected_source_lang;
                    }
                    full_translated.push_str(&res.text);
                    // 仅在西文块之间补空格；中文等无空格语言补空格会在句中断字
                    let ascii_tail = chunk_str
                        .chars()
                        .next_back()
                        .map(|c| c.is_ascii())
                        .unwrap_or(false);
                    if ascii_tail && !res.text.ends_with(' ') {
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
        assert_eq!(caps.max_batch_size, 1);
        assert!(!caps.supports_batch);
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
    fn test_chunk_paragraph_splits_cjk_without_whitespace() {
        // 无空格中文长段落：按字符窗口硬切，且必须遵守编码长度上限
        let text = "中".repeat(2_000);
        let chunks = chunk_paragraph(&text);
        assert!(
            chunks.len() >= 5,
            "中文长段落必须切成多块，实际块数 {}",
            chunks.len()
        );
        for chunk in &chunks {
            assert!(chunk.chars().count() <= MAX_CHUNK_CHARS);
            assert!(encoded_len(chunk) <= MAX_ENCODED_QUERY_CHARS);
        }
        assert_eq!(chunks.concat(), text, "硬切不得丢失任何字符");
    }

    #[test]
    fn test_needs_chunking_uses_encoded_length_not_char_count() {
        // 回归：门槛曾按字符数判定（>1000），导致 445~1000 字的中文段落
        // 绕过 chunk_paragraph 直连，编码后长度（445×9=4005）已越过自身上限。
        let cjk_500 = "中".repeat(500);
        assert!(
            cjk_500.chars().count() < 1000,
            "该用例必须落在旧门槛的盲区内，否则测不到回归"
        );
        assert!(
            encoded_len(&cjk_500) > MAX_ENCODED_QUERY_CHARS,
            "中文 500 字编码后应超限"
        );
        assert!(needs_chunking(&cjk_500), "超限中文段落必须进切块分支");

        // 边界：恰好等于上限不进切块，多一个非 ASCII 字符即进
        let ascii_at_limit = "a".repeat(MAX_ENCODED_QUERY_CHARS);
        assert!(!needs_chunking(&ascii_at_limit));
        assert!(needs_chunking(&format!("{ascii_at_limit}中")));

        // 短文本与西文长文不受影响
        assert!(!needs_chunking("hello 世界"));
        assert!(needs_chunking(&"a".repeat(MAX_ENCODED_QUERY_CHARS + 1)));
    }

    #[test]
    fn test_every_chunk_respects_encoded_limit_for_former_blind_spot() {
        // 旧盲区文本经 chunk_paragraph 后，每块都必须落在编码长度上限内
        let text = "中".repeat(500);
        let chunks = chunk_paragraph(&text);
        assert!(chunks.len() > 1, "500 字中文必须切成多块");
        for chunk in &chunks {
            assert!(
                encoded_len(chunk) <= MAX_ENCODED_QUERY_CHARS,
                "切块后仍超出编码长度上限"
            );
        }
        assert_eq!(chunks.concat(), text, "硬切不得丢失任何字符");
    }

    #[test]
    fn test_chunk_paragraph_prefers_whitespace_boundaries() {
        let text = std::iter::repeat_n("word", 500)
            .collect::<Vec<_>>()
            .join(" ");
        let chunks = chunk_paragraph(&text);
        assert!(chunks.len() > 1, "超长西文段落必须分块");
        for chunk in &chunks {
            assert!(chunk.chars().count() <= MAX_CHUNK_CHARS);
            assert!(!chunk.starts_with(' ') && !chunk.ends_with(' '));
        }
        assert_eq!(chunks.join(" "), text, "按空白切分不得丢词或改词");
    }

    #[test]
    fn test_chunk_paragraph_hard_splits_long_single_token() {
        // 长 URL / Base64 串这类无空白片段同样不得生成超长 URL
        let token = "x".repeat(2_500);
        let chunks = chunk_paragraph(&token);
        assert!(chunks.len() >= 4);
        assert!(chunks.iter().all(|c| c.chars().count() <= MAX_CHUNK_CHARS));
        assert_eq!(chunks.concat(), token);
    }

    #[test]
    fn test_chunk_paragraph_short_text_single_chunk() {
        let chunks = chunk_paragraph("hello 世界");
        assert_eq!(chunks, vec!["hello 世界".to_string()]);
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
