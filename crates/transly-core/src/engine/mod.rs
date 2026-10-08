use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub mod deepl;
pub mod fallback;
pub mod google;
pub mod openai;
pub use deepl::DeepLEngine;
pub use fallback::FallbackEngine;
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

/// 翻译引擎能力描述符
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub id: String,
    pub name: String,
    pub max_batch_size: usize,
    pub supports_batch: bool,
    pub requires_api_key: bool,
    #[serde(default)]
    pub fallback_engine: Option<String>,
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
            fallback_engine: None,
        }
    }

    async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse>;

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

/// 统一的带 Fallback 降级感知的缓存查询纯函数（杜绝在 IPC 与 HTTP Server 两处重复造轮子与字符串嗅探）
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

    if let Some(ref fallback_id) = engine.capabilities().fallback_engine {
        if fallback_id != engine_id {
            if let Ok(Some(cached_text)) = cache.get(text, source_lang, target_lang, fallback_id) {
                return Some((cached_text, format!("{fallback_id}(fallback)")));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges_of(texts: &[String]) -> Vec<(usize, usize)> {
        plan_batch_ranges(texts.iter())
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
}
