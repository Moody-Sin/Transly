use super::*;
use axum::body::Body;
use http_body_util::BodyExt;
use tower::util::ServiceExt;

/// 构造携带合法回环 `Host` 头的请求构建器。
///
/// 真实浏览器发起的请求必然带 `Host` 头，而本地服务的 DNS Rebinding 防线正是
/// 建立在这个头上。测试若省略它，等于把被测防线绕过去了。
fn local_builder() -> axum::http::request::Builder {
    Request::builder().header(axum::http::header::HOST, "127.0.0.1:39281")
}

#[test]
fn test_host_header_allowed() {
    // 合法的回环 Host（带端口 / 不带端口 / IPv6 字面量）
    assert!(host_header_allowed("127.0.0.1:39281"));
    assert!(host_header_allowed("localhost:39281"));
    assert!(host_header_allowed("[::1]:39281"));
    assert!(host_header_allowed("127.0.0.1"));
    assert!(host_header_allowed("localhost"));

    // DNS Rebinding 的核心：Host 指向攻击者域名，必须拒绝
    assert!(!host_header_allowed("evil.com"));
    assert!(!host_header_allowed("evil.com:39281"));
    // 前缀/后缀伪装
    assert!(!host_header_allowed("127.0.0.1.evil.com"));
    assert!(!host_header_allowed("127.0.0.1.evil.com:39281"));
    assert!(!host_header_allowed("localhost.evil.com:39281"));
    assert!(!host_header_allowed("evil-localhost"));
    assert!(!host_header_allowed("[::1].evil.com"));
    assert!(!host_header_allowed("[::1].evil.com:39281"));
    assert!(!host_header_allowed("[::1]evil.com"));
    assert!(!host_header_allowed("[::1]:evil"));
    // 畸形与空值
    assert!(!host_header_allowed(""));
    assert!(!host_header_allowed("   "));
    assert!(!host_header_allowed("[::1"));
    // 端口位不是纯数字时整体视为主机名
    assert!(!host_header_allowed("127.0.0.1:evil"));
}

#[tokio::test]
async fn test_dns_rebinding_host_guard_rejects_foreign_host() {
    let cache = TranslationCache::in_memory().unwrap();
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("test-token".to_string()));
    let app = create_router(token, Arc::new(cache), pool, glossary, cache_controller);

    // `/pair/request` 不带 Token 鉴权，是 rebinding 攻击链的关键一跳：
    // 恶意的同源页面可以直接发起配对请求。Host 守卫必须在鉴权之前拦下它。
    for uri in ["/health", "/pair/request", "/pair/poll"] {
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header(axum::http::header::HOST, "evil.com:39281")
            .header(axum::http::header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))
            .unwrap();

        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "Host 为攻击者域名的 {uri} 必须被拒绝"
        );
    }

    // 缺少 Host 头同样拒绝
    let req = Request::builder()
        .method("POST")
        .uri("/health")
        .body(Body::empty())
        .unwrap();
    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

/// 官方受信任扩展的 Chrome Origin，供各用例复用。
///
/// ID 由 `extension/manifest.json` 中固定 `"key"`（RSA-2048 SPKI 公钥 Base64）
/// 确定性派生：SHA-256(DER) 前 16 字节，每字节高/低 4 位映射到 `a`~`p`。
const OFFICIAL_CHROME_ORIGIN: &str = "chrome-extension://ngfadgochghcplecgmdcjfancoikiihm";

