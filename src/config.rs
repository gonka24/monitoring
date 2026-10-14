use serde::Deserialize;

/// Immutable conditions; amounts remain decimal strings without floating-point conversion.
#[derive(Debug, Deserialize)]
pub struct DealConfig {
    pub factory: String,
    pub host: String,
    pub deal_address: String,
    pub target_epoch: u64,
    pub price_micro_usdt_per_gnk: String,
    pub buyer_budget_micro_usdt: String,
    pub settlement_cw20: String,
}
