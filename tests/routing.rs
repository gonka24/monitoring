use base64::{Engine, engine::general_purpose::STANDARD};
use monitoring::routing::{
    CancellationToken, DealRoutingResult, DealRoutingState as State, RoutingClient,
    RoutingDeployment, RoutingError as Error, RoutingOptions, RoutingTransport,
    parse_deal_routing_result,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const DEAL: &str = "gonka19q5apa9m2zu4qjytr22ga4trk49szya52cepk0p5lsgt520xypsqv252hy";
const HOST: &str = "gonka1x2qapwtlprmlcxrdtvrskswuznn8v29r4se5tc";
const NODE: &str = "https://node1.gonka.ai:8443";

struct Step {
    path: String,
    reply: Result<Value, Error>,
    delay: Duration,
}
#[derive(Default)]
struct Mock {
    node: Option<String>,
    steps: Mutex<VecDeque<Step>>,
    calls: AtomicUsize,
    active: AtomicUsize,
}
struct Active<'a>(&'a AtomicUsize);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl RoutingTransport for Mock {
    fn get<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let step = self
                .steps
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .pop_front()
                .unwrap_or_else(|| panic!("Unexpected request: {url}"));
            let expected = format!("{}{}", self.node.as_deref().unwrap_or(NODE), step.path);
            assert_eq!(
                reqwest::Url::parse(url).unwrap_or_else(|e| panic!("Invalid URL {url}: {e}")),
                reqwest::Url::parse(&expected).unwrap_or_else(|e| panic!("{e}")),
            );
            self.active.fetch_add(1, Ordering::SeqCst);
            let _active = Active(&self.active);
            tokio::time::sleep(step.delay).await;
            step.reply
        })
    }
}
fn smart(address: &str, query: &Value) -> String {
    let mut url = reqwest::Url::parse(&format!(
        "{NODE}/chain-api/cosmwasm/wasm/v1/contract/{address}/smart/"
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    url.path_segments_mut()
        .unwrap_or_else(|()| panic!("URL"))
        .pop_if_empty()
        .push(&STANDARD.encode(query.to_string()));
    url.as_str()[NODE.len()..].into()
}
fn step(path: &str, value: Value) -> Step {
    Step {
        path: path.into(),
        reply: Ok(value),
        delay: Duration::ZERO,
    }
}
fn tip(height: &str) -> Value {
    json!({"result":{"node_info":{"network":"gonka-mainnet"},"sync_info":{"catching_up":false,"latest_block_height":height,"latest_block_time":time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_default()}}})
}
fn attempt(current: u64, target: u64, entries: Value) -> Vec<Step> {
    let d = RoutingDeployment::mainnet().unwrap_or_else(|e| panic!("{e}"));
    vec![
        step("/chain-rpc/status", tip("9007199254740993")),
        step(
            "/chain-api/productscience/inference/inference/get_current_epoch",
            json!({"epoch":current.to_string()}),
        ),
        step(
            &format!("/chain-api/cosmwasm/wasm/v1/contract/{DEAL}"),
            json!({"address":DEAL,"contract_info":{"code_id":"119","creator":d.factory,"admin":""}}),
        ),
        step(
            &smart(DEAL, &json!({"config":{}})),
            json!({"data":{"factory":d.factory,"deal_address":DEAL,"host":HOST,"target_epoch":target,"settlement_cw20":d.token,"fee_recipient":d.fee_recipient,"fee_bps":150,"pinned_gonka_sha":d.proto_sha}}),
        ),
        step(
            &smart(
                &d.factory,
                &json!({"deal_by_host_epoch":{"host":HOST,"epoch":target}}),
            ),
            json!({"data":{"address":DEAL}}),
        ),
        step(
            &format!("/chain-api/productscience/inference/inference/list_claim_recipients/{HOST}"),
            Value::Object(serde_json::Map::from_iter([("entries".into(), entries)])),
        ),
        step(
            "/chain-api/productscience/inference/inference/get_current_epoch",
            json!({"epoch":current.to_string()}),
        ),
        step("/chain-rpc/status", tip("9007199254740994")),
    ]
}
fn client(steps: Vec<Step>) -> Result<(RoutingClient, Arc<Mock>), Error> {
    let mock = Arc::new(Mock {
        steps: Mutex::new(steps.into()),
        ..Mock::default()
    });
    Ok((
        RoutingClient::new(RoutingDeployment::mainnet()?, NODE, mock.clone())?,
        mock,
    ))
}
async fn observe(steps: Vec<Step>) -> Result<DealRoutingResult, Error> {
    client(steps)?
        .0
        .get_deal_routing(DEAL, RoutingOptions::default())
        .await
}
fn body(steps: &mut [Step], i: usize) -> &mut Value {
    steps[i].reply.as_mut().unwrap_or_else(|e| panic!("{e}"))
}

#[tokio::test]
async fn normalized_node_urls_preserve_origin_and_query_paths() -> TestResult {
    for node in [
        "https://node1.gonka.ai:443",
        "http://localhost:80",
        "https://é.com",
        "http://[0:0:0:0:0:0:0:1]:8080",
    ] {
        let mut deployment = RoutingDeployment::mainnet()?;
        deployment.nodes = vec![node.into()];
        let transport = Arc::new(Mock {
            node: Some(node.into()),
            steps: Mutex::new(attempt(437, 442, json!([{"epoch":442,"recipient":DEAL}])).into()),
            ..Mock::default()
        });
        let client = RoutingClient::new(deployment, node, transport.clone())?;
        let result = client
            .get_deal_routing(DEAL, RoutingOptions::default())
            .await?;
        assert_eq!(result.state, State::Configured);
        assert_eq!(result.evidence.node, node);
        assert_eq!(transport.calls.load(Ordering::SeqCst), 8);
        assert!(
            transport
                .steps
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .is_empty()
        );
    }
    Ok(())
}

#[tokio::test]
async fn all_classifications_and_roundtrip_without_state_or_runtime_reads() -> TestResult {
    for (current, entries, expected, recipient) in [
        (
            437,
            json!([{"epoch":"441","recipient":HOST},{"epoch":"442","recipient":DEAL},{"epoch":440,"recipient":HOST}]),
            State::Configured,
            Some(DEAL),
        ),
        (
            437,
            json!([{"epoch":442,"recipient":HOST}]),
            State::OtherRecipient,
            Some(HOST),
        ),
        (437, json!([]), State::NotConfigured, None),
        (442, json!([]), State::HistoryUnknown, None),
        (443, json!([]), State::HistoryUnknown, None),
        (
            443,
            json!([{"epoch":"442","recipient":DEAL}]),
            State::Configured,
            Some(DEAL),
        ),
    ] {
        let result = observe(attempt(current, 442, entries)).await?;
        assert_eq!(result.state, expected);
        assert_eq!(result.recipient.as_deref(), recipient);
        assert_eq!(result.evidence.start_height, "9007199254740993");
        assert_eq!(
            parse_deal_routing_result(serde_json::to_value(&result)?)?,
            result
        );
    }
    Ok(())
}

#[tokio::test]
async fn no_cache_and_precise_full_u64_epochs() -> TestResult {
    let mut steps = attempt(
        u64::MAX - 1,
        u64::MAX,
        json!([{"epoch":u64::MAX.to_string(),"recipient":DEAL}]),
    );
    steps.extend(attempt(u64::MAX, u64::MAX, json!([])));
    let (client, mock) = client(steps)?;
    let first = client
        .get_deal_routing(DEAL, RoutingOptions::default())
        .await?;
    assert_eq!(first.target_epoch, u64::MAX.to_string());
    assert_eq!(first.state, State::Configured);
    assert_eq!(
        client
            .get_deal_routing(DEAL, RoutingOptions::default())
            .await?
            .state,
        State::HistoryUnknown
    );
    assert_eq!(mock.calls.load(Ordering::SeqCst), 16);
    Ok(())
}

#[tokio::test]
async fn rejects_invalid_routes_and_integer_representations() {
    for entries in [
        json!([{"epoch":442,"recipient":DEAL},{"epoch":"442","recipient":HOST}]),
        json!([{"epoch":440,"recipient":""}]),
        json!([{"epoch":442,"recipient":"gonka1bad"}]),
        json!([{"epoch":442.5,"recipient":DEAL}]),
        json!([{"epoch":"18446744073709551616","recipient":DEAL}]),
        json!([{"epoch":"0442","recipient":DEAL}]),
        json!([{"epoch":-1,"recipient":DEAL}]),
    ] {
        assert_eq!(
            observe(attempt(437, 442, entries)).await,
            Err(Error::RoutingInvalid)
        );
    }
    for entries in [Value::Null, json!({}), json!("bad")] {
        assert_eq!(
            observe(attempt(437, 442, entries)).await,
            Err(Error::Incomplete)
        );
    }
}

#[tokio::test]
async fn complete_identity_profile_and_registration() {
    for (index, path, value) in [
        (2, vec!["address"], json!(HOST)),
        (2, vec!["contract_info", "code_id"], json!(120)),
        (2, vec!["contract_info", "admin"], json!(HOST)),
        (2, vec!["contract_info", "creator"], json!(HOST)),
        (3, vec!["data", "factory"], json!(HOST)),
        (3, vec!["data", "deal_address"], json!(HOST)),
        (3, vec!["data", "settlement_cw20"], json!(HOST)),
        (3, vec!["data", "fee_recipient"], json!(DEAL)),
        (3, vec!["data", "fee_bps"], json!(151)),
        (3, vec!["data", "pinned_gonka_sha"], json!("wrong")),
        (3, vec!["data", "host"], json!("bad")),
    ] {
        let mut steps = attempt(437, 442, json!([]));
        let mut field = body(&mut steps, index);
        for key in path {
            field = &mut field[key];
        }
        *field = value;
        assert_eq!(observe(steps).await, Err(Error::DealMismatch));
    }
    for registration in [json!({"address":HOST}), json!(null), json!({})] {
        let mut steps = attempt(437, 442, json!([]));
        body(&mut steps, 4)["data"] = registration;
        assert_eq!(observe(steps).await, Err(Error::DealUnregistered));
    }
}

#[tokio::test]
async fn network_freshness_and_response_errors_at_every_stage() {
    for index in [0, 7] {
        for (field, value, error) in [
            ("catching_up", json!(true), Error::StaleRpc),
            ("latest_block_time", json!("bad"), Error::StaleRpc),
            (
                "latest_block_time",
                json!("2000-01-01T00:00:00Z"),
                Error::StaleRpc,
            ),
            (
                "latest_block_time",
                json!("2999-01-01T00:00:00Z"),
                Error::StaleRpc,
            ),
            (
                "latest_block_height",
                json!("18446744073709551616"),
                Error::Incomplete,
            ),
            ("latest_block_height", json!(0), Error::Incomplete),
        ] {
            let mut steps = attempt(437, 442, json!([]));
            body(&mut steps, index)["result"]["sync_info"][field] = value;
            assert_eq!(observe(steps).await, Err(error));
        }
        let mut steps = attempt(437, 442, json!([]));
        body(&mut steps, index)["result"]["node_info"]["network"] = json!("other");
        assert_eq!(observe(steps).await, Err(Error::WrongChain));
    }
    for index in 0..8 {
        for error in [Error::RpcError, Error::RpcUnavailable, Error::Incomplete] {
            let mut steps = attempt(437, 442, json!([]));
            steps[index].reply = Err(error);
            assert_eq!(observe(steps).await, Err(error));
        }
        let mut steps = attempt(437, 442, json!([]));
        steps[index].reply = Ok(json!({"error":{"message":"private upstream details"}}));
        assert_eq!(observe(steps).await, Err(Error::RpcError));
    }
}

#[tokio::test]
async fn bounded_full_retry_and_height_regression() -> TestResult {
    let mut steps = attempt(437, 442, json!([]));
    body(&mut steps, 6)["epoch"] = json!(438);
    let mut second = attempt(438, 442, json!([{"epoch":442,"recipient":DEAL}]));
    body(&mut second, 0)["result"]["sync_info"]["latest_block_height"] = json!("9007199254740994");
    steps.extend(second);
    assert_eq!(observe(steps).await?.state, State::Configured);
    let mut steps = attempt(437, 442, json!([]));
    body(&mut steps, 6)["epoch"] = json!(438);
    let mut second = attempt(438, 442, json!([]));
    body(&mut second, 0)["result"]["sync_info"]["latest_block_height"] = json!("9007199254740994");
    body(&mut second, 6)["epoch"] = json!(439);
    steps.extend(second);
    let (client, mock) = client(steps)?;
    assert_eq!(
        client
            .get_deal_routing(DEAL, RoutingOptions::default())
            .await,
        Err(Error::ObservationChanged)
    );
    assert_eq!(mock.calls.load(Ordering::SeqCst), 16);
    let mut steps = attempt(437, 442, json!([]));
    body(&mut steps, 7)["result"]["sync_info"]["latest_block_height"] = json!("9007199254740992");
    assert_eq!(observe(steps).await, Err(Error::ObservationChanged));
    Ok(())
}

#[tokio::test]
async fn invalid_input_and_preabort_make_no_requests() -> TestResult {
    let (client, mock) = client(vec![])?;
    for address in [
        "bad",
        "gonka1bad",
        "",
        "GONKA19Q5APA9M2ZU4QJYTR22GA4TRK49SZYA52CEPK0P5LSGT520XYPSQV252HY",
    ] {
        assert_eq!(
            client
                .get_deal_routing(address, RoutingOptions::default())
                .await,
            Err(Error::InvalidAddress)
        );
    }
    assert_eq!(
        client
            .get_deal_routing(
                DEAL,
                RoutingOptions {
                    timeout_ms: 0,
                    ..RoutingOptions::default()
                }
            )
            .await,
        Err(Error::InvalidArgument)
    );
    let token = CancellationToken::new();
    token.cancel();
    assert_eq!(
        client
            .get_deal_routing(
                DEAL,
                RoutingOptions {
                    cancellation: token,
                    ..RoutingOptions::default()
                }
            )
            .await,
        Err(Error::Aborted)
    );
    assert_eq!(mock.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn abort_and_timeout_drop_every_inflight_stage() -> TestResult {
    for index in 0..8 {
        for abort in [false, true] {
            let mut steps = attempt(437, 442, json!([]));
            steps[index].delay = Duration::from_secs(60);
            let (client, mock) = client(steps)?;
            let cancellation = CancellationToken::new();
            let options = RoutingOptions {
                cancellation: cancellation.clone(),
                ..RoutingOptions::default()
            };
            let operation = client.get_deal_routing(DEAL, options);
            let cancel = async {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if abort {
                    cancellation.cancel();
                }
            };
            let (result, ()) = tokio::join!(operation, cancel);
            assert_eq!(
                result,
                Err(if abort {
                    Error::Aborted
                } else {
                    Error::RpcTimeout
                })
            );
            assert_eq!(mock.active.load(Ordering::SeqCst), 0);
            assert_eq!(mock.calls.load(Ordering::SeqCst), index + 1);
        }
    }
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn total_deadline_covers_chain_and_retry() -> TestResult {
    let mut steps = attempt(437, 442, json!([]));
    body(&mut steps, 6)["epoch"] = json!(438);
    let mut second = attempt(438, 442, json!([]));
    body(&mut second, 0)["result"]["sync_info"]["latest_block_height"] = json!("9007199254740994");
    steps.extend(second);
    for step in &mut steps {
        step.delay = Duration::from_secs(1);
    }
    let (client, mock) = client(steps)?;
    assert_eq!(
        client
            .get_deal_routing(DEAL, RoutingOptions::default())
            .await,
        Err(Error::RpcTimeout)
    );
    assert!(mock.calls.load(Ordering::SeqCst) <= 13);
    assert_eq!(mock.active.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test]
async fn parser_rejects_inconsistent_wire_results() -> TestResult {
    let result = observe(attempt(437, 442, json!([{"epoch":442,"recipient":DEAL}]))).await?;
    let original = serde_json::to_value(result)?;
    for (key, value) in [
        ("schema", json!("v2")),
        ("recipient", json!(HOST)),
        ("recipient", Value::Null),
        ("target_epoch", json!(442)),
        ("current_epoch", json!("01")),
        ("checked_at", json!(-1)),
        ("source", json!("cache")),
        ("host", json!("bad")),
        ("state", json!("NOT_CONFIGURED")),
    ] {
        let mut value_to_parse = original.clone();
        value_to_parse[key] = value;
        assert_eq!(
            parse_deal_routing_result(value_to_parse),
            Err(Error::Incomplete)
        );
    }
    for (key, value) in [
        ("end_height", json!("1")),
        ("registration_verified", json!(false)),
        ("node", json!("https://user:secret@example.org")),
        ("factory", json!("bad")),
    ] {
        let mut value_to_parse = original.clone();
        value_to_parse["evidence"][key] = value;
        assert_eq!(
            parse_deal_routing_result(value_to_parse),
            Err(Error::Incomplete)
        );
    }
    let mut missing = original;
    missing.as_object_mut().ok_or("object")?.remove("recipient");
    assert_eq!(parse_deal_routing_result(missing), Err(Error::Incomplete));
    Ok(())
}

struct SharedMock {
    routes: Mutex<Vec<(String, Value)>>,
    calls: AtomicUsize,
    active: AtomicUsize,
}
impl RoutingTransport for SharedMock {
    fn get<'a>(
        &'a self,
        url: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<Value, Error>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.active.fetch_add(1, Ordering::SeqCst);
            let _active = Active(&self.active);
            tokio::time::sleep(Duration::from_millis(10)).await;
            self.routes
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .iter()
                .find(|(path, _)| url == format!("{NODE}{path}"))
                .map(|(_, v)| v.clone())
                .ok_or(Error::Incomplete)
        })
    }
}
fn shared_client() -> Result<(RoutingClient, Arc<SharedMock>), Error> {
    let routes = attempt(437, 442, json!([]))
        .into_iter()
        .map(|s| Ok((s.path, s.reply?)))
        .collect::<Result<Vec<_>, Error>>()?;
    let transport = Arc::new(SharedMock {
        routes: Mutex::new(routes),
        calls: AtomicUsize::new(0),
        active: AtomicUsize::new(0),
    });
    Ok((
        RoutingClient::new(RoutingDeployment::mainnet()?, NODE, transport.clone())?,
        transport,
    ))
}

#[tokio::test(start_paused = true)]
async fn concurrent_observations_and_cancellation_are_isolated() -> TestResult {
    let (client, transport) = shared_client()?;
    let cancellation = CancellationToken::new();
    let options = RoutingOptions {
        cancellation: cancellation.clone(),
        ..RoutingOptions::default()
    };
    let cancel = async {
        tokio::time::sleep(Duration::from_millis(35)).await;
        cancellation.cancel();
    };
    let (first, second, ()) = tokio::join!(
        client.get_deal_routing(DEAL, options),
        client.get_deal_routing(DEAL, RoutingOptions::default()),
        cancel
    );
    assert_eq!(first, Err(Error::Aborted));
    assert_eq!(second?.state, State::NotConfigured);
    assert_eq!(transport.active.load(Ordering::SeqCst), 0);
    assert_eq!(
        client
            .get_deal_routing(DEAL, RoutingOptions::default())
            .await?
            .state,
        State::NotConfigured
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn dropping_operation_releases_requests() -> TestResult {
    let (client, transport) = shared_client()?;
    let mut operation = Box::pin(client.get_deal_routing(DEAL, RoutingOptions::default()));
    tokio::select! {
        result = &mut operation => panic!("premature completion: {result:?}"),
        () = tokio::time::sleep(Duration::from_millis(5)) => {},
    }
    assert_eq!(transport.active.load(Ordering::SeqCst), 1);
    drop(operation);
    assert_eq!(transport.active.load(Ordering::SeqCst), 0);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn http_endpoint_contract_errors_limits_and_no_store() -> TestResult {
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    let (client, _) = shared_client()?;
    let app = monitoring::create_app(monitoring::cache::Cache::default().with_routing(client));
    for (path, status) in [
        (format!("/v1/deals/{DEAL}/routing"), StatusCode::OK),
        ("/v1/deals/bad/routing".into(), StatusCode::BAD_REQUEST),
        (
            format!("/v1/deals/{DEAL}/routing?host={HOST}"),
            StatusCode::BAD_REQUEST,
        ),
        (
            format!("/v1/deals/{DEAL}/routing?deal={DEAL}&deal={DEAL}"),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty())?)
            .await?;
        assert_eq!(response.status(), status);
        assert_eq!(response.headers()["cache-control"], "no-store");
        if status == StatusCode::OK {
            parse_deal_routing_result(serde_json::from_slice(
                &to_bytes(response.into_body(), 16384).await?,
            )?)?;
        }
    }
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..9 {
        let app = app.clone();
        let request = Request::builder()
            .uri(format!("/v1/deals/{DEAL}/routing"))
            .body(Body::empty())?;
        tasks.spawn(async move { app.oneshot(request).await });
    }
    let mut limited = 0;
    while let Some(result) = tasks.join_next().await {
        let response = result??;
        if response.status() == StatusCode::TOO_MANY_REQUESTS {
            limited += 1;
            assert_eq!(response.headers()["retry-after"], "1");
        } else {
            assert_eq!(response.status(), StatusCode::OK);
        }
    }
    assert_eq!(limited, 1);
    let error = axum::response::IntoResponse::into_response(Error::RpcError);
    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.headers()["cache-control"], "no-store");
    let body: Value = serde_json::from_slice(&to_bytes(error.into_body(), 16384).await?)?;
    assert_eq!(body["error"]["code"], "RPC_ERROR");
    assert_eq!(body["error"]["message"], "Could not verify routing");
    Ok(())
}

#[tokio::test]
async fn actual_http_transport_rejects_http_and_json_failures() -> TestResult {
    use axum::{Router, http::StatusCode, routing::get};
    let app = Router::new()
        .route("/http", get(|| async { StatusCode::BAD_GATEWAY }))
        .route("/json", get(|| async { "not JSON" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = format!("http://{}", listener.local_addr()?);
    let server = tokio::spawn(async move { axum::serve(listener, app).await });
    let transport = monitoring::routing::HttpRoutingTransport::new()?;
    assert_eq!(
        transport.get(&format!("{endpoint}/http")).await,
        Err(Error::RpcError)
    );
    assert_eq!(
        transport.get(&format!("{endpoint}/json")).await,
        Err(Error::Incomplete)
    );
    server.abort();
    Ok(())
}
