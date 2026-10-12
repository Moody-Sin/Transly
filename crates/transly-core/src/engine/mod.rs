use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod deepl;
pub mod google;
pub mod openai;
pub use deepl::DeepLEngine;
pub use google::GoogleWebEngine;
pub use openai::OpenAIEngine;

use crate::{error::ErrorKind, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub source_lang: String,
    pub target_lang: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationResponse {
    pub text: String,
    pub detected_source_lang: Option<String>,
    pub engine: String,
}

impl TranslationResponse {
    /// 实际产出译文的引擎标识（剥离 `(fallback)` 后缀）。
    /// 缓存写入必须用它，否则降级路径会与查询侧的 key 不对称而永久 Miss。
    pub fn actual_engine(&self) -> &str {
        self.engine
            .strip_suffix("(fallback)")
            .unwrap_or(&self.engine)
    }
}

/// 批量翻译中单项请求契约（保证输入与输出具备稳定映射）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchItemRequest {
    pub id: String,
    pub text: String,
}

/// 批量翻译中单项结果契约（支持逐项表达成功或部分失败）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BatchItemResult {
    Success {
        id: String,
        text: String,
        detected_source_lang: Option<String>,
        engine: String,
    },
    Failed {
        id: String,
        error_kind: ErrorKind,
        message: String,
    },
}

impl BatchItemResult {
    pub fn id(&self) -> &str {
        match self {
            BatchItemResult::Success { id, .. } => id,
            BatchItemResult::Failed { id, .. } => id,
        }
    }
}

/// 流式输出增量 Token 回调
pub type StreamCallback = Box<dyn Fn(&str) + Send + Sync>;

/// 构建带超时与可选代理的 HTTP 客户端。
///
/// 若 `no_proxy` 为真，则显式调用 `builder.no_proxy()` 阻断环境与系统代理；
/// 若提供了 `proxy_url`，则配置 `reqwest::Proxy::all` 统一代理 HTTP/HTTPS 流量；
/// 若两者皆非，则遵循 reqwest 默认行为（自动读取系统环境变量代理）。
pub(crate) fn build_timed_client_with_proxy(
    builder: reqwest::ClientBuilder,
    timeout: std::time::Duration,
    proxy_url: Option<&str>,
    no_proxy: bool,
) -> reqwest::Client {
    let mut builder = builder.timeout(timeout);

    if no_proxy {
        builder = builder.no_proxy();
    } else if let Some(p) = proxy_url {
        let trimmed = p.trim();
        if !trimmed.is_empty() {
            let normalized =
                crate::proxy::normalize_proxy_url(trimmed).unwrap_or_else(|| trimmed.to_string());
            match reqwest::Proxy::all(&normalized) {
                Ok(proxy) => {
                    builder = builder.proxy(proxy);
                }
                Err(err) => {
                    eprintln!(
                        "[transly] 配置代理地址 '{normalized}' 失败: {err}。已退化为默认连接策略。"
                    );
                }
            }
        }
    }

    match builder.build() {
        Ok(client) => client,
        Err(err) => {
            eprintln!(
                "[transly] 构建带 {}s 超时的 HTTP 客户端失败：{err}。已退化为无超时客户端，翻译请求可能长时间挂起。",
                timeout.as_secs()
            );
            reqwest::Client::new()
        }
    }
}

#[allow(dead_code)]
pub(crate) fn build_timed_client(
    builder: reqwest::ClientBuilder,
    timeout: std::time::Duration,
) -> reqwest::Client {
    build_timed_client_with_proxy(builder, timeout, None, false)
}

/// 翻译引擎能力描述符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub id: String,
    pub name: String,
    pub max_batch_size: usize,
    pub supports_batch: bool,
    pub requires_api_key: bool,
    #[serde(default)]
    pub supports_streaming: bool,
}

#[async_trait]
pub trait TranslateEngine: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities {
            id: self.id().to_string(),
            name: self.name().to_string(),
            max_batch_size: 100,
            supports_batch: true,
            requires_api_key: false,
            supports_streaming: false,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse>;

