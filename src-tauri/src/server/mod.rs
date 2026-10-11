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

pub mod pairing;

#[derive(Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
    /// 批量上限由服务端在 `/health` 公开，供扩展端切批时读取。
    /// 扩展端按「项数」切批、服务端按「字符总量」拒绝，两者量纲不同，
    /// 客户端只有拿到真实上限才能保证发出的每一批都必然被接受。
    pub max_batch_items: usize,
    pub max_batch_total_chars: usize,
    pub max_item_chars: usize,
}

#[derive(Deserialize)]
pub struct TranslateBatchRequest {
    /// 兼容旧版客户端：纯文本数组（新客户端统一走 `items`）
    #[serde(default)]
    pub texts: Vec<String>,
    /// 主契约：带有明确 stable ID 的批量项目
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

#[derive(Deserialize)]
pub struct PairRequestPayload {
    pub client_id: String,
    pub browser_hint: Option<String>,
}

#[derive(Serialize)]
pub struct PairRequestResponse {
    pub session_id: String,
    pub expires_in: u64,
}

#[derive(Deserialize)]
pub struct PairPollPayload {
    pub session_id: String,
    pub client_id: String,
}

pub type PairingNotifyHook = Arc<dyn Fn(&str, Option<&str>) + Send + Sync>;

#[derive(Clone)]
pub struct ServerState {
    pub token: Arc<RwLock<String>>,
    pub pool: Arc<crate::engine_pool::EnginePool>,
    pub cache: Arc<TranslationCache>,
    pub glossary: Arc<RwLock<Glossary>>,
    pub concurrency_limit: Arc<tokio::sync::Semaphore>,
    pub cache_controller: Arc<crate::cache_controller::CacheController>,
    pub pairing_manager: Arc<pairing::PairingManager>,
    pub pairing_notify_hook: Option<PairingNotifyHook>,
}

impl ServerState {
    pub fn new(
        token: Arc<RwLock<String>>,
        pool: Arc<crate::engine_pool::EnginePool>,
        cache: Arc<TranslationCache>,
        glossary: Arc<RwLock<Glossary>>,
        cache_controller: Arc<crate::cache_controller::CacheController>,
    ) -> Self {
        Self {
            token,
            pool,
            cache,
            glossary,
            concurrency_limit: Arc::new(tokio::sync::Semaphore::new(10)),
            cache_controller,
            pairing_manager: Arc::new(pairing::PairingManager::new()),
            pairing_notify_hook: None,
        }
    }
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

/// 恒定时间比较两个字节序列，避免 Bearer Token 被计时侧信道逐字节推断。
/// 长度不等时立即返回 false（长度本身不是秘密）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    use subtle::ConstantTimeEq;
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

/// 校验 `Authorization: Bearer <token>` 头是否与期望令牌一致。
/// 校验 `Host` 头是否指向回环地址，用于抵御 DNS Rebinding 攻击。
///
/// **CORS 不是服务端访问控制。** 攻击者把域名的 A 记录 TTL 设为 0 指向
/// `127.0.0.1`（DNS Rebinding）后，`http://evil.com:39281` 与其恶意页面**同源**，
/// 浏览器不会发送预检、也不会校验 CORS，`is_allowed_origin` 形同虚设。
/// 此时唯一有效的防线是在服务端独立校验 `Host` 头。
fn host_header_allowed(host: &str) -> bool {
    let host = host.trim();
    let name = match host.strip_prefix('[') {
        // IPv6 字面量，如 `[::1]:39281` 或 `[::1]`
        Some(rest) => match rest.split_once(']') {
            Some((name, tail))
                if tail.is_empty()
                    || (tail.starts_with(':')
                        && !tail[1..].is_empty()
                        && tail[1..].chars().all(|c| c.is_ascii_digit())) =>
            {
                name
            }
            _ => "",
        },
        None => match host.rsplit_once(':') {
            // `主机名:端口`：端口必须全为数字，否则整体视为主机名
            Some((name, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
                name
            }
            _ => host,
        },
    };
    matches!(name, "127.0.0.1" | "localhost" | "::1")
}

/// 拒绝任何指向非回环地址的请求。这是抵御 DNS Rebinding 的第一道也是最关键的一道。
async fn host_guard(request: Request, next: Next) -> Result<Response, StatusCode> {
    let allowed = request
        .headers()
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(host_header_allowed);

    if allowed {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn bearer_matches(header: Option<&str>, expected: &str) -> bool {
    let Some(val) = header else {
        return false;
    };
    // 期望令牌为空时必须 fail-closed：`Authorization: Bearer `（空值）
    // 恰好等于空串会在恒定时间比较中判定为相等，导致鉴权整体失效。
    if expected.is_empty() {
        return false;
    }
    let Some(presented) = val.strip_prefix("Bearer ") else {
        return false;
    };
    constant_time_eq(presented.as_bytes(), expected.as_bytes())
}

pub fn create_router(
    token: Arc<RwLock<String>>,
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
    cache_controller: Arc<crate::cache_controller::CacheController>,
) -> Router {
    create_router_with_pairing(
        token,
        cache,
        pool,
        glossary,
        cache_controller,
        Arc::new(pairing::PairingManager::new()),
        None,
    )
}

pub fn create_router_with_pairing(
    token: Arc<RwLock<String>>,
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
    cache_controller: Arc<crate::cache_controller::CacheController>,
    pairing_manager: Arc<pairing::PairingManager>,
    pairing_notify_hook: Option<PairingNotifyHook>,
) -> Router {
    let state = ServerState {
        token,
        pool,
        cache,
        glossary,
        concurrency_limit: Arc::new(tokio::sync::Semaphore::new(10)), // 限制本地并发处理至多 10 批
        cache_controller,
        pairing_manager,
        pairing_notify_hook,
    };

    let protected = Router::new()
        .route("/translate", post(translate_batch_handler))
        .route("/verify_token", get(verify_token_handler))
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
        .route("/pair/request", post(pair_request_handler))
        .route("/pair/poll", post(pair_poll_handler))
        .merge(protected)
        .layer(cors)
        // host_guard 必须在 cors 之内（更先执行）：预检与实请求都要先过 Host 校验，
        // 否则 DNS Rebinding 下同源请求根本不会触发 CORS 分支。
        .layer(middleware::from_fn(host_guard))
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
    if bearer_matches(auth_header, current_token.as_str()) {
        drop(current_token);
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    #[test]
    fn test_bearer_matches_exact_token() {
        assert!(bearer_matches(Some("Bearer abc123"), "abc123"));
    }

    #[test]
    fn test_bearer_rejects_malformed_and_mismatched() {
        // 缺失头部 / 前缀错误 / 大小写不符
        assert!(!bearer_matches(None, "abc123"));
        assert!(!bearer_matches(Some("abc123"), "abc123"));
        assert!(!bearer_matches(Some("bearer abc123"), "abc123"));
        // 前缀正确但令牌不同（含长度不同与同长度不同两种）
        assert!(!bearer_matches(Some("Bearer abc12"), "abc123"));
        assert!(!bearer_matches(Some("Bearer abc124"), "abc123"));
        // 空令牌不得匹配空期望值以外的任何输入
        assert!(!bearer_matches(Some("Bearer "), "abc123"));
    }

    #[test]
    fn test_constant_time_eq_semantics() {
        assert!(constant_time_eq(b"token", b"token"));
        assert!(!constant_time_eq(b"token", b"tokeN"));
        assert!(!constant_time_eq(b"token", b"token2"));
        assert!(constant_time_eq(b"", b""));
    }
}

async fn health_handler() -> Json<StatusResponse> {
    Json(status_response())
}

async fn verify_token_handler() -> Json<StatusResponse> {
    Json(status_response())
}

async fn pair_request_handler(
    State(state): State<ServerState>,
    Json(payload): Json<PairRequestPayload>,
) -> Result<Json<PairRequestResponse>, StatusCode> {
    let client_id = payload.client_id.trim();
    if client_id.is_empty() || client_id.len() > 128 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let (session_id, expires_in) = state
        .pairing_manager
        .create_session(client_id.to_string(), payload.browser_hint.clone())
        .await;

    // 若注册了事件钩子（如 Tauri AppHandle），立即广播通知前端
    if let Some(ref hook) = state.pairing_notify_hook {
        hook(&session_id, payload.browser_hint.as_deref());
    }

    Ok(Json(PairRequestResponse {
        session_id,
        expires_in,
    }))
}

async fn pair_poll_handler(
    State(state): State<ServerState>,
    Json(payload): Json<PairPollPayload>,
) -> Result<Json<pairing::PairingPollResult>, StatusCode> {
    let current_token = state.token.read().await;
    let res = state
        .pairing_manager
        .poll_session(&payload.session_id, &payload.client_id, &current_token)
        .await;
    Ok(Json(res))
}

fn status_response() -> StatusResponse {
    StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        max_batch_items: transly_core::MAX_BATCH_ITEMS,
        max_batch_total_chars: transly_core::MAX_BATCH_TOTAL_CHARS,
        max_item_chars: transly_core::MAX_ITEM_CHARS,
    }
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

    // 限制单次批量文本数量与总字符数，防御 DoS 与过载攻击（上限与 transly-core 共享同一常量，
    // 并由 `/health` 公开给客户端，扩展端据此切批，不再各写一个数字）
    if unified_items.is_empty() || unified_items.len() > transly_core::MAX_BATCH_ITEMS {
        return Err(StatusCode::BAD_REQUEST);
    }
    let total_chars: usize = unified_items.iter().map(|t| t.text.chars().count()).sum();
    if total_chars > transly_core::MAX_BATCH_TOTAL_CHARS {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    for t in &unified_items {
        if t.text.chars().count() > transly_core::MAX_ITEM_CHARS {
            return Err(StatusCode::PAYLOAD_TOO_LARGE);
        }
    }

    let target = payload.target_lang;
    let source = payload.source_lang.unwrap_or_else(|| "auto".to_string());

    // 与 IPC 侧共用同一个语言对校验："不支持该语言"只允许有一个判定来源，
    // 两处各写一份必然漂移。HTTP 侧只负责把失败映射为状态码。
    if transly_core::validate_language_pair(&source, &target).is_err() {
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

    // 检查缓存状态快照与 generation 版本，杜绝并发关闭清空后的写回竞争
    let (cache_enabled, start_generation) = state.cache_controller.snapshot();

    let mut item_results: Vec<Option<BatchItemResult>> = vec![None; unified_items.len()];
    let mut missing_indices = Vec::new();
    let mut missing_requests = Vec::new();

    // 1. 若开启缓存，优先查缓存（先查当前引擎，若具备 fallback 且未命中，允许命中降级引擎历史缓存）
    if cache_enabled {
        let cached_items = {
            let cache_ref = state.cache.clone();
            let engine_ref = active_engine.clone();
            let items = unified_items.clone();
            let s = source.clone();
            let t = target.clone();
            tokio::task::spawn_blocking(move || {
                let mut matches = Vec::with_capacity(items.len());
                for item in &items {
                    let hit = transly_core::lookup_cache_with_fallback(
                        cache_ref.as_ref(),
                        engine_ref.as_ref(),
                        &item.text,
                        &s,
                        &t,
                    );
                    matches.push(hit);
                }
                matches
            })
            .await
            .unwrap_or_else(|_| vec![None; unified_items.len()])
        };

        for (idx, hit) in cached_items.into_iter().enumerate() {
            if let Some((cached_text, used_engine)) = hit {
                item_results[idx] = Some(BatchItemResult::Success {
                    id: unified_items[idx].id.clone(),
                    text: cached_text,
                    detected_source_lang: None,
                    engine: used_engine,
                });
            } else {
                missing_indices.push(idx);
                missing_requests.push(TranslationRequest {
                    text: unified_items[idx].text.clone(),
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

    // 2. 对未命中缓存的项发起容错批量引擎翻译并写回缓存
    if !missing_requests.is_empty() {
        let results = active_engine
            .translate_batch_tolerant(&missing_requests)
            .await;
        // 安全防范：验证引擎返回条数与请求条数完全一致
        if results.len() != missing_requests.len() {
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }

        // 构造批量写入项，通过单次事务批量写入缓存，避免循环频繁加锁与隐式事务开销
        let mut batch_cache_entries = Vec::with_capacity(results.len());
        for (req_idx, item_res) in results.into_iter().enumerate() {
            let original_idx = missing_indices[req_idx];
            let original_item = &unified_items[original_idx];

            match item_res {
                Ok(res) => {
                    let actual_engine = res.actual_engine().to_string();
                    item_results[original_idx] = Some(BatchItemResult::Success {
                        id: original_item.id.clone(),
                        text: res.text.clone(),
                        detected_source_lang: res.detected_source_lang,
                        engine: res.engine,
                    });

                    batch_cache_entries.push((original_item.text.clone(), res.text, actual_engine));
                }
                Err(e) => {
                    item_results[original_idx] = Some(BatchItemResult::Failed {
                        id: original_item.id.clone(),
                        error_kind: e.kind(),
                        message: e.to_string(),
                    });
                }
            }
        }

        // 写回时通过同一同步门禁 set_if_current 校验版本并执行事务批量写入成功项
        if !batch_cache_entries.is_empty() {
            // 同步门禁 + SQLite 写必须下沉到阻塞线程池，否则单批最长可占用
            // 一个 tokio worker 5 秒（连接 busy_timeout），且会拖住全局串行门禁。
            let entries = batch_cache_entries.clone();
            let source = source.clone();
            let target = target.clone();
            let controller = Arc::clone(&state.cache_controller);
            let cache = Arc::clone(&state.cache);
            crate::cache_controller::write_cache_offloaded(
                controller,
                start_generation,
                move || {
                    let rows = entries.iter().map(|(txt, trans, eng)| {
                        (
                            txt.as_str(),
                            trans.as_str(),
                            source.as_str(),
                            target.as_str(),
                            eng.as_str(),
                        )
                    });
                    cache.set_batch(rows)
                },
            )
            .await;
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
            // 失败项在旧版 translations 数组中保留原文，避免旧客户端把失败结果替换为空文本
            final_translations.push(unified_items[idx].text.clone());
        }
        final_items.push(item);
    }

    Ok(Json(TranslateBatchResponse {
        translations: final_translations,
        items: final_items,
    }))
}

/// 绑定端口并启动本地 HTTP 服务。
///
/// 拆成 bind 与 serve 两段，`on_listening` 在端口已成功绑定后触发：
/// `axum::serve` 正常只在出错时返回，若等服务返回再上报状态，
/// 成功路径永远停在“启动中”。
#[allow(clippy::too_many_arguments)]
pub async fn start_server(
    port: u16,
    token: Arc<RwLock<String>>,
    cache: Arc<TranslationCache>,
    pool: Arc<crate::engine_pool::EnginePool>,
    glossary: Arc<RwLock<Glossary>>,
    cache_controller: Arc<crate::cache_controller::CacheController>,
    pairing_manager: Arc<pairing::PairingManager>,
    pairing_notify_hook: Option<PairingNotifyHook>,
    on_listening: impl FnOnce(),
) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    on_listening();
    axum::serve(
        listener,
        create_router_with_pairing(
            token,
            cache,
            pool,
            glossary,
            cache_controller,
            pairing_manager,
            pairing_notify_hook,
        ),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
