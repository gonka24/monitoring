use axum::{Router, routing::get};

mod api;
pub mod chain;
pub mod deal;
pub mod factory;

/// Factory: assembles the HTTP application independently of its server.
pub fn create_app(client: chain::ChainClient) -> Router {
    Router::new()
        .route("/", get(|| async { "Hello, world!\n" }))
        .route("/healthz", get(|| async { "ok\n" }))
        .route("/v1/deals/{address}", get(api::deal))
        .with_state(client)
}