    /// 流式翻译输出：每产生一个增量片段即触发 on_chunk 回调，最终返回完整的聚合响应。
    /// 默认实现回落至普通非流式 translate 并在完成时一次性回调完整文本。
    async fn translate_stream(
        &self,
        req: &TranslationRequest,
        on_chunk: StreamCallback,
    ) -> Result<TranslationResponse> {
        let res = self.translate(req).await?;
        on_chunk(&res.text);
        Ok(res)
    }

    async fn translate_batch(
        &self,
        requests: &[TranslationRequest],
    ) -> Result<Vec<TranslationResponse>> {
        let mut results = Vec::with_capacity(requests.len());
        for req in requests {
            results.push(self.translate(req).await?);
        }
        Ok(results)
    }

    /// 逐项批量翻译（支持部分成功与部分失败容错隔离）。
    /// 默认实现优先调用 translate_batch，若整体失败则回退至逐项翻译以隔离故障项。
    async fn translate_batch_tolerant(
        &self,
        requests: &[TranslationRequest],
    ) -> Vec<Result<TranslationResponse>> {
        match self.translate_batch(requests).await {
            Ok(results) => results.into_iter().map(Ok).collect(),
            Err(_) => {
                let mut tolerant = Vec::with_capacity(requests.len());
                for req in requests {
                    tolerant.push(self.translate(req).await);
                }
                tolerant
            }
        }
    }
}

/// 单次批量请求的硬上限（项数）。
///
/// 三处必须共用同一常量：引擎契约（`EngineCapabilities::max_batch_size` 上限）、
/// 本地 HTTP 服务入口校验、扩展端切批大小。任一处各写一个数字必然漂移。
pub const MAX_BATCH_ITEMS: usize = 30;

/// 单次批量请求的字符总量上限。
///
/// 扩展端按「项数」切批，服务端按「字符总量」拒绝，两者量纲不同：
/// 30 项 × 平均 1667 字即触及此上限，整批被 413 拒绝。
/// 因此切批必须同时满足两个上限，由 [`plan_batch_ranges`] 统一决定。
pub const MAX_BATCH_TOTAL_CHARS: usize = 50_000;

/// 单条文本的字符上限（与 `validate_translation_input` 一致）。
pub const MAX_ITEM_CHARS: usize = 10_000;

/// 将一批文本按项数与字符总量双上限切成连续区间的下标。
///
/// 返回 `Vec<(start, end)>`（左闭右开），保证：
/// - 每个区间内字符总量不超过 `MAX_BATCH_TOTAL_CHARS`；
/// - 每个区间内项数不超过 `MAX_BATCH_ITEMS`；
/// - 单条文本自身超限时不静默丢弃，独占一个区间，交由服务端逐项报错。
pub fn plan_batch_ranges<I, S>(texts: I) -> Vec<(usize, usize)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let lens: Vec<usize> = texts
        .into_iter()
        .map(|t| t.as_ref().chars().count())
        .collect();

    let mut ranges = Vec::new();
    let mut start = 0usize;
    let mut acc = 0usize;

    for (idx, len) in lens.iter().enumerate() {
        let count = idx - start;
        let over_chars = acc + len > MAX_BATCH_TOTAL_CHARS;
        let over_items = count >= MAX_BATCH_ITEMS;

        if count > 0 && (over_chars || over_items) {
            ranges.push((start, idx));
            start = idx;
            acc = 0;
        }
        acc += len;
    }

    if start < lens.len() {
        ranges.push((start, lens.len()));
    }
    ranges
}

/// 判断 HTTP 状态码或错误类型是否为可进行瞬态重试的错误
pub fn is_transient_retryable_status(status_code: u16) -> bool {
    // 429: Too Many Requests (限流)
    // 500: Internal Server Error
    // 502: Bad Gateway
    // 503: Service Unavailable
    // 504: Gateway Timeout
    matches!(status_code, 429 | 500 | 502 | 503 | 504)
}

