mod support;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use support::{Mock, TestResult, address, record};
use tower::ServiceExt;

async fn post(
    app: axum::Router,
    body: Value,
) -> Result<(StatusCode, Value), Box<dyn std::error::Error>> {
    post_raw(app, body.to_string()).await
}
async fn post_raw(
    app: axum::Router,
    body: String,
) -> Result<(StatusCode, Value), Box<dyn std::error::Error>> {
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v2/deals/search")
                .header("content-type", "application/json")
                .body(Body::from(body))?,
        )
        .await?;
    Ok((
        response.status(),
        serde_json::from_slice(&to_bytes(response.into_body(), 200_000).await?)?,
    ))
}
#[tokio::test]
async fn validates_before_network_and_never_falls_back_to_registry() -> TestResult {
    let mock = Mock::new(vec![]).await?;
    for (query, code) in [
        (json!({}), "INVALID_FILTER"),
        (json!({"hosts":[],"buyers":[]}), "INVALID_FILTER"),
        (json!({"hosts":null}), "INVALID_FILTER"),
        (json!({"buyers":null}), "INVALID_FILTER"),
        (json!({"hosts":"x"}), "INVALID_FILTER"),
        (json!({"hosts":[7]}), "INVALID_FILTER"),
        (json!({"hosts":[address(1),"bad"]}), "INVALID_FILTER"),
        (json!({"hosts":vec![address(1);101]}), "INVALID_FILTER"),
        (json!({"host":[address(1)]}), "INVALID_FILTER"),
        (json!({"limit":0}), "INVALID_LIMIT"),
        (json!({"limit":101}), "INVALID_LIMIT"),
        (json!({"limit":1.5}), "INVALID_LIMIT"),
        (json!({"limit":null}), "INVALID_LIMIT"),
        (json!({"cursor":42}), "INVALID_CURSOR"),
        (
            json!({"hosts":[address(1)],"cursor":"broken"}),
            "INVALID_CURSOR",
        ),
    ] {
        let (status, error) = post(mock.app.clone(), query.clone()).await?;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}: {error}");
        assert_eq!(error["error"]["code"], code);
    }
    assert_eq!(
        post_raw(mock.app.clone(), " ".repeat(32769)).await?.0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    assert_eq!(
        post_raw(mock.app.clone(), "{".into()).await?.0,
        StatusCode::BAD_REQUEST
    );
    assert!(
        mock.node
            .calls
            .lock()
            .map_err(|e| e.to_string())?
            .is_empty()
    );
    Ok(())
}
#[tokio::test]
async fn host_buyer_and_filters_use_actual_roles_and_return_details() -> TestResult {
    let mock = Mock::new(vec![
        record(1, 10, None),
        record(2, 10, Some(20)),
        record(3, 11, Some(21)),
        record(4, 11, Some(20)),
    ])
    .await?;
    for (query, mut expected) in [
        (json!({"hosts":[address(10)]}), vec![1, 2]),
        (
            json!({"hosts":[address(10),address(11),address(10)]}),
            vec![1, 2, 3, 4],
        ),
        (
            json!({"buyers":[address(20),address(21),address(20)]}),
            vec![2, 3, 4],
        ),
        (
            json!({"hosts":[address(10)],"buyers":[address(20)]}),
            vec![2],
        ),
        (json!({"hosts":[],"buyers":[address(21)]}), vec![3]),
        (json!({"hosts":[address(20)]}), vec![]),
    ] {
        let (status, result) = post(mock.app.clone(), query).await?;
        assert_eq!(status, StatusCode::OK, "{result}");
        let mut ids: Vec<_> = result["deals"]
            .as_array()
            .ok_or("deals")?
            .iter()
            .map(|d| {
                d["id"]
                    .as_str()
                    .unwrap_or_default()
                    .parse::<u64>()
                    .unwrap_or_default()
            })
            .collect();
        ids.sort_unstable();
        expected.sort_unstable();
        assert_eq!(ids, expected);
        assert_eq!(result["next_cursor"], Value::Null);
        assert_eq!(result["history_completeness"], "unknown");
        for deal in result["deals"].as_array().ok_or("deals")? {
            assert_eq!(
                deal["buyer_budget_micro_usdt"],
                "340282366920938463463374607431768211455"
            );
        }
    }
    let calls = mock.node.calls.lock().map_err(|e| e.to_string())?;
    assert!(calls.iter().all(|s| !s.contains("list_deals")));
    Ok(())
}
#[tokio::test]
async fn cursor_continues_transactions_and_accepts_normalized_filters() -> TestResult {
    let mock = Mock::new((1..=25).map(|id| record(id, 10, Some(20))).collect()).await?;
    mock.node.mode.store(4, Ordering::Relaxed);
    let mut query = json!({"hosts":[address(10)],"limit":3});
    let mut ids = Vec::new();
    let mut saved = Value::Null;
    loop {
        let (status, result) = post(mock.app.clone(), query.clone()).await?;
        assert_eq!(status, StatusCode::OK, "{result}");
        ids.extend(result["deals"].as_array().ok_or("deals")?.iter().map(|d| {
            d["id"]
                .as_str()
                .unwrap_or_default()
                .parse::<u64>()
                .unwrap_or_default()
        }));
        if saved.is_null() {
            saved = result["next_cursor"].clone();
        }
        if result["next_cursor"].is_null() {
            break;
        }
        query["hosts"] = json!([address(10), address(10)]);
        query["cursor"] = result["next_cursor"].clone();
        assert!(ids.len() <= 25);
    }
    assert_eq!(ids, (1..=25).rev().collect::<Vec<_>>());
    for value in [
        json!({"hosts":[address(11)],"limit":3,"cursor":saved}),
        json!({"hosts":[address(10)],"limit":4,"cursor":saved}),
    ] {
        assert_eq!(
            post(mock.app.clone(), value).await?.0,
            StatusCode::BAD_REQUEST
        );
    }
    Ok(())
}
#[tokio::test]
async fn foreign_emitters_and_foreign_factory_deals_are_excluded() -> TestResult {
    let mut foreign = record(1, 10, Some(20));
    foreign["factory"] = json!(address(777));
    let mock = Mock::new(vec![foreign]).await?;
    let result = post(mock.app.clone(), json!({"buyers":[address(20)]})).await?;
    assert_eq!(result.0, StatusCode::OK);
    assert_eq!(result.1["deals"], json!([]));
    assert!(
        mock.node
            .calls
            .lock()
            .map_err(|e| e.to_string())?
            .iter()
            .all(|s| !s.starts_with("smart:"))
    );
    let mock = Mock::new(vec![record(1, 10, Some(20))]).await?;
    mock.node.mode.store(3, Ordering::Relaxed);
    assert_eq!(
        post(mock.app.clone(), json!({"hosts":[address(10)]}))
            .await?
            .1["deals"],
        json!([])
    );
    Ok(())
}
#[tokio::test]
async fn upstream_errors_do_not_become_success_or_end_of_list() -> TestResult {
    let mock = Mock::new(vec![record(1, 10, Some(20))]).await?;
    for mode in [1, 2, 6] {
        mock.node.mode.store(mode, Ordering::Relaxed);
        let (status, result) = post(mock.app.clone(), json!({"hosts":[address(10)]})).await?;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{mode} {result}");
        assert_eq!(result["error"]["code"], "MONITORING_UNAVAILABLE");
    }
    Ok(())
}
#[tokio::test]
async fn bounded_work_returns_a_resumable_partial_page() -> TestResult {
    let mock = Mock::new(vec![record(1, 10, None)]).await?;
    mock.node.mode.store(5, Ordering::Relaxed);
    let mut query = json!({"hosts":[address(10)]});
    let first = post(mock.app.clone(), query.clone()).await?;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(first.1["deals"], json!([]));
    assert_eq!(first.1["partial_page"], true);
    query["cursor"] = first.1["next_cursor"].clone();
    let second = post(mock.app.clone(), query).await?;
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(second.1["deals"][0]["id"], "1");
    assert_eq!(second.1["next_cursor"], Value::Null);
    Ok(())
}
#[test]
fn validates_bech32_checksum_and_account_or_contract_length() -> TestResult {
    use bech32::{Bech32, Bech32m, Hrp};
    use monitoring::search::valid_address;
    for len in [20, 32] {
        assert!(valid_address(&bech32::encode::<Bech32>(
            Hrp::parse("gonka")?,
            &vec![7; len]
        )?));
    }
    for len in [0, 19, 21, 31, 33] {
        assert!(!valid_address(&bech32::encode::<Bech32>(
            Hrp::parse("gonka")?,
            &vec![7; len]
        )?));
    }
    assert!(!valid_address(&bech32::encode::<Bech32m>(
        Hrp::parse("gonka")?,
        &[7; 20]
    )?));
    assert!(!valid_address(&bech32::encode::<Bech32>(
        Hrp::parse("cosmos")?,
        &[7; 20]
    )?));
    let mut invalid = address(1);
    invalid.push('q');
    assert!(!valid_address(&invalid));
    Ok(())
}

