use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use transly_core::{
    engine::{BatchItemRequest, BatchItemResult},
    error::ErrorKind,
    Glossary, TranslationCache, TranslationRequest,
};

#[derive(Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Deserialize)]
pub struct TranslateBatchRequest {
    /// 兼容旧版：纯文本数组
    #[serde(default)]
    pub texts: Vec<String>,
    /// 新增契约：带有明确 stable ID 的批量项目
    #[serde(default)]
    pub items: Vec<BatchItemRequest>,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub engine: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TranslateBatchResponse {
    /// 兼容旧版响应格式：直接保持译文字符串列表
    pub translations: Vec<String>,
    /// 结构化逐项反馈（包含 id, 状态, 真实引擎或错误分类）
    #[serde(default)]
    pub items: Vec<BatchItemResult>,
}


#[derive(Clone)]
pub struct ServerState {
    pub token: Arc<RwLock<String>>,
    pub pool: Arc<crate::engine_pool::EnginePool>,
    pub cache: Arc<TranslationCache>,
    pub glossary: Arc<RwLock<Glossary>>,
    pub concurrency_limit: Arc<tokio::sync::Semaphore>,
    pub cache_controller: Arc<crate::cache_controller::CacheController>,
}

fn is_allowed_origin(origin_str: &str) -> bool {
    // 1. Chrome 扩展校验：chrome-extension://<32位小写a-p字符扩展ID>
    if let Some(id) = origin_str.strip_prefix("chrome-extension://") {
        if id.len() == 32 && id.bytes().all(|c| (b'a'..=b'p').contains(&c)) {
            return true;
        }
    }

    // 2. Firefox 扩展校验：moz-extension://<标准UUID扩展ID>
    if let Some(id) = origin_str.strip_prefix("moz-extension://") {
        let bytes = id.as_bytes();
        if bytes.len() == 36
            && [8, 13, 18, 23].into_iter().all(|i| bytes[i] == b'-')
            && bytes
                .iter()
                .enumerate()
                .all(|(i, &c)| [8, 13, 18, 23].contains(&i) || c.is_ascii_hexdigit())
        {
            return true;
        }
    }

    // 3. 仅在开发调试环境下允许本地回环端口
    #[cfg(debug_assertions)]
    {
        if let Ok(uri) = origin_str.parse::<axum::http::Uri>() {
            if let Some(host) = uri.host() {
                return host == "127.0.0.1" || host == "localhost";
            }
        }
    }

    false
}

pub fn create_router(
    token: Arc<RwLock<String>>,
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
    cache_controller: Arc<crate::cache_controller::CacheController>,
) -> Router {
    let state = ServerState {
        token,
        pool,
        cache,
        glossary,
        concurrency_limit: Arc::new(tokio::sync::Semaphore::new(10)), // 限制本地并发处理至多 10 批
        cache_controller,
    };

    let protected = Router::new()
        .route("/translate", post(translate_batch_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(
            |origin: &axum::http::HeaderValue, _parts| {
                if let Ok(s) = origin.to_str() {
                    is_allowed_origin(s)
                } else {
                    false
                }
            },
        ))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    Router::new()
        .route("/health", get(health_handler))
        .merge(protected)
        .layer(cors)
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024)) // 限制单次 HTTP 请求体最大 2MB
        .with_state(state)
}

