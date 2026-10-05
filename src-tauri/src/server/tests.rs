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
    assert!(
        !write_result.unwrap(),
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

#[tokio::test]
async fn test_fallback_cache_hit_on_secondary() {
    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    // 使用注入了模拟密钥的 EnginePool，严禁读写开发者的真实 keyring
    let pool = Arc::new(crate::engine_pool::EnginePool::with_secret_getter(
        Arc::new(|key: &str| {
            if key == "deepl" {
                Ok("mock-deepl-key".to_string())
            } else {
                Ok(String::new())
            }
        }),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("valid-token".to_string()));

    // 模拟先前 Fallback 写回了实际成功的 google_web 引擎缓存
    cache
        .set("fallback-test", "降级译文", "auto", "zh-CN", "google_web")
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

    // 使用 deepl 引擎请求，该引擎具备容灾降级能力，可命中 google_web 降级缓存
    let req = Request::builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer valid-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{
                "texts": ["fallback-test"],
                "target_lang": "zh-CN",
                "engine": "deepl"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: TranslateBatchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.translations, vec!["降级译文"]);
    assert_eq!(resp.items[0].engine(), Some("google_web(fallback)"));
}