#[tokio::test]
async fn changed_history_and_out_of_bounds_cursor_are_rejected() -> TestResult {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    let mock = Mock::new((1..=25).map(|id| record(id, 10, None)).collect()).await?;
    let mut query = json!({"hosts":[address(10)],"limit":1});
    let first = post(mock.app.clone(), query.clone()).await?;
    let encoded = first.1["next_cursor"].as_str().ok_or("cursor")?;
    query["cursor"] = json!(encoded);
    mock.node.mode.store(2, Ordering::Relaxed);
    assert_eq!(
        post(mock.app.clone(), query.clone()).await?.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    mock.node.mode.store(0, Ordering::Relaxed);
    let original: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(encoded)?)?;
    for (key, value) in [
        ("version", json!(2)),
        ("page", json!(0)),
        ("wallet", json!(100)),
        ("transaction", json!(20)),
        ("event", json!(9999)),
        ("height", json!(1001)),
    ] {
        let mut cursor = original.clone();
        cursor[key] = value;
        query["cursor"] = json!(URL_SAFE_NO_PAD.encode(cursor.to_string()));
        assert_eq!(
            post(mock.app.clone(), query.clone()).await?.0,
            StatusCode::BAD_REQUEST,
            "{key}"
        );
    }
    Ok(())
}
