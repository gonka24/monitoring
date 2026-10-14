use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use tower::ServiceExt;

#[tokio::test]
async fn public_routes() -> Result<(), Box<dyn std::error::Error>> {
    for (path, status, body) in [
        ("/", StatusCode::OK, "Hello, world!\n"),
        ("/healthz", StatusCode::OK, "ok\n"),
        ("/missing", StatusCode::NOT_FOUND, ""),
    ] {
        let client = monitoring::chain::ChainClient::new("http://127.0.0.1:1")?;
        let response = monitoring::create_app(client)
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), status);
        assert_eq!(
            to_bytes(response.into_body(), 1024).await?.as_ref(),
            body.as_bytes()
        );
    }
    Ok(())
}
