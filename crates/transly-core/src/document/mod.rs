//! 文档与字幕翻译模块（纯逻辑，无平台依赖）
//!
//! 支持 `.txt`（纯文本）、`.md`（Markdown）、`.srt`（SubRip 字幕）格式。
//! 严格保障原文档结构、Markdown 代码块与公式、SRT 序号和时间轴 100% 保真。

pub mod html;
pub mod json;
pub mod md;
pub mod srt;
pub mod txt;

pub use html::parse_html;
pub use json::parse_json;
pub use md::parse_md;
pub use srt::parse_srt;
pub use txt::parse_txt;

/// 重构闭包类型别名
pub type DocumentReconstructor = Box<dyn FnOnce(Vec<String>) -> String + Send + Sync>;
/// 文档解析结果（文本块集合与对应重构闭包）
pub type ParsedDocument = (Vec<String>, DocumentReconstructor);

use crate::engine::{plan_batch_ranges, translate_batch_with_cache, BatchPipelineDeps};
use crate::{Result, TranslationRequest};

/// 支持的文档格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum DocumentFormat {
    Txt,
    Md,
    Srt,
    Json,
    Html,
}

impl std::str::FromStr for DocumentFormat {
    type Err = crate::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "txt" | "text" => Ok(Self::Txt),
            "md" | "markdown" => Ok(Self::Md),
            "srt" => Ok(Self::Srt),
            "json" => Ok(Self::Json),
            "html" | "htm" => Ok(Self::Html),
            other => Err(crate::Error::Input(format!("不支持的文档格式: {other}"))),
        }
    }
}

/// 文档翻译执行结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct DocumentTranslateOutcome {
    /// 翻译并按原格式完整重构后的文档内容
    pub translated_content: String,
    /// 文档拆分出的总待翻译块数
    pub total_chunks: u32,
    /// 成功翻译的块数
    pub translated_chunks: u32,
    /// 命中本地缓存的块数
    pub cache_hits: u32,
    /// 实际执行翻译的引擎名称
    pub engine: String,
}

/// 文档翻译进度回调参数：(当前已处理待翻块数, 待翻总块数, 缓存命中块数)
pub type DocumentProgressCallback<'a> = Box<dyn FnMut(u32, u32, u32) + Send + 'a>;

/// 执行文档全量翻译与重构流水线（无进度回调简易入口）。
pub async fn translate_document_content(
    deps: BatchPipelineDeps<'_>,
    content: &str,
    format: DocumentFormat,
    source_lang: &str,
    target_lang: &str,
) -> Result<DocumentTranslateOutcome> {
    translate_document_content_with_progress(
        deps,
        content,
        format,
        source_lang,
        target_lang,
        None,
        None,
    )
    .await
}