async fn auth_middleware(
    State(state): State<ServerState>,
    headers: HeaderMap,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let current_token = state.token.read().await;
    match auth_header {
        Some(val) if val == format!("Bearer {}", current_token.as_str()) => {
            drop(current_token);
            Ok(next.run(request).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

async fn health_handler() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn translate_batch_handler(
    State(state): State<ServerState>,
    Json(payload): Json<TranslateBatchRequest>,
) -> Result<Json<TranslateBatchResponse>, StatusCode> {
    // 归一化输入：若提供了 items 则优先使用 items，否则由 texts 构造虚拟 index ID
    let unified_items: Vec<BatchItemRequest> = if !payload.items.is_empty() {
        payload.items
    } else {
        payload
            .texts
            .into_iter()
            .enumerate()
            .map(|(i, text)| BatchItemRequest {
                id: i.to_string(),
                text,
            })
            .collect()
    };

    // 限制单次批量文本数量与总字符数，防御 DoS 与过载攻击
    if unified_items.is_empty() || unified_items.len() > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let total_chars: usize = unified_items.iter().map(|t| t.text.chars().count()).sum();
    if total_chars > 50_000 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    for t in &unified_items {
        if t.text.chars().count() > 10_000 {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }

    let target = payload.target_lang;
    let source = payload.source_lang.unwrap_or_else(|| "auto".to_string());

    // 强校验语言参数代码，拒绝异常或恶意超长代码
    if !transly_core::validate_language_code(&target)
        || target.eq_ignore_ascii_case("auto")
        || (!source.is_empty() && !transly_core::validate_language_code(&source))
    {
        return Err(StatusCode::BAD_REQUEST);
    }

    // 强校验引擎名称合法性，杜绝未知引擎静默降级掩盖调用错误
    if let Some(ref eng) = payload.engine {
        if !transly_core::validate_engine_id(eng) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // 获取并发令牌（超载直接返回 429 Too Many Requests，杜绝无限排队造成内存堆积）
    let _permit = state
        .concurrency_limit
        .try_acquire()
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)?;

    // 支持按请求参数动态选择引擎，未指定则使用默认引擎
    let active_engine = if let Some(ref eng) = payload.engine {
        state.pool.get_engine(eng).await
    } else {
        state.pool.default_engine()
    };
    let engine_id = active_engine.id();

    // 检查缓存状态快照与 generation 版本，杜绝并发关闭清空后的写回竞争
    let (cache_enabled, start_generation) = state.cache_controller.snapshot();

    let mut item_results: Vec<Option<BatchItemResult>> = vec![None; unified_items.len()];
    let mut missing_indices = Vec::new();
    let mut missing_requests = Vec::new();

    // 1. 若开启缓存，优先查缓存
    if cache_enabled {
        for (idx, item) in unified_items.iter().enumerate() {
            if let Ok(Some(cached_text)) = state.cache.get(&item.text, &source, &target, engine_id)
            {
                item_results[idx] = Some(BatchItemResult::Success {
                    id: item.id.clone(),
                    text: cached_text,
                    detected_source_lang: None,
                    engine: engine_id.to_string(),
                });
            } else {
                missing_indices.push(idx);
                missing_requests.push(TranslationRequest {
                    text: item.text.clone(),
                    source_lang: source.clone(),
                    target_lang: target.clone(),
                });
            }
        }
    } else {
        for (idx, item) in unified_items.iter().enumerate() {
            missing_indices.push(idx);
            missing_requests.push(TranslationRequest {
                text: item.text.clone(),
                source_lang: source.clone(),
                target_lang: target.clone(),
            });
        }
    }

    // 2. 对未命中缓存的项发起批量引擎翻译并写回缓存
    if !missing_requests.is_empty() {
        match active_engine.translate_batch(&missing_requests).await {
            Ok(results) => {
                // 安全防范：验证引擎返回条数与请求条数完全一致
                if results.len() != missing_requests.len() {
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
                for (req_idx, res) in results.into_iter().enumerate() {
                    let original_idx = missing_indices[req_idx];
                    let original_item = &unified_items[original_idx];
                    // 写回时通过同一同步门禁 set_if_current 校验版本并执行写入
                    let actual_engine = res.actual_engine();
                    if let Err(e) = state.cache_controller.set_if_current(start_generation, || {
                        state.cache.set(
                            &original_item.text,
                            &res.text,
                            &source,
                            &target,
                            actual_engine,
                        )
                    }) {
                        eprintln!("[Cache Write Warning] 扩展批量翻译缓存写入失败: {e}");
                    }
                    item_results[original_idx] = Some(BatchItemResult::Success {
                        id: original_item.id.clone(),
                        text: res.text,
                        detected_source_lang: res.detected_source_lang,
                        engine: res.engine,
                    });
                }
            }
            Err(e) => {
                // 逐项标记失败，支持部分失败结构化反馈
                let kind = e.kind();
                let msg = e.to_string();
                for &orig_idx in &missing_indices {
                    let original_item = &unified_items[orig_idx];
                    item_results[orig_idx] = Some(BatchItemResult::Failed {
                        id: original_item.id.clone(),
                        error_kind: kind,
                        message: msg.clone(),
                    });
                }
            }
        }
    }

    // 3. 统一应用全局术语库修正，并组装最终的 BatchItemResult 与旧版兼容 translations 列表
    let g = state.glossary.read().await;
    let mut final_items = Vec::with_capacity(unified_items.len());
    let mut final_translations = Vec::with_capacity(unified_items.len());

    for (idx, opt_res) in item_results.into_iter().enumerate() {
        let mut item = opt_res.unwrap_or_else(|| BatchItemResult::Failed {
            id: unified_items[idx].id.clone(),
            error_kind: ErrorKind::Internal,
            message: "未处理的项".to_string(),
        });

        if let BatchItemResult::Success { ref mut text, .. } = item {
            if !g.is_empty() {
                *text = g.apply(text);
            }
            final_translations.push(text.clone());
        } else {
            // 失败项在旧版 translations 数组中保持空串或原文占位
            final_translations.push(String::new());
        }
        final_items.push(item);
    }

    Ok(Json(TranslateBatchResponse {
        translations: final_translations,
        items: final_items,
    }))
}


pub async fn start_server(
    port: u16,
    token: Arc<RwLock<String>>,
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
    cache_controller: Arc<crate::cache_controller::CacheController>,
) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(
        listener,
        create_router(token, cache, pool, glossary, cache_controller),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    #[test]
    fn test_is_allowed_origin() {
        // 合法的扩展 Origin 格式
        assert!(is_allowed_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop"
        ));
        assert!(is_allowed_origin(
            "moz-extension://d4e73b40-7988-4e89-a2e6-81525a74e2d3"
        ));

        // 拒绝格式不正确的扩展 ID、伪装前缀与外部网页
        assert!(!is_allowed_origin("chrome-extension://abcdefg"));
        assert!(!is_allowed_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnox"
        ));
        assert!(!is_allowed_origin(
            "moz-extension://d4e73b40-7988-4e89-a2e6-81525a74e2d"
        ));
        assert!(!is_allowed_origin("chrome-extension://"));
        assert!(!is_allowed_origin("https://evil.com"));
        assert!(!is_allowed_origin("http://localhost.evil.com"));
    }

    #[tokio::test]
    async fn test_health_check() {
        let cache = TranslationCache::in_memory().unwrap();
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let glossary = Arc::new(RwLock::new(Glossary::new()));
        let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
        let token = Arc::new(RwLock::new("test-token".to_string()));
        let app = create_router(token, Arc::new(cache), pool, glossary, cache_controller);
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cors_origin_policy() {
        let cache = TranslationCache::in_memory().unwrap();
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let glossary = Arc::new(RwLock::new(Glossary::new()));
        let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
        let token = Arc::new(RwLock::new("test-token".to_string()));
        let app = create_router(token, Arc::new(cache), pool, glossary, cache_controller);

        // 1. 允许扩展协议与本地开发
        let allowed_req = Request::builder()
            .method("OPTIONS")
            .uri("/health")
            .header(
                header::ORIGIN,
                "chrome-extension://abcdefghijklmnopabcdefghijklmnop",
            )
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .unwrap();
        let allowed_res = app.clone().oneshot(allowed_req).await.unwrap();
        assert_eq!(
            allowed_res
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .map(|v| v.to_str().unwrap()),
            Some("chrome-extension://abcdefghijklmnopabcdefghijklmnop")
        );

        // 2. 拒绝普通不受信外部网页
        let denied_req = Request::builder()
            .method("OPTIONS")
            .uri("/health")
            .header(header::ORIGIN, "https://malicious-site.com")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .unwrap();
        let denied_res = app.oneshot(denied_req).await.unwrap();
        assert!(denied_res
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }

    #[tokio::test]
    async fn test_translate_unauthorized() {
        let cache = TranslationCache::in_memory().unwrap();
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let glossary = Arc::new(RwLock::new(Glossary::new()));
        let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
        let token = Arc::new(RwLock::new("valid-token".to_string()));
        let app = create_router(token, Arc::new(cache), pool, glossary, cache_controller);
        let req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"texts":["hello"],"target_lang":"zh-CN"}"#))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_translate_authorized() {
        let cache = Arc::new(TranslationCache::in_memory().unwrap());
        // 预写入缓存避免测试发出真实网络请求
        cache
            .set("hello", "你好", "auto", "zh-CN", "google_web")
            .unwrap();
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let mut glossary_instance = Glossary::new();
        glossary_instance.insert("你好", "世界你好");
        let glossary = Arc::new(RwLock::new(glossary_instance));
        let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
        let token = Arc::new(RwLock::new("valid-token".to_string()));
        let state = ServerState {
            token,
            pool,
            cache: cache.clone(),
            glossary,
            concurrency_limit: Arc::new(tokio::sync::Semaphore::new(10)),
            cache_controller,
        };

        let app = Router::new()
            .route(
                "/translate",
                post(translate_batch_handler).layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
            )
            .with_state(state);

        let req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer valid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"texts":["hello"],"target_lang":"zh-CN"}"#))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let resp: TranslateBatchResponse = serde_json::from_slice(&body).unwrap();
        // 验证经过了术语库替换 ("你好" -> "世界你好")
        assert_eq!(resp.translations, vec!["世界你好"]);

        // 验证翻译结果已自动写入缓存（缓存存的是引擎原始译文）
        let cached = cache.get("hello", "auto", "zh-CN", "google_web").unwrap();
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_translate_cache_disabled_path() {
        let cache = Arc::new(TranslationCache::in_memory().unwrap());
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let glossary = Arc::new(RwLock::new(Glossary::new()));

        let cache_controller = Arc::new(crate::cache_controller::CacheController::new(false));

        let token = Arc::new(RwLock::new("valid-token".to_string()));
        let state = ServerState {
            token,
            pool,
            cache: cache.clone(),
            glossary,
            concurrency_limit: Arc::new(tokio::sync::Semaphore::new(10)),
            cache_controller,
        };

        let app = Router::new()
            .route(
                "/translate",
                post(translate_batch_handler).layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
            )
            .with_state(state);

        // 预设即使缓存里已有旧词条
        cache
            .set("privacy_test", "旧译文", "auto", "zh-CN", "google_web")
            .unwrap();

        let req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer valid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"texts":["fresh_text"],"target_lang":"zh-CN"}"#,
            ))
            .unwrap();

        // 验证在关闭缓存时，不会写入新条目
        let _ = app.oneshot(req).await;
        let fresh_cached = cache
            .get("fresh_text", "auto", "zh-CN", "google_web")
            .unwrap();
        assert_eq!(fresh_cached, None, "关闭缓存时绝对不可向 SQLite 写入新条目");
    }

    #[tokio::test]
    async fn test_translate_language_and_length_validation() {
        let cache = Arc::new(TranslationCache::in_memory().unwrap());
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let glossary = Arc::new(RwLock::new(Glossary::new()));
        let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));

        let token = Arc::new(RwLock::new("valid-token".to_string()));
        let state = ServerState {
            token: token.clone(),
            pool,
            cache,
            glossary,
            concurrency_limit: Arc::new(tokio::sync::Semaphore::new(10)),
            cache_controller,
        };

        let app = Router::new()
            .route(
                "/translate",
                post(translate_batch_handler).layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
            )
            .with_state(state);

        // 1. 测试非法语言参数拒绝 (target_lang 为 auto 或 包含非法字符)
        let invalid_lang_req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer valid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"texts":["hello"],"target_lang":"auto"}"#))
            .unwrap();
        let res = app.clone().oneshot(invalid_lang_req).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);

        // 2. 测试总字符数超限拒绝 (超过 50,000 字符)
        let huge_text = "a".repeat(50_001);
        let payload = serde_json::json!({
            "texts": [huge_text],
            "target_lang": "zh-CN"
        });
        let huge_req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer valid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let res2 = app.clone().oneshot(huge_req).await.unwrap();
        assert_eq!(res2.status(), StatusCode::PAYLOAD_TOO_LARGE);

        // 3. 测试未知非法引擎参数拒绝，杜绝静默降级
        let invalid_engine_req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer valid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{"texts":["hello"],"target_lang":"zh-CN","engine":"unknown_provider"}"#,
            ))
            .unwrap();
        let res3 = app.clone().oneshot(invalid_engine_req).await.unwrap();
        assert_eq!(res3.status(), StatusCode::BAD_REQUEST);

        // 4. 测试 Token 轮换：旧 Token 立即返回 401，新 Token 正常访问
        {
            let mut w = token.write().await;
            *w = "rotated-token-123456".to_string();
        }

        // 使用旧 Token 请求 -> 必须 401
        let old_token_req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer valid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"texts":["hello"],"target_lang":"zh-CN"}"#))
            .unwrap();
        let res_old = app.clone().oneshot(old_token_req).await.unwrap();
        assert_eq!(
            res_old.status(),
            StatusCode::UNAUTHORIZED,
            "Token 轮换后旧 Token 必须被立即拒绝"
        );

        // 使用新 Token 请求 -> 必须 200
        let new_token_req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer rotated-token-123456")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"texts":["hello"],"target_lang":"zh-CN"}"#))
            .unwrap();
        let res_new = app.oneshot(new_token_req).await.unwrap();
        assert_eq!(
            res_new.status(),
            StatusCode::OK,
            "新 Token 必须正常鉴权通过"
        );
    }

    #[tokio::test]
    async fn test_cache_disable_and_clear_synchronization_boundary() {
        let cache = Arc::new(TranslationCache::in_memory().unwrap());
        let controller = Arc::new(crate::cache_controller::CacheController::new(true));

        // 1. 模拟请求 1 开始时获取快照
        let (enabled, start_gen) = controller.snapshot();
        assert!(enabled);

        // 2. 模拟用户在请求 1 进行中关闭缓存并清空
        let clear_res = controller.disable_and_clear(cache.as_ref());
        assert!(clear_res.is_ok());
        assert!(!controller.is_enabled());

        // 3. 模拟请求 1 试图写入缓存 -> 必须被安全阻断拒绝
        let write_result = controller.set_if_current(start_gen, || {
            cache.set("concurrent_text", "译文", "en", "zh-CN", "google_web")
        });
        assert_eq!(
            write_result.unwrap(),
            false,
            "请求跨越关闭清空边界时，set_if_current 必须返回 Ok(false) 拒绝写入"
        );

        // 4. 强断言数据库依然保持完全为空
        let read_back = cache
            .get("concurrent_text", "en", "zh-CN", "google_web")
            .unwrap();
        assert_eq!(
            read_back, None,
            "数据库中绝对不得残留关闭前发起的在途请求译文"
        );
    }

    #[tokio::test]
    async fn test_translate_structured_batch_items() {
        let cache = Arc::new(TranslationCache::in_memory().unwrap());
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let glossary = Arc::new(RwLock::new(Glossary::new()));
        let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
        let token = Arc::new(RwLock::new("valid-token".to_string()));

        // 预设缓存
        cache
            .set("item-cached", "已缓存译文", "auto", "zh-CN", "google_web")
            .unwrap();

        let state = ServerState {
            token,
            pool,
            cache,
            glossary,
            concurrency_limit: Arc::new(tokio::sync::Semaphore::new(10)),
            cache_controller,
        };

        let app = Router::new()
            .route(
                "/translate",
                post(translate_batch_handler).layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth_middleware,
                )),
            )
            .with_state(state);

        let req = Request::builder()
            .method("POST")
            .uri("/translate")
            .header(header::AUTHORIZATION, "Bearer valid-token")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                r#"{
                    "items": [
                        {"id": "node-1", "text": "item-cached"}
                    ],
                    "target_lang": "zh-CN"
                }"#,
            ))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let resp: TranslateBatchResponse = serde_json::from_slice(&body).unwrap();
        assert_eq!(resp.translations, vec!["已缓存译文"]);
        assert_eq!(resp.items.len(), 1);
        assert_eq!(resp.items[0].id(), "node-1");
        assert!(resp.items[0].is_success());
    }

}
