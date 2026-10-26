use super::{RoutingError, RoutingOptions};
use crate::{cache::Cache, search::valid_address};
use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

impl IntoResponse for RoutingError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::InvalidAddress | Self::InvalidArgument => StatusCode::BAD_REQUEST,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            _ => StatusCode::SERVICE_UNAVAILABLE,
        };
        let mut response = (
            status,
            Json(serde_json::json!({"error":{"code":self,"message":"Could not verify routing"}})),
        )
            .into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if self == Self::RateLimited {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, HeaderValue::from_static("1"));
        }
        response
    }
}

pub(crate) async fn handler(
    State(cache): State<Cache>,
    Path(deal): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, RoutingError> {
    if query.is_some_and(|s| !s.is_empty()) {
        return Err(RoutingError::InvalidArgument);
    }
    if !valid_address(&deal) {
        return Err(RoutingError::InvalidAddress);
    }
    let client = cache.routing().ok_or(RoutingError::RpcUnavailable)?;
    let _permit = client
        .capacity
        .clone()
        .try_acquire_owned()
        .map_err(|_| RoutingError::RateLimited)?;
    let result = client
        .get_deal_routing(&deal, RoutingOptions::default())
        .await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
