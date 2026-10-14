use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::get,
};
use monitoring::{chain::ChainClient, create_app};
use tower::ServiceExt;

#[tokio::test]
async fn returns_deal_and_maps_errors() -> Result<(), Box<dyn std::error::Error>> {
    let address = "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs";
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let client = ChainClient::new(&format!("http://{}", listener.local_addr()?))?;
    let mock = Router::new().fallback(get(|| async { include_str!("fixtures/deal-state.json") }));
    let server = tokio::spawn(async move { axum::serve(listener, mock).await });
    let app = create_app(client);
    let request = |address| {
        Request::builder()
            .uri(format!("/v1/deals/{address}"))
            .body(Body::empty())
    };
    let response = app.clone().oneshot(request(address)?).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 1024).await?)?;
    assert_eq!(body["status"], "releasing");
    assert_eq!(
        body["buyer"],
        "gonka1ww2p5wv8plwte5tr00spkv0vt2hzr9s9kppd0x"
    );
    server.abort();
    let _ = server.await;
    for (address, expected) in [
        ("bad", StatusCode::BAD_REQUEST),
        (address, StatusCode::BAD_GATEWAY),
    ] {
        let response = app.clone().oneshot(request(address)?).await?;
        assert_eq!(response.status(), expected);
    }
    Ok(())
}