#[test]
fn test_is_allowed_origin() {
    // 仅**白名单内**的官方扩展 Origin 被接受（身份校验，而非格式校验）
    assert!(is_allowed_origin(OFFICIAL_CHROME_ORIGIN));
    assert!(is_allowed_origin(
        "moz-extension://d4e73b40-7988-4e89-a2e6-81525a74e2d3"
    ));

    // Codex Review 回归：任意满足 32 位 a-p 的第三方扩展 ID 必须被严格拒绝。
    // 旧实现只做格式校验，任何恶意扩展都能跨域调用 /pair/request 弹窗诱导配对。
    assert!(
        !is_allowed_origin("chrome-extension://pppppppppppppppppppppppppppppppp"),
        "任意第三方 32 位 a-p 扩展 ID 必须被拒绝"
    );
    assert!(
        !is_allowed_origin("chrome-extension://abcdefghijklmnopabcdefghijklmnop"),
        "格式合法但不在白名单内的扩展 ID 必须被拒绝"
    );

    // 任意随机 UUID 的 Firefox 扩展同样必须被拒绝
    assert!(
        !is_allowed_origin("moz-extension://11111111-2222-3333-4444-555555555555"),
        "任意第三方 UUID 扩展必须被拒绝"
    );

    // 校验 Firefox 构建脚本中声明的 gecko addon ID 必须与服务端 ALLOWED_FIREFOX_EXT_IDS 严格一致
    let firefox_build_script = include_str!("../../../extension/scripts/build-firefox.mjs");
    for allowed_ff_id in ALLOWED_FIREFOX_EXT_IDS {
        assert!(
            firefox_build_script.contains(&format!("{{{allowed_ff_id}}}")),
            "extension/scripts/build-firefox.mjs 必须包含服务端白名单 ID {{{allowed_ff_id}}}"
        );
    }

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
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("test-token".to_string()));
    let app = create_router(token, Arc::new(cache), pool, glossary, cache_controller);
    let req = local_builder().uri("/health").body(Body::empty()).unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cors_origin_policy() {
    let cache = TranslationCache::in_memory().unwrap();
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("test-token".to_string()));
    let app = create_router(token, Arc::new(cache), pool, glossary, cache_controller);

    // 1. 允许官方受信任扩展
    let allowed_req = local_builder()
        .method("OPTIONS")
        .uri("/health")
        .header(header::ORIGIN, OFFICIAL_CHROME_ORIGIN)
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();
    let allowed_res = app.clone().oneshot(allowed_req).await.unwrap();
    assert_eq!(
        allowed_res
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .map(|v| v.to_str().unwrap()),
        Some(OFFICIAL_CHROME_ORIGIN)
    );

    // 2. 拒绝普通不受信外部网页
    let denied_req = local_builder()
        .method("OPTIONS")
        .uri("/health")
        .header(header::ORIGIN, "https://malicious-site.com")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();
    let denied_res = app.clone().oneshot(denied_req).await.unwrap();
    assert!(denied_res
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .is_none());

    // 3. 回归：格式合法但不在白名单内的第三方扩展必须被拒绝。
    //    旧实现只校验 32 位 a-p 字符格式，任意恶意扩展都能通过预检。
    let third_party_req = local_builder()
        .method("OPTIONS")
        .uri("/health")
        .header(
            header::ORIGIN,
            "chrome-extension://pppppppppppppppppppppppppppppppp",
        )
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .body(Body::empty())
        .unwrap();
    let third_party_res = app.oneshot(third_party_req).await.unwrap();
    assert!(
        third_party_res
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none(),
        "第三方扩展 Origin 绝不能被 CORS 放行"
    );
}

#[tokio::test]
async fn test_translate_unauthorized() {
    let cache = TranslationCache::in_memory().unwrap();
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("valid-token".to_string()));
    let app = create_router(token, Arc::new(cache), pool, glossary, cache_controller);
    let req = local_builder()
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
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let mut glossary_instance = Glossary::new();
    glossary_instance.insert("你好", "世界你好");
    let glossary = Arc::new(RwLock::new(glossary_instance));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("valid-token".to_string()));
    let state = ServerState::new(token, pool, cache.clone(), glossary, cache_controller);

    let app = Router::new()
        .route(
            "/translate",
            post(translate_batch_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        .with_state(state);

    let req = local_builder()
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
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));

    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(false));

    let token = Arc::new(RwLock::new("valid-token".to_string()));
    let state = ServerState::new(token, pool, cache.clone(), glossary, cache_controller);

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

    let req = local_builder()
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
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));

    let token = Arc::new(RwLock::new("valid-token".to_string()));
    let state = ServerState::new(token.clone(), pool, cache, glossary, cache_controller);

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
    let invalid_lang_req = local_builder()
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
    let huge_req = local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer valid-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(payload.to_string()))
        .unwrap();
    let res2 = app.clone().oneshot(huge_req).await.unwrap();
    assert_eq!(res2.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // 3. 测试未知非法引擎参数拒绝，杜绝静默降级
    let invalid_engine_req = local_builder()
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

    // 3.5. 测试批量请求中包含重复 ID 时直接拒绝
    let duplicate_id_req = local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer valid-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{"items":[{"id":"1","text":"hello"},{"id":"1","text":"world"}],"target_lang":"zh-CN"}"#,
        ))
        .unwrap();
    let res_dup = app.clone().oneshot(duplicate_id_req).await.unwrap();
    assert_eq!(
        res_dup.status(),
        StatusCode::BAD_REQUEST,
        "重复 ID 必须被 400 拒绝"
    );

    // 4. 测试 Token 轮换：旧 Token 立即返回 401，新 Token 正常访问
    {
        let mut w = token.write().await;
        *w = "rotated-token-123456".to_string();
    }

    // 使用旧 Token 请求 -> 必须 401
    let old_token_req = local_builder()
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
    let new_token_req = local_builder()
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
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("valid-token".to_string()));

    // 预设缓存
    cache
        .set("item-cached", "已缓存译文", "auto", "zh-CN", "google_web")
        .unwrap();

    let state = ServerState::new(token, pool, cache, glossary, cache_controller);

    let app = Router::new()
        .route(
            "/translate",
            post(translate_batch_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        .with_state(state);

    let req = local_builder()
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
    match &resp.items[0] {
        BatchItemResult::Success { id, text, .. } => {
            assert_eq!(id, "node-1");
            assert_eq!(text, "已缓存译文");
        }
        BatchItemResult::Failed { message, .. } => panic!("命中缓存却返回失败项: {message}"),
    }
}

#[tokio::test]
async fn test_translate_structured_batch_items_with_numeric_ids() {
    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("valid-token".to_string()));

    cache
        .set(
            "num-cached",
            "数字ID缓存译文",
            "auto",
            "zh-CN",
            "google_web",
        )
        .unwrap();

    let state = ServerState::new(token, pool, cache, glossary, cache_controller);

    let app = Router::new()
        .route(
            "/translate",
            post(translate_batch_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        .with_state(state);

    let req = local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer valid-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{
                "items": [
                    {"id": 101, "text": "num-cached"},
                    {"id": "str-2", "text": "num-cached"}
                ],
                "target_lang": "zh-CN"
            }"#,
        ))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: TranslateBatchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.items.len(), 2);
    assert_eq!(resp.items[0].id(), "101");
    assert_eq!(resp.items[1].id(), "str-2");
}

