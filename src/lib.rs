use axum::{Router, routing::get};

pub mod chain;
pub mod deal;

/// Factory: assembles the HTTP application independently of its server.
pub fn create_app() -> Router {
    Router::new()
        .route("/", get(|| async { "Hello, world!\n" }))
        .route("/healthz", get(|| async { "ok\n" }))
}
