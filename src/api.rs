use crate::cache::Cache;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde_json::{Value, json};

pub async fn deal(
    State(cache): State<Cache>,
    Path(address): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let (snapshot, stale) = cache.read().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let mut deal = snapshot
        .deals
        .values()
        .find(|deal| deal["address"] == address)
        .cloned()
        .ok_or(StatusCode::NOT_FOUND)?;
    deal["updated_at"] = json!(snapshot.updated_at);
    deal["stale"] = json!(stale);
    Ok(Json(deal))
}