#[tokio::test]
async fn test_cache_hit_isolation_without_fallback() {
    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    let pool = Arc::new(crate::engine_pool::EnginePool::new());
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("valid-token".to_string()));

    // 写入 google_web 引擎缓存
    cache
        .set("cache-test", "谷歌独立译文", "auto", "zh-CN", "google_web")
        .unwrap();

    let state = ServerState::new(token, pool, cache, glossary, cache_controller);

    let app = Router::new()
        .route(
            "/translate",
            post(translate_batch_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        .with_state(state);

    // 1. 使用 google_web 引擎请求，正常命中专属缓存并返回原生引擎标识
    let req = local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer valid-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            r#"{
                "texts": ["cache-test"],
                "target_lang": "zh-CN",
                "engine": "google_web"
            }"#,
        ))
        .unwrap();

    let response = app.clone().oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let resp: TranslateBatchResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(resp.translations, vec!["谷歌独立译文"]);
    match &resp.items[0] {
        BatchItemResult::Success { engine, .. } => {
            assert_eq!(engine, "google_web");
        }
        BatchItemResult::Failed { message, .. } => panic!("缓存未命中: {message}"),
    }
}

/// 构建一个仅挂载 `/translate` 的测试应用（含鉴权中间件）。
fn translate_app(
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
) -> Router {
    let state = ServerState::new(
        Arc::new(RwLock::new("valid-token".to_string())),
        pool,
        cache,
        Arc::new(RwLock::new(Glossary::new())),
        Arc::new(crate::cache_controller::CacheController::new(true)),
    );
    Router::new()
        .route(
            "/translate",
            post(translate_batch_handler).layer(middleware::from_fn_with_state(
                state.clone(),
                auth_middleware,
            )),
        )
        .with_state(state)
}

