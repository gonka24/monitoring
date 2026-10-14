use monitoring::event::ContractEvent;

#[test]
fn reads_live_event_and_verifies_emitter() -> Result<(), serde_json::Error> {
    let mut event: ContractEvent = serde_json::from_str(include_str!("fixtures/deal-funded.json"))?;
    let address = "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs";
    assert_eq!(event.kind, "wasm-deal_funded");
    assert_eq!(event.attributes[2].value, "140000000");
    assert!(event.emitted_by(address));
    assert!(!event.emitted_by(monitoring::factory::ADDRESS));
    event.attributes.push(event.attributes[0].clone());
    assert!(!event.emitted_by(address));
    event.attributes.retain(|a| a.key != "_contract_address");
    assert!(!event.emitted_by(address));
    Ok(())
}
