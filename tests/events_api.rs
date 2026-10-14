use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use monitoring::{
    cache::{Cache, Snapshot, now},
    create_app,
};
use serde_json::{Value, json};
use tower::ServiceExt;

#[tokio::test]
async fn serves_cached_history_and_preserves_stale_events() -> Result<(), Box<dyn std::error::Error>>
{
    let address = "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs";
    let cache = Cache::default();
    let app = create_app(cache.clone());
    let request = |address| {
        Request::builder()
            .uri(format!("/v1/deals/{address}/events"))
            .body(Body::empty())
    };
    assert_eq!(
        app.clone().oneshot(request(address)?).await?.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let event: Value = serde_json::from_str(include_str!("fixtures/deal-funded.json"))?;
    let records = vec![json!({"transaction_hash": "A", "height": "6519973",
        "event_index": 11, "event": event})];
    let updated_at = now();
    cache.publish(Snapshot {
        deals: [(1, json!({"address": address}))].into(),
        histories: [(address.into(), records.clone())].into(),
        updated_at,
    });
    for stale in [false, true] {
        if stale {
            cache.mark_failed();
        }
        let response = app.clone().oneshot(request(address)?).await?;
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
        assert_eq!(
            body,
            json!({"address": address, "events": records,
            "updated_at": updated_at, "stale": stale})
        );
    }
    assert_eq!(
        app.clone().oneshot(request("missing")?).await?.status(),
        StatusCode::NOT_FOUND
    );
    cache.publish(Snapshot {
        deals: [(1, json!({"address": address}))].into(),
        histories: [(address.into(), vec![])].into(),
        updated_at,
    });
    let response = app.oneshot(request(address)?).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), 4096).await?)?;
    assert_eq!(body["events"], json!([]));
    assert_eq!(body["stale"], false);
    Ok(())
}
