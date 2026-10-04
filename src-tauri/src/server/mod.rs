use axum::{
    extract::{Request, State},
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
use tower_http::cors::{Any, CorsLayer};
use transly_core::{Glossary, TranslationCache, TranslationRequest};

#[derive(Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Deserialize)]
pub struct TranslateBatchRequest {
    pub texts: Vec<String>,
    pub source_lang: Option<String>,
    pub target_lang: String,
    pub engine: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TranslateBatchResponse {
    pub translations: Vec<String>,
}

#[derive(Clone)]
pub struct ServerState {
    pub token: Arc<String>,
    pub pool: Arc<crate::engine_pool::EnginePool>,
    pub cache: Arc<TranslationCache>,
    pub glossary: Arc<RwLock<Glossary>>,
}

pub fn create_router(
    token: String,
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
) -> Router {
    let state = ServerState {
        token: Arc::new(token),
        pool,
        cache,
        glossary,
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
                    s.starts_with("chrome-extension://")
                        || s.starts_with("moz-extension://")
                        || s.starts_with("http://127.0.0.1")
                        || s.starts_with("http://localhost")
                } else {
                    false
                }
            },
        ))
        .allow_methods(Any)
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    Router::new()
        .route("/health", get(health_handler))
        .merge(protected)
        .layer(cors)
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

    match auth_header {
        Some(val) if val == format!("Bearer {}", state.token.as_str()) => {
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
    let target = payload.target_lang;
    let source = payload.source_lang.unwrap_or_else(|| "auto".to_string());

    // 支持按请求参数动态选择引擎，未指定则使用默认引擎
    let active_engine = if let Some(ref eng) = payload.engine {
        state.pool.get_engine(eng).await
    } else {
        state.pool.default_engine()
    };
    let engine_id = active_engine.id();

    let mut translations = Vec::with_capacity(payload.texts.len());
    let mut missing_indices = Vec::new();
    let mut missing_requests = Vec::new();

    // 1. 优先查缓存
    for (idx, text) in payload.texts.iter().enumerate() {
        if let Ok(Some(cached_text)) = state.cache.get(text, &source, &target, engine_id) {
            translations.push(cached_text);
        } else {
            translations.push(String::new()); // 占位
            missing_indices.push(idx);
            missing_requests.push(TranslationRequest {
                text: text.clone(),
                source_lang: source.clone(),
                target_lang: target.clone(),
            });
        }
    }

    // 2. 对未命中缓存的项发起批量引擎翻译并写回缓存
    if !missing_requests.is_empty() {
        match active_engine.translate_batch(&missing_requests).await {
            Ok(results) => {
                for (req_idx, res) in results.into_iter().enumerate() {
                    let original_idx = missing_indices[req_idx];
                    let original_text = &payload.texts[original_idx];
                    let _ = state
                        .cache
                        .set(original_text, &res.text, &source, &target, engine_id);
                    translations[original_idx] = res.text;
                }
            }
            Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    // 3. 统一应用全局术语库修正
    let g = state.glossary.read().await;
    if !g.is_empty() {
        for t in &mut translations {
            *t = g.apply(t);
        }
    }

    Ok(Json(TranslateBatchResponse { translations }))
}

pub async fn start_server(
    port: u16,
    token: String,
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, create_router(token, cache, pool, glossary)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;

    #[tokio::test]
    async fn test_health_check() {
        let cache = TranslationCache::in_memory().unwrap();
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let glossary = Arc::new(RwLock::new(Glossary::new()));
        let app = create_router("test-token".to_string(), Arc::new(cache), pool, glossary);
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
        let app = create_router("test-token".to_string(), Arc::new(cache), pool, glossary);

        // 1. 允许扩展协议与本地开发
        let allowed_req = Request::builder()
            .method("OPTIONS")
            .uri("/health")
            .header(header::ORIGIN, "chrome-extension://abcdefg")
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .body(Body::empty())
            .unwrap();
        let allowed_res = app.clone().oneshot(allowed_req).await.unwrap();
        assert_eq!(
            allowed_res
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .map(|v| v.to_str().unwrap()),
            Some("chrome-extension://abcdefg")
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
        let app = create_router("valid-token".to_string(), Arc::new(cache), pool, glossary);
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
        let pool = Arc::new(crate::engine_pool::EnginePool::new());
        let mut glossary_instance = Glossary::new();
        glossary_instance.insert("你好", "世界你好");
        let glossary = Arc::new(RwLock::new(glossary_instance));
        let state = ServerState {
            token: Arc::new("valid-token".to_string()),
            pool,
            cache: cache.clone(),
            glossary,
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
}