/// 执行带指数退避的瞬态网络请求重试
///
/// 仅针对底层网络连接中断、超时或服务端 429/5xx 瞬态故障重试至多 max_retries 次；
/// 401/403 等鉴权与输入错误绝不重试。
pub async fn send_with_retry<F, Fut>(
    max_retries: usize,
    base_delay: std::time::Duration,
    mut op: F,
) -> std::result::Result<reqwest::Response, reqwest::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = std::result::Result<reqwest::Response, reqwest::Error>>,
{
    let mut attempt = 0;
    loop {
        match op().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if attempt < max_retries && is_transient_retryable_status(status) {
                    attempt += 1;
                    tokio::time::sleep(base_delay * (1 << (attempt - 1))).await;
                    continue;
                }
                return Ok(resp);
            }
            Err(err) => {
                let is_transient = err.is_timeout() || err.is_connect() || err.is_request();
                if attempt < max_retries && is_transient {
                    attempt += 1;
                    tokio::time::sleep(base_delay * (1 << (attempt - 1))).await;
                    continue;
                }
                return Err(err);
            }
        }
    }
}

/// 统一的缓存查询纯函数（仅查询请求引擎专属缓存，杜绝跨引擎静默降级）
pub fn lookup_cache_with_fallback(
    cache: &crate::TranslationCache,
    engine: &dyn TranslateEngine,
    text: &str,
    source_lang: &str,
    target_lang: &str,
) -> Option<(String, String)> {
    let engine_id = engine.id();
    if let Ok(Some(cached_text)) = cache.get(text, source_lang, target_lang, engine_id) {
        return Some((cached_text, engine_id.to_string()));
    }

    None
}