fn post_translate(body: String) -> Request<Body> {
    local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer valid-token")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn test_health_exposes_batch_limits_matching_core_constants() {
    // 回归：扩展端按「项数」切批、服务端按「字符总量」拒绝，两者量纲不同。
    // 客户端只有从 /health 拿到真实上限才能保证发出的每一批都被接受。
    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let token = Arc::new(RwLock::new("test-token".to_string()));
    let app = create_router(
        token,
        cache,
        pool,
        Arc::new(RwLock::new(Glossary::new())),
        Arc::new(crate::cache_controller::CacheController::new(true)),
    );

    let res = app
        .oneshot(local_builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body = res.into_body().collect().await.unwrap().to_bytes();
    // 用 Value 而非 StatusResponse：后者含 `&'static str` 字段，无法从借用数据反序列化
    let status: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // 必须是 core 常量的真实值，不得在服务端另写一份数字
    let max_items = status["max_batch_items"].as_u64().unwrap() as usize;
    let max_total = status["max_batch_total_chars"].as_u64().unwrap() as usize;
    let max_item = status["max_item_chars"].as_u64().unwrap() as usize;
    assert_eq!(max_items, transly_core::MAX_BATCH_ITEMS);
    assert_eq!(max_total, transly_core::MAX_BATCH_TOTAL_CHARS);
    assert_eq!(max_item, transly_core::MAX_ITEM_CHARS);
    assert!(max_items > 0 && max_total > 0);
}

#[tokio::test]
async fn test_batch_over_char_total_is_rejected_while_under_item_limit() {
    // 回归的核心用例：项数未超上限，但字符总量超限 → 必须 413。
    // 这正是旧扩展端「30 项一批」被整批拒绝的形态。
    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let app = translate_app(cache, pool);

    // 项数远低于 MAX_BATCH_ITEMS，但每项够长以致总量越过上限
    let per_item = (transly_core::MAX_BATCH_TOTAL_CHARS / 4) + 1;
    let items: Vec<_> = (0..4)
        .map(|i| serde_json::json!({ "id": format!("n{i}"), "text": "a".repeat(per_item) }))
        .collect();
    assert!(
        items.len() < transly_core::MAX_BATCH_ITEMS,
        "项数必须未超限"
    );

    let payload = serde_json::json!({ "items": items, "target_lang": "zh-CN" });
    let res = app
        .clone()
        .oneshot(post_translate(payload.to_string()))
        .await
        .unwrap();
    assert_eq!(
        res.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "字符总量超限必须拒绝，否则引擎侧会生成超长请求"
    );

    // 对照组：同样项数但总量在上限内 → 必须放行
    let small: Vec<_> = (0..4)
        .map(|i| serde_json::json!({ "id": format!("n{i}"), "text": "a".repeat(10) }))
        .collect();
    let payload = serde_json::json!({ "items": small, "target_lang": "zh-CN" });
    let res = app
        .oneshot(post_translate(payload.to_string()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK, "总量在上限内必须放行");
}

#[tokio::test]
async fn test_batch_item_count_over_limit_is_rejected() {
    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let app = translate_app(cache, pool);

    let items: Vec<_> = (0..=transly_core::MAX_BATCH_ITEMS)
        .map(|i| serde_json::json!({ "id": format!("n{i}"), "text": "hi" }))
        .collect();
    let payload = serde_json::json!({ "items": items, "target_lang": "zh-CN" });
    let res = app
        .oneshot(post_translate(payload.to_string()))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_pairing_session_lifecycle() {
    let pairing_mgr = Arc::new(pairing::PairingManager::new());
    let (session_id, expires_in) = match pairing_mgr
        .create_session("client-1".to_string(), Some("Chrome".to_string()))
        .await
    {
        pairing::CreateSessionOutcome::Created(id, ttl) => (id, ttl),
        other => panic!("首次配对必须成功创建会话，实际 {other:?}"),
    };
    assert_eq!(expires_in, pairing::PAIRING_SESSION_TTL_SECS);

    // 1. 未授权时轮询返回 pending
    let poll_res = pairing_mgr
        .poll_session(&session_id, "client-1", "real-token-123")
        .await;
    assert_eq!(poll_res, pairing::PairingPollResult::Pending);

    // 2. 授权
    let approved = pairing_mgr.approve(&session_id).await;
    assert!(approved);

    // 3. 首次轮询返回 approved 与 token
    let poll_res2 = pairing_mgr
        .poll_session(&session_id, "client-1", "real-token-123")
        .await;
    assert_eq!(
        poll_res2,
        pairing::PairingPollResult::Approved {
            token: "real-token-123".to_string()
        }
    );

    // 4. 一次性消费：二次轮询返回 expired
    let poll_res3 = pairing_mgr
        .poll_session(&session_id, "client-1", "real-token-123")
        .await;
    assert_eq!(poll_res3, pairing::PairingPollResult::Expired);
}

#[tokio::test]
async fn test_pairing_reject_and_client_mismatch() {
    let pairing_mgr = Arc::new(pairing::PairingManager::new());
    let session_id = match pairing_mgr
        .create_session("client-1".to_string(), None)
        .await
    {
        pairing::CreateSessionOutcome::Created(id, _) => id,
        other => panic!("配对必须成功创建会话，实际 {other:?}"),
    };

    // 客户端 ID 不匹配返回 expired
    let poll_mismatch = pairing_mgr
        .poll_session(&session_id, "evil-client", "real-token")
        .await;
    assert_eq!(poll_mismatch, pairing::PairingPollResult::Expired);

    // 重新创建并拒绝
    let session_id2 = match pairing_mgr
        .create_session("client-2".to_string(), None)
        .await
    {
        pairing::CreateSessionOutcome::Created(id, _) => id,
        other => panic!("配对必须成功创建会话，实际 {other:?}"),
    };
    let rejected = pairing_mgr.reject(&session_id2).await;
    assert!(rejected);

    let poll_reject = pairing_mgr
        .poll_session(&session_id2, "client-2", "real-token")
        .await;
    assert_eq!(poll_reject, pairing::PairingPollResult::Rejected);
}

/// 回归：会话表满时**拒绝**新请求，绝不淘汰他人仍在等待的有效会话。
///
/// 复现的缺陷：旧实现按 created_at 淘汰最早的会话。由于 `/pair/request` 位于
/// 鉴权之外（配对本就发生在持有 Token 之前），本机任意进程只要连发
/// MAX_PENDING_SESSIONS 个不同 client_id 的请求，就能把正常浏览器扩展正在等待的
/// 配对会话挤掉 —— 扩展轮询立即收到 Expired，构成配对拒绝服务。
#[tokio::test]
async fn test_pairing_manager_rejects_new_session_instead_of_evicting_existing() {
    let pairing_mgr = Arc::new(pairing::PairingManager::new());
    let mut created = Vec::new();

    // 先让一个「正常扩展」建立会话
    let victim = match pairing_mgr
        .create_session("legit-extension".to_string(), None)
        .await
    {
        pairing::CreateSessionOutcome::Created(id, _) => id,
        other => panic!("正常扩展必须能创建会话，实际 {other:?}"),
    };

    // 恶意进程用不同 client_id 持续灌会话表。
    // 全局限流按窗口突发放行，故此处按窗口节奏推进，模拟「有耐心的攻击者」，
    // 以验证即便攻击者愿意等待，会话表也不会被突破，正常会话更不会被挤掉。
    // 本用例只验证「容量上限不可被突破」，不验证限流节奏（后者由专门的
    // 全局限流用例覆盖）。因此无需真实等待窗口，直接大批量尝试即可：
    // 无论请求如何密集，接受的新会话数都必须被限流与容量双重约束住。
    for i in 0..(pairing::MAX_PENDING_SESSIONS * 4) {
        if let pairing::CreateSessionOutcome::Created(sid, _) = pairing_mgr
            .create_session(format!("attacker-{i}"), None)
            .await
        {
            created.push(sid);
        }
    }

    // 核心保护：无论攻击者如何尝试，接受的新会话数都不得超过容量上限
    assert!(
        created.len() <= pairing::MAX_PENDING_SESSIONS,
        "接受的新会话数不得超过容量上限，实际 {}",
        created.len()
    );

    // 核心断言：正常扩展的会话必须**仍然有效**
    let res = pairing_mgr
        .poll_session(&victim, "legit-extension", "token")
        .await;
    assert_eq!(
        res,
        pairing::PairingPollResult::Pending,
        "正常扩展的等待会话绝不能被泛洪请求挤掉"
    );
}

/// 回归：攻击者用**随机 client_id** 也必须在全局层面被限流。
///
/// 复现的缺陷：仅有按 client_id 的节流可被「每次换一个新 ID」轻易绕过 ——
/// 16 个并发请求即可占满会话表，使正常扩展在随后 60 秒 TTL 内全部被拒，
/// 构成可反复触发的配对拒绝服务。现增加全局滑动窗口限流。
#[tokio::test]
async fn test_pairing_manager_global_throttle_blocks_random_client_id_flood() {
    let pairing_mgr = Arc::new(pairing::PairingManager::new());

    // 窗口内密集请求（每次都换一个全新 client_id）：最多放行 BURST 个，
    // 其余必须被全局限流拦下，而不是逐个放行直到占满会话表。
    let mut created = 0;
    for i in 0..pairing::MAX_PENDING_SESSIONS {
        if matches!(
            pairing_mgr
                .create_session(format!("attacker-{i}"), None)
                .await,
            pairing::CreateSessionOutcome::Created(_, _)
        ) {
            created += 1;
        }
    }

    assert_eq!(
        created,
        pairing::PAIRING_GLOBAL_BURST,
        "同一窗口内只允许突发 {} 个新会话，其余必须被全局限流拦下",
        pairing::PAIRING_GLOBAL_BURST
    );
}

/// 回归：全局限流的稳态容纳量必须**严格小于**会话表容量。
///
/// 复现的缺陷：限流只约束了瞬时突发，未约束「有耐心的攻击者」跨窗口的持续速率。
/// 长期稳态下同时在世的会话数趋近于 `burst / window × TTL`（进入速率 × 存活时长）。
/// 若该值 ≥ `MAX_PENDING_SESSIONS`，攻击者只需按窗口节奏稳定请求约 50 秒即可占满
/// 全部槽位，此后正常浏览器扩展的每一次配对都会收到 429，而攻击者的会话在 TTL 内
/// 持续在位——构成可反复触发的配对拒绝服务。
///
/// 历史参数 `window=10s, burst=3, TTL=60s` 下该值为 18 > 16，
/// 故注释中「灌满 16 个会话至少需要约 50 秒，其间早先的会话已过期」的论断并不成立：
/// 首个会话要到 60 秒才过期，而 50 秒时容量就已打满。
///
/// 本用例只做常量算术断言（不真实等待），因此是确定性的快速测试。
#[test]
fn test_pairing_global_throttle_steady_state_below_capacity() {
    use pairing::{
        MAX_PENDING_SESSIONS, PAIRING_GLOBAL_BURST, PAIRING_GLOBAL_WINDOW_SECS,
        PAIRING_SESSION_TTL_SECS,
    };

    // 稳态上界：进入速率 (burst/window) × 存活时长 (TTL)
    let steady_state_upper_bound =
        PAIRING_GLOBAL_BURST as u64 * PAIRING_SESSION_TTL_SECS / PAIRING_GLOBAL_WINDOW_SECS;

    assert!(
        steady_state_upper_bound < MAX_PENDING_SESSIONS as u64,
        "全局限流的稳态容纳量（{steady_state_upper_bound} = burst {PAIRING_GLOBAL_BURST} \
         / window {PAIRING_GLOBAL_WINDOW_SECS}s × TTL {PAIRING_SESSION_TTL_SECS}s）\
         必须严格小于会话表容量 {MAX_PENDING_SESSIONS}，否则攻击者跨窗口持续请求即可占满\
         全部槽位，令正常扩展在 TTL 内无法配对。请调大 window 或调小 burst。"
    );
}

/// 同一 client_id 在节流窗口内重复请求必须被拒绝，避免配对弹窗轰炸。
#[tokio::test]
async fn test_pairing_manager_throttles_repeated_requests_from_same_client() {
    let pairing_mgr = Arc::new(pairing::PairingManager::new());

    // 首次请求成功
    assert!(matches!(
        pairing_mgr
            .create_session("flooder".to_string(), None)
            .await,
        pairing::CreateSessionOutcome::Created(_, _)
    ));

    // 紧随其后的重复请求必须被节流
    assert_eq!(
        pairing_mgr
            .create_session("flooder".to_string(), None)
            .await,
        pairing::CreateSessionOutcome::Throttled,
        "同一 client_id 的密集重复请求必须被节流"
    );
}

/// 同一 client_id 重复请求应复用既有会话，而不是让扩展把自己的会话作废。
#[tokio::test]
async fn test_pairing_manager_reuses_existing_session_for_same_client() {
    let pairing_mgr = Arc::new(pairing::PairingManager::new());

    let first = match pairing_mgr
        .create_session("client-x".to_string(), None)
        .await
    {
        pairing::CreateSessionOutcome::Created(id, _) => id,
        other => panic!("首次创建应成功，实际 {other:?}"),
    };

    // 越过节流窗口后再请求，应拿到同一个 session_id（复用而非新建）
    tokio::time::sleep(std::time::Duration::from_secs(
        pairing::PAIRING_MIN_REQUEST_INTERVAL_SECS + 1,
    ))
    .await;

    let second = match pairing_mgr
        .create_session("client-x".to_string(), None)
        .await
    {
        pairing::CreateSessionOutcome::Created(id, _) => id,
        other => panic!("复用请求应返回会话，实际 {other:?}"),
    };

    assert_eq!(first, second, "同一 client_id 必须复用既有未决会话");
}

#[tokio::test]
async fn test_pairing_http_endpoints_roundtrip() {
    let cache = TranslationCache::in_memory().unwrap();
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let glossary = Arc::new(RwLock::new(Glossary::new()));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("secret-pairing-token".to_string()));
    let pairing_manager = Arc::new(pairing::PairingManager::new());

    let app = create_router_with_pairing(
        token,
        Arc::new(cache),
        pool,
        glossary,
        cache_controller,
        pairing_manager.clone(),
        None,
    );

    // 1. 发起配对申请
    let req_body = serde_json::json!({
        "client_id": "test-ext-1",
        "browser_hint": "Chrome"
    });
    let req = local_builder()
        .method("POST")
        .uri("/pair/request")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, OFFICIAL_CHROME_ORIGIN)
        .body(Body::from(req_body.to_string()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let data: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let session_id = data["session_id"].as_str().unwrap().to_string();
    assert!(!session_id.is_empty());

    // 2. 轮询待确认状态
    let poll_body = serde_json::json!({
        "session_id": session_id,
        "client_id": "test-ext-1"
    });
    let poll_req = local_builder()
        .method("POST")
        .uri("/pair/poll")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, OFFICIAL_CHROME_ORIGIN)
        .body(Body::from(poll_body.to_string()))
        .unwrap();
    let poll_res = app.clone().oneshot(poll_req).await.unwrap();
    assert_eq!(poll_res.status(), StatusCode::OK);
    let poll_bytes = poll_res.into_body().collect().await.unwrap().to_bytes();
    let poll_data: serde_json::Value = serde_json::from_slice(&poll_bytes).unwrap();
    assert_eq!(poll_data["status"], "pending");

    // 3. 桌面端批准
    assert!(pairing_manager.approve(&session_id).await);

    // 4. 再次轮询成功获得 Token
    let poll_req2 = local_builder()
        .method("POST")
        .uri("/pair/poll")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, OFFICIAL_CHROME_ORIGIN)
        .body(Body::from(poll_body.to_string()))
        .unwrap();
    let poll_res2 = app.clone().oneshot(poll_req2).await.unwrap();
    assert_eq!(poll_res2.status(), StatusCode::OK);
    let poll_bytes2 = poll_res2.into_body().collect().await.unwrap().to_bytes();
    let poll_data2: serde_json::Value = serde_json::from_slice(&poll_bytes2).unwrap();
    assert_eq!(poll_data2["status"], "approved");
    assert_eq!(poll_data2["token"], "secret-pairing-token");
}

#[tokio::test]
async fn test_e2e_extension_batch_translate_and_fallback_flow() {
    // 搭建真实路由服务端拓扑
    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let mut glossary_instance = Glossary::new();
    glossary_instance.insert_with_options("agent", "智能体", false);
    let glossary = Arc::new(RwLock::new(glossary_instance));
    let glossary_for_test = glossary.clone();
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("e2e-session-token".to_string()));

    let app = create_router(token, cache.clone(), pool, glossary, cache_controller);

    // 1. 模拟 Chrome 浏览器插件发起一整批 DOM 文本节点的沉浸式翻译请求
    let batch_req_body = serde_json::json!({
        "items": [
            { "id": "node_1", "text": "Hello world with Agent" },
            { "id": "node_2", "text": "Another paragraph for testing" }
        ],
        "source_lang": "auto",
        "target_lang": "zh-CN",
        "engine": "google_web"
    });

    let req = local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer e2e-session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, OFFICIAL_CHROME_ORIGIN)
        .body(Body::from(batch_req_body.to_string()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let resp: TranslateBatchResponse = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(resp.items.len(), 2);

    // 验证翻译结果结构化返回，并经过了忽略大小写的全局术语库修正 ("Agent" -> "智能体")
    if let BatchItemResult::Success { ref text, .. } = resp.items[0] {
        assert!(text.contains("智能体"), "术语替换应生效: {text}");
    } else {
        panic!("第一项应为成功结果");
    }

    // 2. 验证写入缓存使用的是批量事务 set_batch
    let cached_1 = cache
        .get("Hello world with Agent", "auto", "zh-CN", "google_web")
        .unwrap();
    let cached_2 = cache
        .get(
            "Another paragraph for testing",
            "auto",
            "zh-CN",
            "google_web",
        )
        .unwrap();
    assert!(cached_1.is_some(), "第一项应已被写入缓存");
    assert!(cached_2.is_some(), "第二项应已被写入缓存");

    // 3. 再次发起相同批量请求：验证 100% 命中缓存且毫秒级响应
    let req2 = local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer e2e-session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, OFFICIAL_CHROME_ORIGIN)
        .body(Body::from(batch_req_body.to_string()))
        .unwrap();

    let res2 = app.clone().oneshot(req2).await.unwrap();
    assert_eq!(res2.status(), StatusCode::OK);
    let bytes2 = res2.into_body().collect().await.unwrap().to_bytes();
    let resp2: TranslateBatchResponse = serde_json::from_slice(&bytes2).unwrap();
    assert_eq!(resp2.items.len(), 2);
    // 4. 验证当用户新增源语言术语时，即使旧原文已在缓存中，新术语依然能穿透旧缓存立即生效
    {
        let mut g = glossary_for_test.write().await;
        g.insert_with_options("World", "宇宙", false);
    }
    let req3 = local_builder()
        .method("POST")
        .uri("/translate")
        .header(header::AUTHORIZATION, "Bearer e2e-session-token")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ORIGIN, OFFICIAL_CHROME_ORIGIN)
        .body(Body::from(batch_req_body.to_string()))
        .unwrap();
    let res3 = app.clone().oneshot(req3).await.unwrap();
    assert_eq!(res3.status(), StatusCode::OK);
    let bytes3 = res3.into_body().collect().await.unwrap().to_bytes();
    let resp3: TranslateBatchResponse = serde_json::from_slice(&bytes3).unwrap();
    if let BatchItemResult::Success { ref text, .. } = resp3.items[0] {
        assert!(
            text.contains("宇宙") && text.contains("智能体"),
            "新增源语言术语后，不得被旧的未替换缓存屏蔽: {text}"
        );
    } else {
        panic!("第一项应为成功结果");
    }
}

