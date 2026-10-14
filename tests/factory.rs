use monitoring::factory::DealPage;
use serde_json::{Value, json};

#[test]
fn reads_factory_registry() -> Result<(), serde_json::Error> {
    let response: Value = serde_json::from_str(include_str!("fixtures/factory-deals.json"))?;
    let page: DealPage = serde_json::from_value(response["data"].clone())?;
    assert_eq!(page.deals.len(), 1);
    assert_eq!(page.deals[0].id, 1);
    assert_eq!(
        page.deals[0].address,
        "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs"
    );
    assert_eq!(
        serde_json::from_value::<DealPage>(json!({"deals": []}))?.deals,
        []
    );
    assert!(serde_json::from_value::<DealPage>(json!({})).is_err());
    Ok(())
}
