use crate::chain::ChainClient;
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
    State(client): State<ChainClient>,
    Query(query): Query<PageQuery>,
) -> Result<Json<Value>, StatusCode> {
    let page = client
        .deals(query.after)
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let next = if page.deals.len() == 50 {
        page.deals.last().map(|deal| deal.id.to_string())
    } else {
        None
    };
    let deals: Vec<_> = page
        .deals
        .into_iter()
        .map(|deal| json!({"id": deal.id.to_string(), "address": deal.address}))
        .collect();
    Ok(Json(json!({"deals": deals, "next_after": next})))
}
