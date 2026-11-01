use serde::Deserialize;

/// Server-owned node inventory; never populated from an API request.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeConfig {
    pub nodes: Vec<String>,
}

impl NodeConfig {
    /// Loads a JSON inventory and validates every origin before startup.
    /// # Errors
    /// Returns file, JSON or pool validation errors without dumping file contents.
    pub fn load(
        path: &std::path::Path,
    ) -> Result<crate::pool::NodePool, Box<dyn std::error::Error>> {
        let text = std::fs::read_to_string(path).map_err(|_| "Cannot read node configuration")?;
        let config: Self =
            serde_json::from_str(&text).map_err(|_| "Invalid node configuration JSON")?;
        crate::pool::NodePool::new(&config.nodes)
    }
}

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