/// 执行文档全量翻译与重构流水线，支持流式进度回调与主动取消信号。
///
/// 遵循原则：
/// 1. 分片上限守卫：严格通过 `plan_batch_ranges` 将待翻译块切分为受控批次，绝不超 `MAX_BATCH_ITEMS` 与 `MAX_BATCH_TOTAL_CHARS`；
/// 2. 逐项容错隔离：若个别块翻译失败，安全回退到该块原文（fail-closed），绝不因个别报错中断或损坏整篇文档；
/// 3. 格式保真重构：重构阶段将译文填回原始定界槽位，保留空行、代码块、时间轴等非翻译结构；
/// 4. 批次级响应取消：在批次循环前检测取消信号，立即中断后续请求与 Token 消耗；
/// 5. 实时进度广播：单批完成立即触发回调，上层可即刻向 UI 汇报百分比。
pub async fn translate_document_content_with_progress<'a>(
    deps: BatchPipelineDeps<'_>,
    content: &str,
    format: DocumentFormat,
    source_lang: &str,
    target_lang: &str,
    mut on_progress: Option<DocumentProgressCallback<'a>>,
    mut cancel_rx: Option<&mut tokio::sync::oneshot::Receiver<()>>,
) -> Result<DocumentTranslateOutcome> {
    if content.is_empty() {
        return Ok(DocumentTranslateOutcome {
            translated_content: String::new(),
            total_chunks: 0,
            translated_chunks: 0,
            cache_hits: 0,
            engine: deps.engine.id().to_string(),
        });
    }

    // 1. 根据格式解析提取文本块与重构器
    let (chunks, reconstructor) = match format {
        DocumentFormat::Txt => parse_txt(content),
        DocumentFormat::Md => parse_md(content),
        DocumentFormat::Srt => parse_srt(content),
        DocumentFormat::Json => parse_json(content),
        DocumentFormat::Html => parse_html(content),
    };

    if chunks.is_empty() {
        return Ok(DocumentTranslateOutcome {
            translated_content: reconstructor(Vec::new()),
            total_chunks: 0,
            translated_chunks: 0,
            cache_hits: 0,
            engine: deps.engine.id().to_string(),
        });
    }

    // 2. 收集非空待翻译项（纯空白项不送翻译，避免触发输入为空报错）
    let mut translatable_indices = Vec::new();
    let mut reqs = Vec::new();

    for (idx, chunk) in chunks.iter().enumerate() {
        if !chunk.trim().is_empty() {
            translatable_indices.push(idx);
            reqs.push(TranslationRequest {
                text: chunk.clone(),
                source_lang: source_lang.to_string(),
                target_lang: target_lang.to_string(),
            });
        }
    }

    let total_chunks = chunks.len() as u32;
    if reqs.is_empty() {
        // 全部是空白块
        return Ok(DocumentTranslateOutcome {
            translated_content: reconstructor(chunks),
            total_chunks,
            translated_chunks: 0,
            cache_hits: 0,
            engine: deps.engine.id().to_string(),
        });
    }

    // 3. 按项数与字符总量双上限切批执行
    let ranges = plan_batch_ranges(reqs.iter().map(|r| r.text.as_str()));
    let mut translated_results = Vec::with_capacity(reqs.len());
    let mut total_cache_hits = 0u32;
    let mut successful_chunks = 0u32;
    let total_translatable = reqs.len() as u32;

    for (start, end) in ranges {
        // 批次前置取消检查：若收到主动取消信号，立即中断，避免浪费网络配额与模型调用
        if let Some(ref mut rx) = cancel_rx {
            match rx.try_recv() {
                Ok(()) => {
                    return Err(crate::Error::Internal(
                        "文档翻译任务已被主动取消".to_string(),
                    ));
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {}
            }
        }

        let batch_reqs = &reqs[start..end];
        let sub_deps = BatchPipelineDeps {
            engine: deps.engine,
            cache: deps.cache,
            glossary: deps.glossary,
            cache_enabled: deps.cache_enabled,
        };

        let outcome = translate_batch_with_cache(sub_deps, batch_reqs).await;
        total_cache_hits += outcome.cache_hits as u32;

        for (i, res) in outcome.results.into_iter().enumerate() {
            match res {
                Ok(resp) => {
                    successful_chunks += 1;
                    translated_results.push(resp.text);
                }
                Err(_) => {
                    // 局部失败容错回退为原文
                    translated_results.push(batch_reqs[i].text.clone());
                }
            }
        }

        // 单批完成，通知外部进度（已翻译块数, 待翻总块数, 缓存命中数）
        if let Some(ref mut cb) = on_progress {
            cb(successful_chunks, total_translatable, total_cache_hits);
        }
    }

    // 4. 将翻译结果对齐映射回原始全量块（含空块）
    let mut final_chunks = chunks;
    for (trans_idx, &orig_idx) in translatable_indices.iter().enumerate() {
        if let Some(translated_text) = translated_results.get(trans_idx) {
            final_chunks[orig_idx] = translated_text.clone();
        }
    }

    // 5. 格式重构
    let translated_content = reconstructor(final_chunks);

    Ok(DocumentTranslateOutcome {
        translated_content,
        total_chunks,
        translated_chunks: successful_chunks,
        cache_hits: total_cache_hits,
        engine: deps.engine.id().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::TranslateEngine;
    use crate::{Glossary, TranslationCache, TranslationResponse};
    use async_trait::async_trait;

    struct MockEchoEngine;

    #[async_trait]
    impl TranslateEngine for MockEchoEngine {
        fn id(&self) -> &str {
            "mock"
        }

        fn name(&self) -> &str {
            "Mock"
        }

        async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
            Ok(TranslationResponse {
                text: format!("[translated: {}]", req.text),
                detected_source_lang: None,
                engine: "mock".to_string(),
            })
        }
    }

    #[tokio::test]
    async fn test_translate_document_txt() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::new();
        let engine = MockEchoEngine;

        let deps = BatchPipelineDeps {
            engine: &engine,
            cache: &cache,
            glossary: &glossary,
            cache_enabled: true,
        };

        let content = "Hello world.\n\nSecond line.";
        let outcome = translate_document_content(deps, content, DocumentFormat::Txt, "en", "zh-CN")
            .await
            .unwrap();

        assert_eq!(outcome.total_chunks, 2);
        assert_eq!(outcome.translated_chunks, 2);
        assert_eq!(
            outcome.translated_content,
            "[translated: Hello world.]\n\n[translated: Second line.]"
        );
    }

    #[tokio::test]
    async fn test_translate_document_srt() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::new();
        let engine = MockEchoEngine;

        let deps = BatchPipelineDeps {
            engine: &engine,
            cache: &cache,
            glossary: &glossary,
            cache_enabled: true,
        };

        let content = "\
1
00:00:01,000 --> 00:00:04,000
First subtitle line.

2
00:00:05,000 --> 00:00:08,000
Second subtitle line.
";
        let outcome = translate_document_content(deps, content, DocumentFormat::Srt, "en", "zh-CN")
            .await
            .unwrap();

        assert_eq!(outcome.total_chunks, 2);
        assert_eq!(outcome.translated_chunks, 2);
        assert!(outcome
            .translated_content
            .contains("00:00:01,000 --> 00:00:04,000\n[translated: First subtitle line.]"));
        assert!(outcome
            .translated_content
            .contains("00:00:05,000 --> 00:00:08,000\n[translated: Second subtitle line.]"));
    }

    #[tokio::test]
    async fn test_translate_document_with_progress_callback() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::new();
        let engine = MockEchoEngine;

        let deps = BatchPipelineDeps {
            engine: &engine,
            cache: &cache,
            glossary: &glossary,
            cache_enabled: true,
        };

        let mut progress_snapshots = Vec::new();
        let on_progress: DocumentProgressCallback = Box::new(|done, total, hits| {
            progress_snapshots.push((done, total, hits));
        });

        let content = "Line 1\n\nLine 2";
        let outcome = translate_document_content_with_progress(
            deps,
            content,
            DocumentFormat::Txt,
            "en",
            "zh-CN",
            Some(on_progress),
            None,
        )
        .await
        .unwrap();

        assert_eq!(outcome.translated_chunks, 2);
        assert!(!progress_snapshots.is_empty());
        let (last_done, last_total, _) = *progress_snapshots.last().unwrap();
        assert_eq!(last_done, 2);
        assert_eq!(last_total, 2);
    }

    #[tokio::test]
    async fn test_translate_document_with_cancel_signal() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::new();
        let engine = MockEchoEngine;

        let deps = BatchPipelineDeps {
            engine: &engine,
            cache: &cache,
            glossary: &glossary,
            cache_enabled: true,
        };

        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        // 触发取消信号
        let _ = cancel_tx.send(());

        let content = "Line 1\n\nLine 2";
        let err = translate_document_content_with_progress(
            deps,
            content,
            DocumentFormat::Txt,
            "en",
            "zh-CN",
            None,
            Some(&mut cancel_rx),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("主动取消"));
    }
}
