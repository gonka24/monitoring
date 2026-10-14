use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use monitoring::{
    cache::{Cache, Snapshot, now},
    create_app,
};
use tower::ServiceExt;

#[tokio::test]
async fn returns_deal_and_maps_errors() -> Result<(), Box<dyn std::error::Error>> {
    let address = "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs";
    let cache = Cache::default();
    let app = create_app(cache.clone());
    let request = |address| {
        Request::builder()
            .uri(format!("/v1/deals/{address}"))
            .body(Body::empty())
    };
    assert_eq!(
        app.clone().oneshot(request(address)?).await?.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/deal-state.json"))?;
    let mut record = fixture["data"].clone();
    record["address"] = address.into();
    cache.publish(Snapshot {
        deals: [(1, record)].into(),
        updated_at: now(),
    });
    let response = app.clone().oneshot(request(address)?).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await?)?;
    assert_eq!(body["status"], "releasing");
    assert_eq!(body["stale"], false);
    assert!(body["updated_at"].as_u64().is_some());
    assert_eq!(
        body["buyer"],
        "gonka1ww2p5wv8plwte5tr00spkv0vt2hzr9s9kppd0x"
    );
    cache.mark_failed();
    for (address, expected) in [("bad", StatusCode::NOT_FOUND), (address, StatusCode::OK)] {
        let response = app.clone().oneshot(request(address)?).await?;
        assert_eq!(response.status(), expected);
        if expected == StatusCode::OK {
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 2048).await?)?;
            assert_eq!(body["stale"], true);
        }
    }
    Ok(())
}
