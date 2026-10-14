use crate::{
    cache::{Snapshot, now},
    chain::{ChainClient, ChainError},
    factory,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Collects a complete traversal, not an atomic snapshot at one block height.
/// The timestamp marks collection start; no partial result is published.
/// # Errors
/// Rejects upstream failures, inconsistent identity/pagination and more than 1000 deals.
pub async fn collect(client: &ChainClient) -> Result<Snapshot, ChainError> {
    let updated_at = now();
    let mut deals = BTreeMap::new();
    let mut addresses = BTreeSet::new();
    let mut after = None;
    loop {
        let page = client.deals(after).await?;
        if page.deals.is_empty() {
            return Ok(Snapshot { deals, updated_at });
        }
        for deal in page.deals {
            if deals.len() >= 1000
                || Some(deal.id) <= after
                || !addresses.insert(deal.address.clone())
            {
                return Err(ChainError::Upstream);
            }
            let config = client.config(&deal.address).await?;
            if config.factory != factory::ADDRESS || config.deal_address != deal.address {
                return Err(ChainError::Upstream);
            }
            let state = client.state(&deal.address).await?;
            deals.insert(
                deal.id,
                json!({
                    "id": deal.id.to_string(), "address": deal.address,
                    "status": state.status, "buyer": state.buyer, "host": config.host,
                    "target_epoch": config.target_epoch.to_string(), "factory": config.factory,
                    "buyer_budget_micro_usdt": config.buyer_budget_micro_usdt,
                    "price_micro_usdt_per_gnk": config.price_micro_usdt_per_gnk,
                    "settlement_cw20": config.settlement_cw20
                }),
            );
            after = Some(deal.id);
        }
    }
}
