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
        Self::new_with_proxy(None, false)
    }

    pub fn new_with_proxy(proxy_url: Option<&str>, no_proxy: bool) -> Self {
        Self {
            client: super::build_timed_client_with_proxy(
                Client::builder()
                    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36"),
                std::time::Duration::from_secs(10),
                proxy_url,
                no_proxy,
            ),
        }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
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
        let encoded_text = urlencoding::encode(text);

        // 优先使用 Chrome 扩展客户端标识（dict-chrome-ex），避免 gtx 标识在公共代理/VPN 节点被 Google 429 拦截；
        // 若首选客户端非 200，支持备选 gtx 客户端重试
        let clients = ["dict-chrome-ex", "gtx"];
        let mut last_err = None;

        for client_name in clients {
            let url = format!(
                "https://translate.googleapis.com/translate_a/single?client={client_name}&sl={sl}&tl={tl}&dt=t&q={encoded_text}"
            );

            let resp = super::send_with_retry(1, std::time::Duration::from_millis(200), || {
                self.client.get(&url).send()
            })
            .await;

            match resp {
                Ok(res) => {
                    if res.status().is_success() {
                        let json: Value = res.json().await?;
                        return Self::parse_gtx_response(&json, self.id());
                    } else {
                        last_err = Some(Error::Engine {
                            engine: self.id().to_string(),
                            message: format!("HTTP 请求失败状态码: {}", res.status()),
                        });
                    }
                }
                Err(err) => {
                    last_err = Some(Error::Network(err));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| Error::Engine {
            engine: self.id().to_string(),
            message: "Google 网页翻译端点请求失败".to_string(),
        }))
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

/// 判断在同一段落内拼接两个分片译文之间是否需要补一个空格。
///
/// 旧实现只看**原文**分片是否以 ASCII 结尾：长中文段落（非 ASCII 结尾）译成英文后，
/// 两个英文分片会被直接粘连成 `...worldThis...`。译文的分片边界同样是词边界，
/// 因此判定必须同时看**译文**首尾。
///
/// 判定条件（调用方保证 buf 与 next_chunk 均非空、且都不以空白起止）：
/// 1) 原文分片以 ASCII 结尾，且下一分片译文以 ASCII 开头——原文此处本就有词边界；
/// 2) 已拼接译文以 ASCII 字母/数字或英文常见标点结尾，且下一分片以 ASCII 字母/数字开头
///    ——说明译文自身是空白分隔的西文语言，粘连同样需要补空格。
///
/// 注意：仅当**存在下一个分片**时才调用。段落末尾不再追加空格，
/// 避免旧实现在行尾产生的多余空格。
fn needs_chunk_space(
    prev_translated: &str,
    next_translated: &str,
    prev_source_ascii_tail: bool,
) -> bool {
    let Some(next_first) = next_translated.chars().next() else {
        return false;
    };
    // 任一侧已带空白，说明边界已由译文自身处理，不再重复插入
    if next_first.is_whitespace() {
        return false;
    }
    let Some(prev_last) = prev_translated.chars().next_back() else {
        return false;
    };
    if prev_last.is_whitespace() {
        return false;
    }

    // 条件 1：原文分片以 ASCII 结尾且下一译文以 ASCII 开头
    if prev_source_ascii_tail && next_first.is_ascii() {
        return true;
    }

    // 条件 2：译文自身是空白分隔的西文（前一字符为 ASCII 字母数字或英文常见标点，
    // 且下一字符为 ASCII 字母数字）
    let prev_is_word_or_punct =
        prev_last.is_ascii_alphanumeric() || matches!(prev_last, '.' | ',' | '!' | '?' | ';' | ':');
    prev_is_word_or_punct && next_first.is_ascii_alphanumeric()
}

/// 把一个分片译文追加到已拼接缓冲区，按 `needs_chunk_space` 决定是否补空格。
fn append_translated_chunk(buf: &mut String, next_chunk: &str, prev_source_ascii_tail: bool) {
    if next_chunk.is_empty() {
        return;
    }
    if !buf.is_empty() && needs_chunk_space(buf, next_chunk, prev_source_ascii_tail) {
        buf.push(' ');
    }
    buf.push_str(next_chunk);
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
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
        // 超限文本按段落与字符窗口双重切片，彻底防止超长单行溢出。
        // 门槛按编码后长度判定，与 chunk_paragraph 内部上限保持同一量纲
        if needs_chunking(&req.text) {
            struct ChunkMeta {
                ascii_tail: bool,
            }

            enum ParagraphElement {
                EmptyLine,
                Chunks(Vec<ChunkMeta>),
            }

            let mut elements = Vec::new();
            let mut flat_chunk_texts = Vec::new();

            for paragraph in req.text.split('\n') {
                if paragraph.trim().is_empty() {
                    elements.push(ParagraphElement::EmptyLine);
                    continue;
                }

                let chunk_strs = chunk_paragraph(paragraph);
                let mut metas = Vec::with_capacity(chunk_strs.len());
                for chunk_str in chunk_strs {
                    let ascii_tail = chunk_str
                        .chars()
                        .next_back()
                        .map(|c| c.is_ascii())
                        .unwrap_or(false);
                    flat_chunk_texts.push(chunk_str);
                    metas.push(ChunkMeta { ascii_tail });
                }
                elements.push(ParagraphElement::Chunks(metas));
            }

            // 受控并发拉取（并发度 4，保序缓冲 buffered），将原本长文本串行累加延迟降为并发延迟
            use futures::stream::{self, StreamExt};
            let futures: Vec<_> = flat_chunk_texts
                .iter()
                .map(|chunk_str| {
                    self.translate_single_chunk(chunk_str, &req.source_lang, &req.target_lang)
                })
                .collect();

            let chunk_responses: Result<Vec<TranslationResponse>> = stream::iter(futures)
                .buffered(4)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect();
            let mut resp_iter = chunk_responses?.into_iter();

            let mut full_translated = String::new();
            let mut detected_lang = None;

            for element in elements {
                match element {
                    ParagraphElement::EmptyLine => {
                        full_translated.push('\n');
                    }
                    ParagraphElement::Chunks(metas) => {
                        // 记录前一分片的**原文**末尾是否为 ASCII，用于判定分片之间是否补空格。
                        // 段落起点处 full_translated 必以 '\n' 结尾（或为空），
                        // append_translated_chunk 的空白守卫会自动跳过，不会误插段间空格。
                        let mut prev_source_ascii_tail = false;
                        for meta in metas {
                            if let Some(res) = resp_iter.next() {
                                if detected_lang.is_none() {
                                    detected_lang = res.detected_source_lang;
                                }
                                append_translated_chunk(
                                    &mut full_translated,
                                    &res.text,
                                    prev_source_ascii_tail,
                                );
                            }
                            prev_source_ascii_tail = meta.ascii_tail;
                        }
                        full_translated.push('\n');
                    }
                }
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

        // 使用受控并发（并发度 4）并发执行单项翻译，保持故障隔离且避免串行阻塞过慢
        let chunk_size = 4;
        let mut all_results = Vec::with_capacity(requests.len());

        for chunk in requests.chunks(chunk_size) {
            let futures: Vec<_> = chunk.iter().map(|req| self.translate(req)).collect();
            let chunk_results = futures::future::join_all(futures).await;
            for res in chunk_results {
                all_results.push(res);
            }
        }

        all_results
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

    /// 回归：旧实现只看**原文**分片是否以 ASCII 结尾。长中文段落（非 ASCII 结尾）
    /// 译成英文后，两个英文分片会被直接粘连成 `...worldThis...`，整段译文不可读。
    #[test]
    fn test_needs_chunk_space_for_cjk_source_translated_to_english() {
        // 中文原文分片以非 ASCII 结尾（ascii_tail = false），但译文是英文句子。
        // 译文自身是空白分隔的西文，必须补空格。
        assert!(
            needs_chunk_space("Hello world", "This is the next part.", false),
            "中文原文译成英文时，两个英文分片之间必须补空格"
        );

        // 与 append_translated_chunk 的端到端拼接结果
        let mut buf = String::new();
        append_translated_chunk(&mut buf, "Hello world", false);
        append_translated_chunk(&mut buf, "This is the next part.", false);
        assert_eq!(buf, "Hello world This is the next part.");
        assert!(!buf.contains("worldThis"), "英文分片绝不允许粘连：{buf}");

        // 原文分片以 ASCII 结尾且译文以 ASCII 开头（原文本身有词边界）同样补空格
        assert!(needs_chunk_space("Hello", "world", true));
    }

    /// 反向：英文原文译成中文时，中文字符之间**不得**被塞入多余空格。
    #[test]
    fn test_append_translated_chunk_does_not_pad_chinese_output() {
        let mut buf = String::new();
        // 英文原文分片（ascii_tail = true），译文为中文
        append_translated_chunk(&mut buf, "你好", true);
        append_translated_chunk(&mut buf, "世界", true);
        assert_eq!(buf, "你好世界", "中文译文之间不得插入空格：{buf}");

        // 直接验证判定函数：中文首尾均不满足西文空白分隔条件
        assert!(!needs_chunk_space("你好", "世界", true));
        assert!(!needs_chunk_space("你好", "世界", false));

        // 中文译文以中文标点结尾时同样不补空格
        assert!(!needs_chunk_space("你好，", "世界。", true));
    }

    /// 边界：任一侧已带空白或为空时，不得重复插入空格。
    #[test]
    fn test_needs_chunk_space_respects_existing_whitespace_and_empty() {
        // 译文自身已以空白分隔
        assert!(!needs_chunk_space("Hello ", "world", true));
        assert!(!needs_chunk_space("Hello", " world", true));
        assert!(!needs_chunk_space("Hello", "\nworld", true));
        // 空分片
        assert!(!needs_chunk_space("Hello", "", true));
        assert!(!needs_chunk_space("", "world", true));
        // 缓冲区为空白（如段落起始的换行）时不得插入空格
        assert!(!needs_chunk_space("\n", "world", false));

        // 空分片被 append 直接忽略
        let mut buf = String::from("Hello");
        append_translated_chunk(&mut buf, "", true);
        assert_eq!(buf, "Hello");

        // 段落起始：缓冲区以 '\n' 结尾时，第一个分片必须原样追加，不得前置空格
        let mut para = String::from("上一段\n");
        append_translated_chunk(&mut para, "First chunk", false);
        assert_eq!(para, "上一段\nFirst chunk");
    }

    /// 回归：旧实现会在段落**最后一个分片**之后、'\n' 之前推入一个空格，
    /// 造成行尾多余空格。新逻辑只在存在下一个分片时判定，行尾不再补空格。
    #[test]
    fn test_append_translated_chunk_leaves_no_trailing_space_at_paragraph_end() {
        // 模拟段落只有一个分片且原文以 ASCII 结尾（旧实现会补 ' ' 再补 '\n'）
        let mut full = String::new();
        append_translated_chunk(&mut full, "Only chunk", true);
        full.push('\n');

        assert_eq!(full, "Only chunk\n");
        assert!(!full.contains(" \n"), "行尾不得残留多余空格：{full:?}");
        assert_eq!(full.trim(), "Only chunk");
    }
}
