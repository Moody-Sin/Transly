use axum::{routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;

#[derive(Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
}

pub fn create_router() -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive())
}

async fn health_handler() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

pub async fn start_server(port: u16) -> anyhow::Result<()> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, create_router()).await?;
    Ok(())
}
