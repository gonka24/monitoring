use axum::{Json, Router, http::Uri, routing::get};
use base64::{Engine, engine::general_purpose::STANDARD};
use monitoring::{chain::ChainClient, collector::collect};
use serde_json::{Value, json};

#[tokio::test]
async fn collects_pages_and_rejects_inconsistent_data() -> Result<(), Box<dyn std::error::Error>> {
    for mode in 0..5 {
        let mock = Router::new().fallback(get(move |uri: Uri| async move {
            let encoded = uri.path().rsplit('/').next().unwrap_or_default();
            let query: Value =
                serde_json::from_slice(&STANDARD.decode(encoded).unwrap_or_default())
                    .unwrap_or_default();
            let fixture = if query.get("state").is_some() {
                include_str!("fixtures/deal-state.json")
            } else if query.get("config").is_some() {
                include_str!("fixtures/deal-config.json")
            } else {
                include_str!("fixtures/factory-deals.json")
            };
            let mut body: Value = serde_json::from_str(fixture).unwrap_or_default();
            if query["list_deals"]["start_after"] == 1 && mode != 2 {
                body = json!({"data": {"deals": []}});
            }
            if query.get("config").is_some() && mode == 1 {
                body["data"]["factory"] = json!("foreign");
            }
            if query.get("config").is_some() && mode == 3 {
                body["data"]["deal_address"] = json!("foreign");
            }
            if query.get("state").is_some() && mode == 4 {
                body = json!({"data": {}});
            }
            Json(body)
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let client = ChainClient::new(&format!("http://{}", listener.local_addr()?))?;
        let server = tokio::spawn(async move { axum::serve(listener, mock).await });
        let result = collect(&client).await;
        server.abort();
        assert_eq!(result.is_ok(), mode == 0);
        if let Ok(snapshot) = result {
            assert_eq!(snapshot.deals.len(), 1);
            assert_eq!(snapshot.deals[&1]["buyer_budget_micro_usdt"], "140000000");
            assert_eq!(snapshot.deals[&1]["status"], "releasing");
        }
    }
    Ok(())
}
