use monitoring::transaction::Transaction;
use serde_json::json;

#[test]
fn filters_execution_and_preserves_event_identity() -> Result<(), serde_json::Error> {
    let event: serde_json::Value = serde_json::from_str(include_str!("fixtures/deal-funded.json"))?;
    let address = "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs";
    // Synthetic transaction envelope with a real contract event.
    let mut input = json!({
        "hash": "76A75CDE1C93E3C32B927F340365F30B8D485A678F95A5665D87C2265BCC16CD",
        "height": "6519973",
        "tx_result": {"code": 0, "events": [
            {"type": "transfer", "attributes": []}, event.clone(), event
        ]}
    });
    let tx: Transaction = serde_json::from_value(input.clone())?;
    let records: Vec<_> = tx.events_for(address).collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].event_index, 1);
    assert_eq!(records[1].event_index, 2);
    assert_eq!(records[0].transaction_hash, tx.hash);
    assert_eq!(records[0].height, "6519973");
    assert_eq!(records[0].event.kind, "wasm-deal_funded");
    assert_eq!(serde_json::to_value(&records[0])?["height"], "6519973");
    assert_eq!(tx.events_for(monitoring::factory::ADDRESS).count(), 0);
    input["tx_result"]["code"] = json!(5);
    assert_eq!(
        serde_json::from_value::<Transaction>(input.clone())?
            .events_for(address)
            .count(),
        0
    );
    input["tx_result"] = json!({"events": []});
    assert!(serde_json::from_value::<Transaction>(input).is_err());
    Ok(())
}
