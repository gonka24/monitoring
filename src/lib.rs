use axum::{Router, routing::get};

/// Factory: assembles the HTTP application independently of its server.
pub fn create_app() -> Router {
    Router::new()
        .route("/", get(|| async { "Hello, world!\n" }))
        .route("/healthz", get(|| async { "ok\n" }))
}
