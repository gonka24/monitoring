use axum::{Router, extract::Path, routing::get};
use base64::{Engine, engine::general_purpose::STANDARD};
use monitoring::chain::ChainClient;

#[tokio::test]
async fn sends_registry_cursor() -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let client = ChainClient::new(&format!("http://{}", listener.local_addr()?))?;
    let route = format!(
        "/cosmwasm/wasm/v1/contract/{}/smart/{{query}}",
        monitoring::factory::ADDRESS
    );
    let mock = Router::new().route(
        &route,
        get(|Path(query): Path<String>| async move {
            let expected = serde_json::json!({"list_deals": {"start_after": 1, "limit": 50}});
            if STANDARD.encode(expected.to_string()) == query {
                (axum::http::StatusCode::OK, r#"{"data":{"deals":[]}}"#)
            } else {
                (axum::http::StatusCode::BAD_REQUEST, "")
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, mock).await });
    let result = client.deals(Some(1)).await;
    server.abort();
    assert_eq!(result.map(|page| page.deals), Ok(vec![]));
    Ok(())
}