#[tokio::test]
async fn test_real_tcp_server_e2e_lifecycle() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let cache = Arc::new(TranslationCache::in_memory().unwrap());
    let pool = Arc::new(crate::engine_pool::EnginePool::with_default_engine(
        Arc::new(crate::engine_pool::testing::StubEngine),
    ));
    let mut glossary_instance = Glossary::new();
    glossary_instance.insert_with_options("pipeline", "流水线", false);
    let glossary = Arc::new(RwLock::new(glossary_instance));
    let cache_controller = Arc::new(crate::cache_controller::CacheController::new(true));
    let token = Arc::new(RwLock::new("real-tcp-secret-token".to_string()));
    let pairing_manager = Arc::new(pairing::PairingManager::new());

    // 绑定真实的操作系统本地回环 TCP 端口（由操作系统动态分配可用端口）
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("绑定测试 TCP 回环端口失败");
    let port = listener.local_addr().unwrap().port();

    let app = create_router_with_pairing(
        token.clone(),
        cache.clone(),
        pool,
        glossary,
        cache_controller,
        pairing_manager.clone(),
        None,
    );

    // 在后台任务中启动真实 Axum HTTP 服务器
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let send_raw = |req: String| async move {
        let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .expect("TCP 连接服务端失败");
        stream
            .write_all(req.as_bytes())
            .await
            .expect("写入请求失败");
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .expect("读取响应失败");
        let resp_str = String::from_utf8_lossy(&response).to_string();
        let status = resp_str
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(0);
        (status, resp_str)
    };

    // 1. 真实 TCP 网络握手与 /health 探针
    let req_health =
        format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    let (status, body) = send_raw(req_health).await;
    assert_eq!(status, 200, "/health 必须返回 200: {body}");
    assert!(body.contains(r#""status":"ok""#), "必须包含 status:ok");
    assert!(
        body.contains("max_batch_items"),
        "必须公开批量上限供客户端切批"
    );

    // 2. 真实 TCP 下防御 DNS Rebinding：恶意 Host 头必须在底层被强制阻断为 403
    let req_rebinding = format!(
        "GET /health HTTP/1.1\r\nHost: attacker-domain.com:{port}\r\nConnection: close\r\n\r\n"
    );
    let (status_rebinding, _) = send_raw(req_rebinding).await;
    assert_eq!(
        status_rebinding, 403,
        "非回环 Host 必须在真实 TCP 连接中被 403 拒绝"
    );

    // 3. 真实配对握手全流程 (POST /pair/request -> 轮询 pending -> approve -> 轮询获取 token)
    let pair_body = r#"{"client_id":"playwright-ext","browser_hint":"Edge"}"#;
    let req_pair = format!(
        "POST /pair/request HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        pair_body.len(),
        pair_body
    );
    let (pair_status, pair_resp) = send_raw(req_pair).await;
    assert_eq!(pair_status, 200, "配对申请必须返回 200: {pair_resp}");
    let session_id_start = pair_resp
        .find(r#""session_id":""#)
        .expect("响应需包含 session_id")
        + 14;
    let session_id_end = pair_resp[session_id_start..].find('"').unwrap() + session_id_start;
    let session_id = &pair_resp[session_id_start..session_id_end];

    // 轮询当前状态（未批准前为 pending）
    let poll_body = format!(r#"{{"session_id":"{session_id}","client_id":"playwright-ext"}}"#);
    let req_poll_pending = format!(
        "POST /pair/poll HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        poll_body.len(),
        poll_body
    );
    let (poll_pending_status, poll_pending_resp) = send_raw(req_poll_pending).await;
    assert_eq!(poll_pending_status, 200);
    assert!(poll_pending_resp.contains(r#""status":"pending""#));

    // 服务端批准该会话（模拟桌面端用户点击“允许配对”）
    let approve_ok = pairing_manager.approve(session_id).await;
    assert!(approve_ok, "服务端批准配对会话应成功");

    // 再次轮询（已批准，必须签发正确的 Bearer Token）
    let req_poll_approved = format!(
        "POST /pair/poll HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        poll_body.len(),
        poll_body
    );
    let (poll_appr_status, poll_appr_resp) = send_raw(req_poll_approved).await;
    assert_eq!(poll_appr_status, 200);
    assert!(poll_appr_resp.contains(r#""status":"approved""#));
    assert!(
        poll_appr_resp.contains("real-tcp-secret-token"),
        "必须返回对应服务端 Token"
    );

    // 4. 携带错误 Token 访问 /translate 必须返回 401
    let trans_body = r#"{"items":[{"id":"1","text":"Test Pipeline"}],"target_lang":"zh-CN"}"#;
    let req_unauth = format!(
        "POST /translate HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer wrong-token\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        trans_body.len(),
        trans_body
    );
    let (unauth_status, _) = send_raw(req_unauth).await;
    assert_eq!(unauth_status, 401, "错误 Token 必须返回 401 Unauthorized");

    // 5. 携带正确 Token 发送批量翻译，验证跨进程真实 TCP 往返、术语库干预与结构化响应
    let req_auth = format!(
        "POST /translate HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer real-tcp-secret-token\r\nOrigin: {OFFICIAL_CHROME_ORIGIN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        trans_body.len(),
        trans_body
    );
    let (auth_status, auth_resp) = send_raw(req_auth).await;
    assert_eq!(auth_status, 200, "授权请求必须返回 200: {auth_resp}");
    assert!(
        auth_resp.contains("流水线"),
        "术语库必须生效且在译文中体现: {auth_resp}"
    );
    assert!(
        auth_resp.contains(r#""status":"success""#),
        "响应必须包含成功状态项"
    );

    // 清理后台服务器
    server_handle.abort();
}
