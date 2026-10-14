use crate::cache::Cache;
use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
pub struct PageQuery {
    after: Option<u64>,
}

pub async fn list(
    State(cache): State<Cache>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>, StatusCode> {
    let (snapshot, stale) = cache.read().ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let mut page = snapshot
        .deals
        .iter()
        .filter(|(id, _)| query.after.is_none_or(|after| **id > after));
    let deals: Vec<_> = page
        .by_ref()
        .take(50)
        .map(|(id, deal)| json!({"id": id.to_string(), "address": deal["address"]}))
        .collect();
    let next = page
        .next()
        .and_then(|_| deals.last().map(|deal| deal["id"].clone()));
    Ok(Json(json!({"deals": deals, "next_after": next,
        "updated_at": snapshot.updated_at, "stale": stale})))
}
