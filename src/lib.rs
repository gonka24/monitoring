use axum::{Router, routing::get};

mod api;
pub mod cache;
pub mod chain;
pub mod collector;
pub mod config;
pub mod deal;
pub mod event;
pub mod factory;
pub mod history;
pub mod refresh;
mod registry_api;
pub mod rpc;
pub mod transaction;

/// Factory: assembles the HTTP application independently of its server.
pub fn create_app(cache: cache::Cache) -> Router {
    Router::new()
        .route("/", get(|| async { "Hello, world!\n" }))
        .route("/healthz", get(|| async { "ok\n" }))
        .route("/v1/deals/{address}", get(api::deal))
        .route("/v1/deals/{address}/events", get(api::events))
        .route("/v1/deals", get(registry_api::list))
        .with_state(cache)
}
