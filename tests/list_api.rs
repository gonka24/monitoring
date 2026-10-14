use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::get,
};
use monitoring::{chain::ChainClient, create_app};
use tower::ServiceExt;

#[tokio::test]
async fn lists_registry_and_rejects_bad_cursor() -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let client = ChainClient::new(&format!("http://{}", listener.local_addr()?))?;
    let mock = Router::new().fallback(get(|| async {
        include_str!("fixtures/factory-deals.json")
    }));
    let server = tokio::spawn(async move { axum::serve(listener, mock).await });
    let app = create_app(client);
    let request = Request::builder().uri("/v1/deals").body(Body::empty())?;
    let response = app.clone().oneshot(request).await?;
    server.abort();
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 2048).await?)?;
    assert_eq!(body["deals"][0]["id"], "1");
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