/// 统一的批量缓存查询纯函数（结合 L1 内存直出与 L2 批量 SQL，仅查询请求引擎专属缓存）
pub fn lookup_batch_with_fallback(
    cache: &crate::TranslationCache,
    engine: &dyn TranslateEngine,
    items: &[BatchItemRequest],
    source_lang: &str,
    target_lang: &str,
) -> Vec<Option<(String, String)>> {
    if items.is_empty() {
        return Vec::new();
    }

    let engine_id = engine.id();
    let primary_keys: Vec<String> = items
        .iter()
        .map(|item| {
            crate::cache::TranslationCache::generate_key(
                &item.text,
                source_lang,
                target_lang,
                engine_id,
            )
        })
        .collect();

    let primary_hits = cache.get_by_keys(&primary_keys).unwrap_or_default();

    let mut results = vec![None; items.len()];
    for (idx, key) in primary_keys.into_iter().enumerate() {
        if let Some(text) = primary_hits.get(&key) {
            results[idx] = Some((text.clone(), engine_id.to_string()));
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges_of(texts: &[String]) -> Vec<(usize, usize)> {
        plan_batch_ranges(texts.iter())
    }

    #[test]
    fn test_is_transient_retryable_status() {
        assert!(is_transient_retryable_status(429));
        assert!(is_transient_retryable_status(500));
        assert!(is_transient_retryable_status(502));
        assert!(is_transient_retryable_status(503));
        assert!(is_transient_retryable_status(504));

        assert!(!is_transient_retryable_status(200));
        assert!(!is_transient_retryable_status(400));
        assert!(!is_transient_retryable_status(401));
        assert!(!is_transient_retryable_status(403));
        assert!(!is_transient_retryable_status(404));
    }

    #[test]
    fn test_plan_batch_ranges_splits_on_char_total() {
        // 回归：扩展端按项数切批（30 项），服务端按字符总量拒绝（50000），
        // 量纲错配导致 30×2000 字的批次被整批 413 拒绝。
        let texts = vec!["a".repeat(2_000); 30];
        let ranges = ranges_of(&texts);
        assert!(ranges.len() > 1, "超出字符总量上限必须切成多批");
        for (start, end) in &ranges {
            let total: usize = texts[*start..*end].iter().map(|t| t.chars().count()).sum();
            assert!(
                total <= MAX_BATCH_TOTAL_CHARS,
                "每批字符总量不得超限，实际 {total}"
            );
            assert!(
                end - start <= MAX_BATCH_ITEMS,
                "每批项数不得超限，实际 {}",
                end - start
            );
        }
        // 区间必须连续且完整覆盖，不得丢项或重复
        assert_eq!(ranges.first().map(|r| r.0), Some(0));
        assert_eq!(ranges.last().map(|r| r.1), Some(texts.len()));
        for pair in ranges.windows(2) {
            assert_eq!(pair[0].1, pair[1].0, "区间必须连续");
        }
    }

    #[test]
    fn test_plan_batch_ranges_splits_on_item_count() {
        // 短文本时由项数上限主导
        let texts = vec!["hi".to_string(); 100];
        let ranges = ranges_of(&texts);
        assert_eq!(ranges, vec![(0, 30), (30, 60), (60, 90), (90, 100)]);
    }

    #[test]
    fn test_plan_batch_ranges_single_oversized_item_isolated() {
        // 单条自身超限：不静默丢弃，独占一批交由服务端逐项报错
        let texts = vec!["a".repeat(60_000), "b".to_string(), "c".to_string()];
        let ranges = ranges_of(&texts);
        assert_eq!(ranges[0], (0, 1), "超长项必须独占一批");
        assert_eq!(ranges[1], (1, 3), "其余项正常成批");
    }

    #[test]
    fn test_plan_batch_ranges_edge_cases() {
        assert!(ranges_of(&[]).is_empty(), "空输入不得产出区间");
        assert_eq!(ranges_of(&["x".to_string()]), vec![(0, 1)]);
        // 恰好等于上限时不分批
        let exact = vec!["a".repeat(MAX_BATCH_TOTAL_CHARS / 2); 2];
        assert_eq!(ranges_of(&exact), vec![(0, 2)]);
    }

    #[test]
    fn test_batch_item_contracts() {
        let success_item = BatchItemResult::Success {
            id: "node_1".to_string(),
            text: "测试译文".to_string(),
            detected_source_lang: Some("en".to_string()),
            engine: "google_web".to_string(),
        };

        let failed_item = BatchItemResult::Failed {
            id: "node_2".to_string(),
            error_kind: ErrorKind::Engine,
            message: "请求超时".to_string(),
        };

        // 验证序列化格式
        let json_success = serde_json::to_string(&success_item).unwrap();
        assert!(json_success.contains(r#""status":"success""#));
        assert!(json_success.contains(r#""text":"测试译文""#));

        let json_failed = serde_json::to_string(&failed_item).unwrap();
        assert!(json_failed.contains(r#""status":"failed""#));
        assert!(json_failed.contains(r#""error_kind":"engine""#));
    }

    #[test]
    fn test_lookup_batch_with_fallback() {
        let cache = crate::TranslationCache::in_memory().unwrap();
        let google = GoogleWebEngine::new();

        cache
            .set("hi", "你好", "en", "zh-CN", "google_web")
            .unwrap();

        let items = vec![
            BatchItemRequest {
                id: "1".into(),
                text: "hi".into(),
            },
            BatchItemRequest {
                id: "2".into(),
                text: "not_cached".into(),
            },
        ];

        let hits = lookup_batch_with_fallback(&cache, &google, &items, "en", "zh-CN");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0], Some(("你好".into(), "google_web".into())));
        assert_eq!(hits[1], None);
    }

    #[test]
    fn test_lookup_cache_isolates_engines_without_fallback() {
        let cache = crate::TranslationCache::in_memory().unwrap();
        let google = GoogleWebEngine::new();
        let deepl = DeepLEngine::new("dummy-key");

        // 仅写入 google_web 缓存
        cache
            .set("hi", "你好", "en", "zh-CN", "google_web")
            .unwrap();

        // google_web 命中
        let hit_google = lookup_cache_with_fallback(&cache, &google, "hi", "en", "zh-CN");
        assert_eq!(hit_google, Some(("你好".into(), "google_web".into())));

        // deepl 不再具备自动降级嗅探，严格隔离返回 None
        let hit_deepl = lookup_cache_with_fallback(&cache, &deepl, "hi", "en", "zh-CN");
        assert_eq!(hit_deepl, None);

        let items = vec![BatchItemRequest {
            id: "1".into(),
            text: "hi".into(),
        }];
        let batch_deepl = lookup_batch_with_fallback(&cache, &deepl, &items, "en", "zh-CN");
        assert_eq!(batch_deepl, vec![None]);
    }
}
