use axum::{Json, Router, extract::Query, routing::get};
use monitoring::{history, rpc::RpcClient};
use serde_json::{Value, json};

#[tokio::test]
async fn collects_pages_and_rejects_partial_history() -> Result<(), Box<dyn std::error::Error>> {
    let address = "gonka1kxlsujae2mgahx2grd8hq8clhsk2zwsqk8q8esx6n2cpj426sdts9cdejs";
    let event: Value = serde_json::from_str(include_str!("fixtures/deal-funded.json"))?;
    for mode in ["ok", "duplicate", "changed", "short", "limit", "error"] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let client = RpcClient::new(&format!("http://{}", listener.local_addr()?))?;
        let event = event.clone();
        let app = Router::new().route(
            "/tx_search",
            get(move |Query(q): Query<Value>| {
                let event = event.clone();
                async move {
                    let second = q["page"] == "2";
                    let total = if mode == "limit" {
                        1001
                    } else {
                        21 + usize::from(second && mode == "changed")
                    };
                    let start = if second && mode != "duplicate" { 20 } else { 0 };
                    let short = mode == "short";
                    let count = if second { usize::from(!short) } else { 20 };
                    let txs: Vec<_> = (start..start + count)
                        .map(|i| {
                            json!({
                                "hash": format!("{i:064X}"), "height": "1",
                                "tx_result": {"code": u32::from(i == 0), "events": [event]}
                            })
                        })
                        .collect();
                    Json(if second && mode == "error" {
                        json!({"error": {"code": -1}})
                    } else {
                        json!({"result": {"txs": txs, "total_count": total.to_string()}})
                    })
                }
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await });
        let result = history::collect(&client, address).await;
        server.abort();
        if mode == "ok" {
            let events = result.map_err(|e| format!("{e:?}"))?;
            assert_eq!(events.len(), 20);
            assert_eq!(events[19]["transaction_hash"], format!("{:064X}", 20));
        } else {
            assert!(result.is_err(), "{mode}");
        }
    }
    Ok(())
}
