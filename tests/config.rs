use monitoring::{config::DealConfig, factory};
use serde_json::Value;

#[test]
fn shipped_inventory_contains_at_least_ten_valid_origins() -> Result<(), Box<dyn std::error::Error>>
{
    let config: monitoring::config::NodeConfig =
        serde_json::from_str(include_str!("../config/nodes.json"))?;
    assert!(config.nodes.len() >= 10);
    monitoring::pool::NodePool::new(&config.nodes)?;
    assert!(
        serde_json::from_str::<monitoring::config::NodeConfig>(r#"{"nodes":[],"typo":true}"#)
            .is_err()
    );
    Ok(())
}

#[test]
fn reads_live_conditions_without_rounding() -> Result<(), serde_json::Error> {
    let response: Value = serde_json::from_str(include_str!("fixtures/deal-config.json"))?;
    let config: DealConfig = serde_json::from_value(response["data"].clone())?;
    assert_eq!(config.factory, factory::ADDRESS);
    assert_eq!(config.target_epoch, 423);
    assert_eq!(config.buyer_budget_micro_usdt, "140000000");
    assert_eq!(config.price_micro_usdt_per_gnk, "200000");
    assert_eq!(config.host, "gonka1hvk3t5c4hzn0q8q3lvg6l7k4dfvfa5xqj4a66m");
    Ok(())
}
