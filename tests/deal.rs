use monitoring::deal::{DealState, DealStatus};
use serde_json::{Value, json};

#[test]
fn reads_live_deal_state() -> Result<(), Box<dyn std::error::Error>> {
    let response: Value = serde_json::from_str(include_str!("fixtures/deal-state.json"))?;
    let state: DealState = serde_json::from_value(response["data"].clone())?;
    assert_eq!(state.status, DealStatus::Releasing);
    assert_eq!(
        state.buyer.as_deref(),
        Some("gonka1ww2p5wv8plwte5tr00spkv0vt2hzr9s9kppd0x")
    );
    Ok(())
}

#[test]
fn handles_unfunded_and_invalid_states() -> Result<(), serde_json::Error> {
    let state: DealState = serde_json::from_value(json!({"status": "open", "buyer": null}))?;
    assert_eq!(
        state,
        DealState {
            status: DealStatus::Open,
            buyer: None
        }
    );
    for invalid in [
        json!({"status": "unknown"}),
        json!({"buyer": null}),
        json!({"status": "funded", "buyer": 42}),
    ] {
        assert!(serde_json::from_value::<DealState>(invalid).is_err());
    }
    Ok(())
}
