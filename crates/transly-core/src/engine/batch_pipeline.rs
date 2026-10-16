//! 带缓存与术语库的批量翻译编排（纯逻辑，无平台依赖）。
//!
//! # 为什么要有这一层
//!
//! 在抽出本模块前，`commands::translate`、`commands::selection.rs`、
//! `server/mod.rs` 各有一份「术语库源端替换 → 查缓存 → 引擎 → 写缓存 →
//! 术语库译文端」的编排。截图翻译若再加一份就是**第四份复制**，
//! 任何一处顺序修正（如 LESSONS 第 8 条的译文端反转）都必然漂移。
//!
//! 本模块是**单一实现**：可用假引擎 + 内存 SQLite 完整单测，
//! 天然计入 `transly-core` 的 70% 覆盖率门禁。

use crate::engine::{TranslateEngine, MAX_ITEM_CHARS};
use crate::{Glossary, Result, TranslationCache, TranslationRequest, TranslationResponse};

/// 批量翻译流水线的依赖。
///
/// 生命周期刻意用引用：调用方持有 `Arc`，本结构只在一次调用内存活。
pub struct BatchPipelineDeps<'a> {
    pub engine: &'a dyn TranslateEngine,
    pub cache: &'a TranslationCache,
    pub glossary: &'a Glossary,
    /// 用户是否启用缓存。关闭时既不查也不写。
    pub cache_enabled: bool,
}

/// 批量翻译结果（与请求等长）。
pub struct BatchPipelineOutcome {
    /// 与请求下标一一对应；`Ok` 为译文，`Err` 为该项的失败原因
    pub results: Vec<Result<TranslationResponse>>,
    /// 命中缓存而不必请求引擎的项数（便于前端展示与测试断言）
    pub cache_hits: usize,
}

