use axum::{
    Json, Router,
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use monitoring::pool::NodePool;
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn response(uri: &Uri, broken: bool, completed: &AtomicUsize) -> Response {
    if uri.path() == "/chain-rpc/status" {
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        return Json(json!({"result":{"node_info":{"network":"gonka-mainnet","other":{"tx_index":"on"}},
            "sync_info":{"catching_up":false,"latest_block_height":"1000","latest_block_time":now}}})).into_response();
    }
    if uri.path() == "/chain-rpc/tx_search" {
        completed.fetch_add(1, Ordering::Relaxed);
        return Json(json!({"result":{"total_count":"0","txs":[]}})).into_response();
    }
    let encoded = uri.path().rsplit('/').next().unwrap_or_default();
    let q: Value =
        serde_json::from_slice(&STANDARD.decode(encoded).unwrap_or_default()).unwrap_or_default();
    if q.get("list_deals").is_some() && (!broken || q["list_deals"]["start_after"] == 1) {
        return if broken {
            StatusCode::SERVICE_UNAVAILABLE.into_response()
        } else {
            Json(json!({"data":{"deals":[]}})).into_response()
        };
    }
    let fixture = if q.get("config").is_some() {
        include_str!("fixtures/deal-config.json")
    } else if q.get("state").is_some() {
        include_str!("fixtures/deal-state.json")
    } else {
        include_str!("fixtures/factory-deals.json")
    };
    Json(serde_json::from_str::<Value>(fixture).unwrap_or_default()).into_response()
}

#[tokio::test]
async fn discards_partial_snapshot_before_retrying_another_node()
-> Result<(), Box<dyn std::error::Error>> {
    let completed = Arc::new(AtomicUsize::new(0));
    let mut bases = Vec::new();
    let mut servers = Vec::new();
    for broken in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        bases.push(format!("http://{}", listener.local_addr()?));
        let counter = completed.clone();
        let routes = Router::new().fallback(get(move |uri: Uri| {
            let counter = counter.clone();
            async move { response(&uri, broken, &counter) }
        }));
        servers.push(tokio::spawn(
            async move { axum::serve(listener, routes).await },
        ));
    }
    let result = NodePool::new(&bases)?.collect().await;
    for server in servers {
        server.abort();
    }
    let snapshot = result.map_err(|e| format!("{e:?}"))?;
    assert_eq!(
        completed.load(Ordering::Relaxed),
        1,
        "first node completed one deal before failing"
    );
    assert!(
        snapshot.deals.is_empty(),
        "partial deals must not leak across nodes"
    );
    assert!(
        snapshot.histories.is_empty(),
        "partial histories must not leak across nodes"
    );
    Ok(())
}
