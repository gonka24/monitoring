use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use monitoring::{
    cache::{Cache, Snapshot},
    create_app,
};
use tower::ServiceExt;

#[tokio::test]
async fn lists_registry_and_rejects_bad_cursor() -> Result<(), Box<dyn std::error::Error>> {
    let cache = Cache::default();
    let app = create_app(cache.clone());
    let request = Request::builder().uri("/v1/deals").body(Body::empty())?;
    assert_eq!(
        app.clone().oneshot(request).await?.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let updated_at = monitoring::cache::now();
    cache.publish(Snapshot {
        deals: (1..=51)
            .map(|id| (id, serde_json::json!({"address": "test"})))
            .collect(),
        updated_at,
    });
    cache.mark_failed();
    let request = Request::builder().uri("/v1/deals").body(Body::empty())?;
    let response = app.clone().oneshot(request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 2048).await?)?;
    assert_eq!(body["deals"][0]["id"], "1");
    assert_eq!(body["next_after"], "50");
    assert_eq!(body["deals"].as_array().map(Vec::len), Some(50));
    assert_eq!(body["stale"], true);
    assert_eq!(body["updated_at"], updated_at);
    let request = Request::builder()
        .uri("/v1/deals?after=50")
        .body(Body::empty())?;
    let response = app.clone().oneshot(request).await?;
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 2048).await?)?;
    assert_eq!(body["deals"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["deals"][0]["id"], "51");
    assert_eq!(body["next_after"], serde_json::Value::Null);
    let request = Request::builder()
        .uri("/v1/deals?after=bad")
        .body(Body::empty())?;
    assert_eq!(
        app.oneshot(request).await?.status(),
        StatusCode::BAD_REQUEST
    );
    Ok(())
}
