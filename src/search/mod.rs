mod cursor;
mod details;
mod network;
mod request;
pub use network::NetworkSearch;
pub use request::{Filters, SearchRequest, valid_address};

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

#[derive(Debug, PartialEq, Eq)]
pub enum SearchError {
    InvalidFilter(String),
    InvalidLimit,
    InvalidCursor,
    TooLarge,
    RateLimited,
    Unavailable,
}
impl SearchError {
    pub(crate) fn filter(field: &str) -> Self {
        Self::InvalidFilter(field.to_owned())
    }
}
impl From<crate::chain::ChainError> for SearchError {
    fn from(_: crate::chain::ChainError) -> Self {
        Self::Unavailable
    }
}
impl IntoResponse for SearchError {
    fn into_response(self) -> Response {
        let (status, code, message, field) = match self {
            Self::InvalidFilter(field) => (
                StatusCode::BAD_REQUEST,
                "INVALID_FILTER",
                "Use at least one nonempty hosts/buyers filter with up to 100 valid Gonka addresses; unknown fields are rejected",
                Some(field),
            ),
            Self::InvalidLimit => (
                StatusCode::BAD_REQUEST,
                "INVALID_LIMIT",
                "limit must be an integer from 1 to 100",
                Some("limit".into()),
            ),
            Self::InvalidCursor => (
                StatusCode::BAD_REQUEST,
                "INVALID_CURSOR",
                "Invalid cursor or incompatible request",
                Some("cursor".into()),
            ),
            Self::TooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "REQUEST_TOO_LARGE",
                "JSON body exceeds 32 KiB",
                None,
            ),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "RATE_LIMITED",
                "Too many concurrent network searches",
                None,
            ),
            Self::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "MONITORING_UNAVAILABLE",
                "Node unavailable or indexed history changed; retry or start a new search",
                None,
            ),
        };
        let mut error = json!({"code":code,"message":message});
        if let Some(field) = field {
            error["field"] = json!(field);
        }
        let mut response = (status, Json(json!({"error":error}))).into_response();
        if status == StatusCode::TOO_MANY_REQUESTS {
            response.headers_mut().insert(
                header::RETRY_AFTER,
                axum::http::HeaderValue::from_static("1"),
            );
        }
        response
    }
}
pub(crate) async fn handler(
    State(cache): State<crate::cache::Cache>,
    request: Request,
) -> Result<Json<Value>, SearchError> {
    let body = axum::body::to_bytes(request.into_body(), 32768)
        .await
        .map_err(|_| SearchError::TooLarge)?;
    let value = serde_json::from_slice(&body).map_err(|_| SearchError::filter("body"))?;
    let request = SearchRequest::parse(&value)?;
    if request.filters.hosts.is_empty() && request.filters.buyers.is_empty() {
        return Err(SearchError::filter("filters"));
    }
    let search = cache.network_search().ok_or(SearchError::Unavailable)?;
    search.search(&request).await.map(Json)
}
