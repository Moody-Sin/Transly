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
use tower_http::cors::{Any, CorsLayer};
use transly_core::{TranslateEngine, TranslationCache, TranslationRequest};

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
}

#[derive(Serialize, Deserialize)]
pub struct TranslateBatchResponse {
    pub translations: Vec<String>,
}

#[derive(Clone)]
pub struct ServerState {
    pub token: Arc<String>,
    pub engine: Arc<dyn TranslateEngine>,
    pub cache: Arc<TranslationCache>,
}

pub fn create_router(
    token: String,
    cache: Arc<TranslationCache>,
    engine: Arc<dyn TranslateEngine>,
) -> Router {
    let state = ServerState {
        token: Arc::new(token),
        engine,
        cache,
    };

    let protected = Router::new()
        .route("/translate", post(translate_batch_handler))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let cors = CorsLayer::new()
        .allow_origin(Any)
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
    let engine_id = state.engine.id();

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
        match state.engine.translate_batch(&missing_requests).await {
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

    Ok(Json(TranslateBatchResponse { translations }))
}

pub async fn start_server(
    port: u16,
    token: String,
    cache: Arc<TranslationCache>,
    engine: Arc<dyn TranslateEngine>,
) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, create_router(token, cache, engine)).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::util::ServiceExt;
    use transly_core::GoogleWebEngine;

    #[tokio::test]
    async fn test_health_check() {
        let cache = TranslationCache::in_memory().unwrap();
        let engine: Arc<dyn TranslateEngine> = Arc::new(GoogleWebEngine::new());
        let app = create_router("test-token".to_string(), Arc::new(cache), engine);
        let req = Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_translate_unauthorized() {
        let cache = TranslationCache::in_memory().unwrap();
        let engine: Arc<dyn TranslateEngine> = Arc::new(GoogleWebEngine::new());
        let app = create_router("valid-token".to_string(), Arc::new(cache), engine);
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
        struct MockEngine;
        #[::async_trait::async_trait]
        impl TranslateEngine for MockEngine {
            fn id(&self) -> &'static str {
                "mock"
            }
            fn name(&self) -> &'static str {
                "Mock"
            }
            async fn translate(
                &self,
                req: &TranslationRequest,
            ) -> transly_core::Result<transly_core::TranslationResponse> {
                Ok(transly_core::TranslationResponse {
                    text: format!("[{}] {}", req.target_lang, req.text),
                    detected_source_lang: Some("en".to_string()),
                    engine: "mock".to_string(),
                })
            }
        }

        let cache = Arc::new(TranslationCache::in_memory().unwrap());
        let state = ServerState {
            token: Arc::new("valid-token".to_string()),
            engine: Arc::new(MockEngine),
            cache: cache.clone(),
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
        assert_eq!(resp.translations, vec!["[zh-CN] hello"]);

        // 验证翻译结果已自动写入缓存
        let cached = cache.get("hello", "auto", "zh-CN", "mock").unwrap();
        assert_eq!(cached, Some("[zh-CN] hello".to_string()));
    }
}
