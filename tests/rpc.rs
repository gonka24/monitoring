use axum::{Router, extract::Query, routing::get};
use monitoring::{chain::ChainError, factory::ADDRESS, rpc::RpcClient};
use serde_json::{Value, json};
use std::num::NonZeroU32;

#[tokio::test]
async fn queries_transactions_and_rejects_rpc_errors() -> Result<(), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let client = RpcClient::new(&format!("http://{}/", listener.local_addr()?))?;
    let mock = Router::new().route(
        "/tx_search",
        get(|Query(q): Query<Value>| async move {
            let expected = json!({"query": format!("\"execute._contract_address='{ADDRESS}'\""),
                "page": "1", "per_page": "20", "order_by": "\"asc\"", "prove": "false"});
            if q == expected {
                r#"{"result":{"txs":[],"total_count":"0"}}"#
            } else {
                r#"{"error":{"code":-32603}}"#
            }
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, mock).await });
    let success = client.page(ADDRESS, NonZeroU32::MIN).await;
    let page2 = NonZeroU32::MIN.saturating_add(1);
    let failure = client.page(ADDRESS, page2).await;
    let invalid = client.page("bad'", page2).await;
    server.abort();
    assert_eq!(success.map(|p| p.total_count), Ok("0".into()));
    assert_eq!(failure.err(), Some(ChainError::Upstream));
    assert_eq!(invalid.err(), Some(ChainError::InvalidAddress));
    Ok(())
}
