mod support;
use axum::{
    body::{Body, to_bytes},
    http::Request,
};
use monitoring::{cache::Cache, pool::NodePool, search::NetworkSearch};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use support::{Mock, TestResult, address, record};
use tower::ServiceExt;

fn app(nodes: &[&Mock]) -> Result<axum::Router, Box<dyn std::error::Error>> {
    let pool = NodePool::new(&nodes.iter().map(|n| n.base.clone()).collect::<Vec<_>>())?;
    let search = NetworkSearch::with_pool(pool).map_err(|e| format!("{e:?}"))?;
    Ok(monitoring::create_app(Cache::with_search(search)))
}
async fn search(
    app: &axum::Router,
    cursor: Option<&Value>,
) -> Result<(u16, Value), Box<dyn std::error::Error>> {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v2/deals/search")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"hosts":[address(10)],"limit":1,"cursor":cursor}).to_string(),
                ))?,
        )
        .await?;
    let status = response.status().as_u16();
    Ok((
        status,
        serde_json::from_slice(&to_bytes(response.into_body(), 200_000).await?)?,
    ))
}
fn calls(node: &Mock) -> Vec<String> {
    node.node
        .calls
        .lock()
        .unwrap_or_else(|e| panic!("{e}"))
        .clone()
}

#[tokio::test]
async fn retries_whole_search_and_keeps_rest_rpc_on_the_successful_node() -> TestResult {
    let broken = Mock::new(vec![record(1, 10, None)]).await?;
    broken.node.mode.store(11, Ordering::Relaxed); // Valid RPC, broken smart queries.
    let healthy = Mock::new(vec![record(2, 10, None)]).await?;
    let app = app(&[&broken, &healthy])?;
    let (status, body) = search(&app, None).await?;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["deals"][0]["address"], address(2));
    assert!(calls(&broken).iter().any(|c| c.starts_with("smart:")));
    assert!(calls(&healthy).iter().any(|c| c.starts_with("events:")));
    assert!(calls(&healthy).iter().any(|c| c.starts_with("smart:")));
    let count = calls(&broken).len();
    assert_eq!(search(&app, None).await?.0, 200);
    assert_eq!(
        calls(&broken).len(),
        count,
        "cooldown must suppress failed node"
    );
    Ok(())
}

#[tokio::test]
async fn excludes_unindexed_wrong_chain_stale_and_syncing_nodes() -> TestResult {
    for mode in [1, 7, 8, 9] {
        let rejected = Mock::new(vec![record(1, 10, None)]).await?;
        rejected.node.mode.store(mode, Ordering::Relaxed);
        let healthy = Mock::new(vec![record(2, 10, None)]).await?;
        let app = app(&[&rejected, &healthy])?;
        assert_eq!(search(&app, None).await?.0, 200);
        assert_eq!(calls(&rejected), vec!["status"], "mode {mode}");
    }
    Ok(())
}

#[tokio::test]
async fn continuation_never_switches_history_even_when_another_node_is_healthy() -> TestResult {
    let first = Mock::new(vec![record(1, 10, None), record(2, 10, None)]).await?;
    let second = Mock::new(vec![record(3, 10, None), record(4, 10, None)]).await?;
    let app = app(&[&first, &second])?;
    let (status, page) = search(&app, None).await?;
    assert_eq!(status, 200);
    let cursor = &page["next_cursor"];
    assert!(cursor.is_string());
    assert_eq!(
        search(&second.app, Some(cursor)).await?.0,
        400,
        "legacy mode rejects pooled cursors"
    );
    let (status, next) = search(&app, Some(cursor)).await?;
    assert_eq!(status, 200, "{next}");
    assert_eq!(calls(&second), Vec::<String>::new());
    first.node.mode.store(6, Ordering::Relaxed);
    let (status, body) = search(&app, Some(cursor)).await?;
    assert_eq!(status, 503, "{body}");
    assert_eq!(calls(&second), Vec::<String>::new());
    // The same cursor cannot be replayed into a differently configured pool.
    let other = self::app(&[&second])?;
    assert_eq!(search(&other, Some(cursor)).await?.0, 400);
    assert_eq!(calls(&second), Vec::<String>::new());
    Ok(())
}

#[tokio::test]
async fn rejects_legacy_cursor_in_pool_and_does_not_retry_invalid_input() -> TestResult {
    let node = Mock::new(vec![record(1, 10, None), record(2, 10, None)]).await?;
    let (_, legacy) = search(&node.app, None).await?;
    let app = app(&[&node])?;
    let before = calls(&node).len();
    assert_eq!(search(&app, Some(&legacy["next_cursor"])).await?.0, 400);
    assert_eq!(search(&app, Some(&json!("broken"))).await?.0, 400);
    assert_eq!(calls(&node).len(), before);
    Ok(())
}

#[tokio::test]
async fn caps_attempts_and_returns_unavailable_when_all_candidates_fail() -> TestResult {
    let mut nodes = Vec::new();
    for _ in 0..4 {
        let node = Mock::new(vec![]).await?;
        node.node.mode.store(1, Ordering::Relaxed);
        nodes.push(node);
    }
    let app = app(&nodes.iter().collect::<Vec<_>>())?;
    assert_eq!(search(&app, None).await?.0, 503);
    assert_eq!(
        calls(&nodes[3]),
        Vec::<String>::new(),
        "only three attempts per request"
    );
    assert_eq!(search(&app, None).await?.0, 503);
    assert_eq!(calls(&nodes[3]), vec!["status"]);
    assert_eq!(search(&app, None).await?.0, 503);
    for node in &nodes {
        assert_eq!(calls(node), vec!["status"]);
    }
    Ok(())
}
