use crate::{
    chain::{ChainClient, ChainError},
    deal::DealState,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};

pub async fn deal(
    State(client): State<ChainClient>,
    Path(address): Path<String>,
) -> Result<Json<DealState>, (StatusCode, &'static str)> {
    client
        .state(&address)
        .await
        .map(Json)
        .map_err(|error| match error {
            ChainError::InvalidAddress => (StatusCode::BAD_REQUEST, "Invalid contract address"),
            ChainError::Upstream => (StatusCode::BAD_GATEWAY, "Unable to read contract state"),
        })
}
