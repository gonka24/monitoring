use axum::{
    Router,
    routing::{get, post},
};

mod api;
pub mod cache;
pub mod chain;
pub mod collector;
pub mod config;
pub mod deal;
mod diagnostics;
pub mod event;
pub mod factory;
pub mod history;
pub mod refresh;
mod registry_api;
pub mod routing;
pub mod rpc;
pub mod search;
pub mod transaction;

pub use routing::{
    CancellationToken, DealRoutingResult, DealRoutingState, RoutingClient, RoutingDeployment,
    RoutingError, RoutingEvidence, RoutingOptions, parse_deal_routing_result,
};

/// Factory: assembles the HTTP application independently of its server.
pub fn create_app(cache: cache::Cache) -> Router {
    Router::new()
        .route("/", get(|| async { "Hello, world!\n" }))
        .route("/healthz", get(|| async { "ok\n" }))
        .route("/v1/deals/{address}", get(api::deal))
        .route("/v1/deals/{address}/events", get(api::events))
        .route("/v1/deals/{address}/routing", get(routing::handler))
        .route("/v1/deals", get(registry_api::list))
        .route("/v2/deals/search", post(search::handler))
        .with_state(cache)
}
