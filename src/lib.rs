use axum::{Router, routing::get};

mod api;
pub mod cache;
pub mod chain;
pub mod collector;
pub mod config;
pub mod deal;
pub mod factory;
pub mod refresh;
mod registry_api;

/// Factory: assembles the HTTP application independently of its server.
pub fn create_app(client: chain::ChainClient, cache: cache::Cache) -> Router {
    Router::new()
        .route("/", get(|| async { "Hello, world!\n" }))
        .route("/healthz", get(|| async { "ok\n" }))
        .route("/v1/deals/{address}", get(api::deal))
        .route("/v1/deals", get(registry_api::list).with_state(cache))
        .with_state(client)
}