/// 执行一次「术语库 → 缓存 → 引擎 → 缓存 → 术语库」的完整批量翻译。
///
/// # 顺序（**不可调换**）
///
/// 1. **源端**术语库预替换（`Glossary::apply`）
/// 2. 查缓存（键必须含引擎 id，绝不跨引擎命中）
/// 3. 未命中的项送引擎（逐项容错，单项失败不影响其他项）
/// 4. 写缓存
/// 5. **译文端**术语库后替换（`apply_to_translation`，会跳过已是目标词的片段）
///
/// ⚠️ 第 5 步必须用 `apply_to_translation` 而非 `apply`：双向词条
/// （`A→B` 且 `B→A`）下用 `apply` 会把正确译文反向改写（LESSONS 第 8 条）。
/// 缓存写入的是**未经**译文端替换的文本——否则缓存里存的就是被改写的版本，
/// 用户调整词库后旧缓存会持续输出错误结果。
pub async fn translate_batch_with_cache(
    deps: BatchPipelineDeps<'_>,
    requests: &[TranslationRequest],
) -> BatchPipelineOutcome {
    if requests.is_empty() {
        return BatchPipelineOutcome {
            results: Vec::new(),
            cache_hits: 0,
        };
    }

    let BatchPipelineDeps {
        engine,
        cache,
        glossary,
        cache_enabled,
    } = deps;

    // ── 1. 源端术语库预替换 ────────────────────────────────────────────
    let effective: Vec<String> = if glossary.is_empty() {
        requests.iter().map(|r| r.text.clone()).collect()
    } else {
        requests.iter().map(|r| glossary.apply(&r.text)).collect()
    };

    // 超长项直接失败：不送引擎，避免浪费配额（与 validate_translation_input 同源）
    let mut results: Vec<Option<Result<TranslationResponse>>> =
        (0..requests.len()).map(|_| None).collect();
    for (idx, text) in effective.iter().enumerate() {
        if text.chars().count() > MAX_ITEM_CHARS {
            results[idx] = Some(Err(crate::Error::Input(format!(
                "翻译文本超长（{} 字符，上限 {MAX_ITEM_CHARS}）",
                text.chars().count()
            ))));
        }
    }

    // ── 2. 查缓存 ──────────────────────────────────────────────────────
    let mut cache_hits = 0usize;
    if cache_enabled {
        let engine_id = engine.id();
        let keys: Vec<String> = effective
            .iter()
            .zip(requests.iter())
            .map(|(text, req)| {
                TranslationCache::generate_key(text, &req.source_lang, &req.target_lang, engine_id)
            })
            .collect();

        let hits = cache.get_by_keys(&keys).unwrap_or_default();
        for (idx, key) in keys.iter().enumerate() {
            if results[idx].is_some() {
                continue; // 已因超长失败
            }
            if let Some(cached) = hits.get(key) {
                results[idx] = Some(Ok(TranslationResponse {
                    text: cached.clone(),
                    detected_source_lang: None,
                    engine: engine_id.to_string(),
                }));
                cache_hits += 1;
            }
        }
    }

    // ── 3. 未命中的项送引擎（逐项容错） ─────────────────────────────────
    let pending: Vec<(usize, &TranslationRequest, &str)> = requests
        .iter()
        .zip(effective.iter())
        .enumerate()
        .filter(|(idx, _)| results[*idx].is_none())
        .map(|(idx, (req, text))| (idx, req, text.as_str()))
        .collect();

    if !pending.is_empty() {
        let engine_reqs: Vec<TranslationRequest> = pending
            .iter()
            .map(|(_, req, text)| TranslationRequest {
                text: (*text).to_string(),
                source_lang: req.source_lang.clone(),
                target_lang: req.target_lang.clone(),
            })
            .collect();

        let engine_results = engine.translate_batch_tolerant(&engine_reqs).await;
        // translate_batch_tolerant 保证与输入等长
        for ((idx, req, text), res) in pending.iter().zip(engine_results) {
            match res {
                Ok(resp) => {
                    // ── 4. 写缓存（仅成功项，且存的是未经译文端替换的文本）
                    if cache_enabled {
                        let _ = cache.set(
                            text,
                            &resp.text,
                            &req.source_lang,
                            &req.target_lang,
                            &resp.engine,
                        );
                    }
                    results[*idx] = Some(Ok(resp));
                }
                Err(e) => results[*idx] = Some(Err(e)),
            }
        }
    }

    // ── 5. 译文端术语库后替换 ──────────────────────────────────────────
    let final_results = results
        .into_iter()
        .enumerate()
        .map(|(idx, r)| {
            let Some(r) = r else {
                return Err(crate::Error::Internal(format!(
                    "第 {idx} 项既未命中缓存也未送引擎（逻辑漏洞）"
                )));
            };
            match r {
                Ok(mut resp) => {
                    if !glossary.is_empty() {
                        resp.text = glossary.apply_to_translation(&resp.text);
                    }
                    Ok(resp)
                }
                Err(e) => Err(e),
            }
        })
        .collect();

    BatchPipelineOutcome {
        results: final_results,
        cache_hits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{TranslationRequest, TranslationResponse};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    /// 假引擎：记录收到的请求，返回可预测的译文。
    struct FakeEngine {
        id: String,
        received: Arc<Mutex<Vec<String>>>,
        /// 若设置，则所有请求返回该错误
        fail_with: Option<String>,
    }

    #[async_trait]
    impl TranslateEngine for FakeEngine {
        fn id(&self) -> &str {
            &self.id
        }
        fn name(&self) -> &str {
            "Fake"
        }
        async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
            self.received.lock().unwrap().push(req.text.clone());
            if let Some(msg) = &self.fail_with {
                return Err(crate::Error::Engine {
                    engine: self.id.clone(),
                    message: msg.clone(),
                });
            }
            Ok(TranslationResponse {
                text: format!("[{}]", req.text),
                detected_source_lang: Some("auto".to_string()),
                engine: self.id.clone(),
            })
        }
        /// 覆盖 batch 以模拟「整批失败后回退逐项」的容错路径
        async fn translate_batch(
            &self,
            reqs: &[TranslationRequest],
        ) -> Result<Vec<TranslationResponse>> {
            let mut out = Vec::with_capacity(reqs.len());
            for r in reqs {
                out.push(self.translate(r).await?);
            }
            Ok(out)
        }
    }

    fn deps<'a>(
        engine: &'a dyn TranslateEngine,
        cache: &'a TranslationCache,
        glossary: &'a Glossary,
        cache_enabled: bool,
    ) -> BatchPipelineDeps<'a> {
        BatchPipelineDeps {
            engine,
            cache,
            glossary,
            cache_enabled,
        }
    }

    fn req(text: &str) -> TranslationRequest {
        TranslationRequest {
            text: text.to_string(),
            source_lang: "auto".to_string(),
            target_lang: "zh-CN".to_string(),
        }
    }

    #[tokio::test]
    async fn test_translates_and_returns_engine_text() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();
        let received = Arc::new(Mutex::new(Vec::new()));
        let engine = FakeEngine {
            id: "fake".into(),
            received: Arc::clone(&received),
            fail_with: None,
        };

        let out = translate_batch_with_cache(
            deps(&engine, &cache, &glossary, true),
            &[req("hello"), req("world")],
        )
        .await;

        assert_eq!(out.cache_hits, 0);
        assert_eq!(out.results.len(), 2);
        assert_eq!(out.results[0].as_ref().unwrap().text, "[hello]");
        assert_eq!(out.results[1].as_ref().unwrap().text, "[world]");
    }

    /// 第二次调用应命中缓存，**不再**打扰引擎。
    #[tokio::test]
    async fn test_cache_hit_avoids_engine_call() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();
        let received = Arc::new(Mutex::new(Vec::new()));
        let engine = FakeEngine {
            id: "fake".into(),
            received: Arc::clone(&received),
            fail_with: None,
        };

        let _ = translate_batch_with_cache(deps(&engine, &cache, &glossary, true), &[req("hello")])
            .await;
        assert_eq!(received.lock().unwrap().len(), 1);

        let out =
            translate_batch_with_cache(deps(&engine, &cache, &glossary, true), &[req("hello")])
                .await;
        assert_eq!(out.cache_hits, 1, "第二次应命中缓存");
        assert_eq!(received.lock().unwrap().len(), 1, "缓存命中仍调用了引擎");
        assert_eq!(out.results[0].as_ref().unwrap().text, "[hello]");
    }

    /// 关闭缓存时既不查也不写。
    #[tokio::test]
    async fn test_cache_disabled_bypasses_cache() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();
        let received = Arc::new(Mutex::new(Vec::new()));
        let engine = FakeEngine {
            id: "fake".into(),
            received: Arc::clone(&received),
            fail_with: None,
        };

        let _ =
            translate_batch_with_cache(deps(&engine, &cache, &glossary, false), &[req("hello")])
                .await;
        let out =
            translate_batch_with_cache(deps(&engine, &cache, &glossary, false), &[req("hello")])
                .await;

        assert_eq!(out.cache_hits, 0);
        assert_eq!(
            received.lock().unwrap().len(),
            2,
            "关闭缓存后应每次都请求引擎"
        );
    }

    /// 关键回归：缓存**不跨引擎**命中。
    #[tokio::test]
    async fn test_cache_is_engine_scoped() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();
        let got_calls = Arc::new(Mutex::new(Vec::new()));
        let engine_a = FakeEngine {
            id: "engine_a".into(),
            received: Arc::clone(&got_calls),
            fail_with: None,
        };
        let _ =
            translate_batch_with_cache(deps(&engine_a, &cache, &glossary, true), &[req("hello")])
                .await;

        // 换成另一个引擎，同样文本必须重新请求
        let engine_b = FakeEngine {
            id: "engine_b".into(),
            received: Arc::new(Mutex::new(Vec::new())),
            fail_with: None,
        };
        let out =
            translate_batch_with_cache(deps(&engine_b, &cache, &glossary, true), &[req("hello")])
                .await;
        assert_eq!(out.cache_hits, 0, "跨引擎命中了缓存");
        assert_eq!(out.results[0].as_ref().unwrap().engine, "engine_b");
    }

    /// 关键回归：单项失败不中断整批。
    #[tokio::test]
    async fn test_partial_failure_does_not_abort_batch() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();

        // 引擎对含 "boom" 的项失败
        struct PartialFail;
        #[async_trait]
        impl TranslateEngine for PartialFail {
            fn id(&self) -> &str {
                "partial"
            }
            fn name(&self) -> &str {
                "Partial"
            }
            async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
                if req.text.contains("boom") {
                    return Err(crate::Error::Engine {
                        engine: "partial".into(),
                        message: "模拟失败".into(),
                    });
                }
                Ok(TranslationResponse {
                    text: format!("[{}]", req.text),
                    detected_source_lang: None,
                    engine: "partial".into(),
                })
            }
        }

        let out = translate_batch_with_cache(
            deps(&PartialFail, &cache, &glossary, true),
            &[req("good1"), req("boom"), req("good2")],
        )
        .await;

        assert_eq!(out.results.len(), 3, "结果必须等长");
        assert!(out.results[0].is_ok(), "失败项之前的不应受影响");
        assert!(out.results[1].is_err(), "失败项应为 Err");
        assert!(out.results[2].is_ok(), "失败项之后的不应被中断");
        assert_eq!(out.results[2].as_ref().unwrap().text, "[good2]");
    }

    /// 关键回归（LESSONS 第 8 条）：双向词条不得把正确译文反向改写。
    ///
    /// 词库含 `apple→苹果` 与 `苹果→Apple`：英译中时源端替换后引擎产出
    /// 「我喜欢苹果」，译文端后替换必须**跳过**已是目标词的「苹果」，
    /// 否则会得到「我喜欢Apple」。
    #[tokio::test]
    async fn test_glossary_reverse_entry_does_not_rewrite_translation() {
        let cache = TranslationCache::in_memory().unwrap();
        let mut glossary = Glossary::default();
        glossary.insert("apple".to_string(), "苹果".to_string());
        glossary.insert("苹果".to_string(), "Apple".to_string());

        struct EchoEngine;
        #[async_trait]
        impl TranslateEngine for EchoEngine {
            fn id(&self) -> &str {
                "echo"
            }
            fn name(&self) -> &str {
                "Echo"
            }
            /// 模拟引擎的真实行为：把 apple 译成「苹果」
            async fn translate(&self, req: &TranslationRequest) -> Result<TranslationResponse> {
                let out = if req.text.contains("apple") {
                    req.text.replace("apple", "苹果")
                } else {
                    req.text.clone()
                };
                Ok(TranslationResponse {
                    text: out,
                    detected_source_lang: None,
                    engine: "echo".into(),
                })
            }
        }

        let out = translate_batch_with_cache(
            deps(&EchoEngine, &cache, &glossary, true),
            &[req("I like apple")],
        )
        .await;

        let text = out.results[0].as_ref().unwrap().text.clone();
        assert!(
            !text.contains("Apple"),
            "译文被反向词条改写为 {text:?}（应保留『苹果』）"
        );
        assert!(text.contains("苹果"), "实际：{text:?}");
    }

    /// 缓存里存的必须是**未经**译文端替换的文本，
    /// 否则用户调整词库后旧缓存会持续输出错误结果。
    ///
    /// ⚠️ 构造要点：引擎的译文必须**能被译文端词条改写**，否则
    /// 「存原文 / 存改写后」两种实现产出一致，测试会退化成摆设
    /// （LESSONS 第 11 条）。故此处词条 `dog→狗` 会命中引擎输出的
    /// `the dog`，使缺陷版（存改写后文本）可被实际观测到。
    #[tokio::test]
    async fn test_cached_text_is_pre_translation_glossary() {
        let cache = TranslationCache::in_memory().unwrap();
        let mut glossary = Glossary::default();
        glossary.insert("cat".to_string(), "猫".to_string());
        // 该词条会命中引擎**译文**中的 "dog"
        glossary.insert("dog".to_string(), "狗".to_string());

        struct FixedEngine;
        #[async_trait]
        impl TranslateEngine for FixedEngine {
            fn id(&self) -> &str {
                "fixed"
            }
            fn name(&self) -> &str {
                "Fixed"
            }
            async fn translate(&self, _req: &TranslationRequest) -> Result<TranslationResponse> {
                Ok(TranslationResponse {
                    text: "the dog".to_string(),
                    detected_source_lang: None,
                    engine: "fixed".into(),
                })
            }
        }

        let out =
            translate_batch_with_cache(deps(&FixedEngine, &cache, &glossary, true), &[req("cat")])
                .await;

        // 先确认译文端替换确实生效了，否则本测试的前提不成立
        assert_eq!(
            out.results[0].as_ref().unwrap().text,
            "the 狗",
            "译文端替换未生效，本测试失去意义"
        );

        // 直接读缓存，验证存的是引擎原文 "the dog" 而非改写后的 "the 狗"
        let key = TranslationCache::generate_key("猫", "auto", "zh-CN", "fixed");
        let stored = cache.get_by_keys(&[key]).unwrap();
        let stored = stored.values().next().expect("缓存未写入");
        assert_eq!(
            stored, "the dog",
            "缓存存了被译文端替换过的文本：{stored:?}"
        );
    }

    /// 超长文本直接失败，不浪费引擎配额。
    #[tokio::test]
    async fn test_oversized_item_fails_without_engine_call() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();
        let received = Arc::new(Mutex::new(Vec::new()));
        let engine = FakeEngine {
            id: "fake".into(),
            received: Arc::clone(&received),
            fail_with: None,
        };

        let out = translate_batch_with_cache(
            deps(&engine, &cache, &glossary, true),
            &[req(&"字".repeat(MAX_ITEM_CHARS + 1))],
        )
        .await;

        assert!(out.results[0].is_err());
        assert!(received.lock().unwrap().is_empty(), "超长项不应送引擎");
    }

    #[tokio::test]
    async fn test_empty_requests() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();
        let engine = FakeEngine {
            id: "fake".into(),
            received: Arc::new(Mutex::new(Vec::new())),
            fail_with: None,
        };
        let out = translate_batch_with_cache(deps(&engine, &cache, &glossary, true), &[]).await;
        assert!(out.results.is_empty());
        assert_eq!(out.cache_hits, 0);
    }

    /// 结果必须与请求**等长且同序**，否则前端按 index 回填会串行错乱。
    #[tokio::test]
    async fn test_results_preserve_order() {
        let cache = TranslationCache::in_memory().unwrap();
        let glossary = Glossary::default();
        let engine = FakeEngine {
            id: "fake".into(),
            received: Arc::new(Mutex::new(Vec::new())),
            fail_with: None,
        };
        let reqs: Vec<TranslationRequest> = (0..5).map(|i| req(&format!("item{i}"))).collect();
        let out = translate_batch_with_cache(deps(&engine, &cache, &glossary, true), &reqs).await;
        assert_eq!(out.results.len(), 5);
        for (i, r) in out.results.iter().enumerate() {
            assert_eq!(r.as_ref().unwrap().text, format!("[item{i}]"));
        }
    }
}
