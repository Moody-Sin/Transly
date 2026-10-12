//! Transly 本地集成测试专用真实 Rust HTTP 服务入口。
//!
//! 启动完全真实的 Axum 路由（含真实 Host 守卫、真实 Bearer Token 鉴权、真实 CORS 预检与切批校验），
//! 底层注入零网络、零凭据依赖的 Stub 引擎，专供全链路跨进程真实端到端测试（Browser Extension E2E）使用。

use std::sync::Arc;
use tokio::sync::RwLock;
use transly_core::{Glossary, TranslationCache};

const TEST_PAGE_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Transly E2E Test Target Page</title>
</head>
<body>
  <h1>Transly In-Place Translation Test Document</h1>

  <!-- 场景 1: 普通文本段落 -->
  <p id="simple-para">Hello World, this is a plain text test for full page in-place replacement.</p>

  <!-- 场景 2: 包含行内标签的富文本段落 -->
  <p id="rich-para">Please visit <a id="rich-link" href="https://example.com/target">Example Official Site</a> and check <strong>Crucial Document</strong> before proceeding.</p>

  <!-- 场景 3: 排除与代码块标签 -->
  <div id="code-container">
    <code id="ignored-code">const apiKey = "do-not-translate-code";</code>
    <div class="notranslate" id="ignored-notranslate">This content must never be translated.</div>
  </div>

  <!-- 场景 4: 开放模式 Web Components Shadow DOM -->
  <custom-card id="shadow-host"></custom-card>

  <script>
    class CustomCard extends HTMLElement {
      connectedCallback() {
        const shadow = this.attachShadow({ mode: 'open' });
        shadow.innerHTML = '<div class="shadow-box"><p id="shadow-para">Shadow component paragraph text inside open web component root.</p></div>';
      }
    }
    customElements.define('custom-card', CustomCard);
  </script>
</body>
</html>"#;

async fn test_page_handler() -> axum::response::Html<&'static str> {
    axum::response::Html(TEST_PAGE_HTML)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let port = std::env::var("TRANSLY_SERVER_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(39281);

    let token_str = std::env::var("TRANSLY_SERVER_TOKEN")
        .unwrap_or_else(|_| "e2e-valid-token-39281".to_string());

    let token = Arc::new(RwLock::new(token_str));
    let cache = Arc::new(TranslationCache::in_memory()?);
    let pool = Arc::new(src_tauri::engine_pool::EnginePool::with_default_engine(
        Arc::new(src_tauri::engine_pool::testing::StubEngine),
    ));

    let mut glossary_inst = Glossary::new();
    glossary_inst.insert_with_options("pipeline", "流水线", false);
    let glossary = Arc::new(RwLock::new(glossary_inst));

    let cache_controller = Arc::new(src_tauri::cache_controller::CacheController::new(true));
    let pairing_manager = Arc::new(src_tauri::server::pairing::PairingManager::new());

    let app = src_tauri::server::create_router_with_pairing(
        token,
        cache,
        pool,
        glossary,
        cache_controller,
        pairing_manager,
        None,
    )
    .route("/test-page.html", axum::routing::get(test_page_handler));

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("[Transly Test Server] READY on port {}", port);
    axum::serve(listener, app).await?;

    Ok(())
}
