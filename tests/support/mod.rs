use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use monitoring::{
    cache::Cache, chain::ChainClient, factory::ADDRESS, rpc::RpcClient, search::NetworkSearch,
};
use serde_json::{Value, json};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

pub type TestResult = Result<(), Box<dyn std::error::Error>>;
pub fn address(n: u64) -> String {
    let mut bytes = [0_u8; 32];
    bytes[..8].copy_from_slice(&n.to_be_bytes());
    bech32::encode::<bech32::Bech32>(
        bech32::Hrp::parse("gonka").unwrap_or_else(|e| panic!("{e}")),
        &bytes,
    )
    .unwrap_or_default()
}
pub fn record(id: u64, host: u64, buyer: Option<u64>) -> Value {
    json!({"id":id,"address":address(id),"host":address(host),"buyer":buyer.map(address),"factory":ADDRESS,"status":"funded"})
}
#[derive(Clone)]
pub struct Node {
    pub records: Arc<Vec<Value>>,
    pub mode: Arc<AtomicUsize>,
    pub calls: Arc<Mutex<Vec<String>>>,
}
pub struct Mock {
    pub app: Router,
    pub node: Node,
    server: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}
impl Drop for Mock {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Mock {
    pub async fn new(records: Vec<Value>) -> Result<Self, Box<dyn std::error::Error>> {
        let node = Node {
            records: Arc::new(records),
            mode: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(Mutex::new(vec![])),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}", listener.local_addr()?);
        let routes = Router::new()
            .route("/status", get(status))
            .route("/tx_search", get(events))
            .route(
                "/cosmwasm/wasm/v1/contract/{address}/smart/{query}",
                get(smart),
            )
            .with_state(node.clone());
        let server = tokio::spawn(async move { axum::serve(listener, routes).await });
        let search = NetworkSearch::new(ChainClient::new(&url)?, RpcClient::new(&url)?);
        Ok(Self {
            app: monitoring::create_app(Cache::with_search(search)),
            node,
            server,
        })
    }
}
fn log(node: &Node, value: String) {
    node.calls
        .lock()
        .unwrap_or_else(|e| panic!("{e}"))
        .push(value);
}
async fn status(State(node): State<Node>) -> Json<Value> {
    log(&node, "status".into());
    let indexing = if node.mode.load(Ordering::Relaxed) == 1 {
        "off"
    } else {
        "on"
    };
    Json(
        json!({"result":{"sync_info":{"latest_block_height":"1000","catching_up":false},"node_info":{"other":{"tx_index":indexing}}}}),
    )
}
fn attr(key: &str, value: &str) -> Value {
    json!({"key":key,"value":value})
}
fn event(record: &Value, funded: bool) -> Value {
    let addr = record["address"].as_str().unwrap_or_default();
    let mut attrs = vec![
        attr("_contract_address", if funded { addr } else { ADDRESS }),
        attr("deal", addr),
        attr("host", record["host"].as_str().unwrap_or_default()),
        attr("deal_id", &record["id"].to_string()),
    ];
    if funded {
        attrs.push(attr("buyer", record["buyer"].as_str().unwrap_or_default()));
    }
    json!({"type":if funded {"wasm-deal_funded"} else {"wasm-offer_created"},"attributes":attrs})
}
fn value_after(query: &str, key: &str) -> Option<String> {
    query
        .split_once(&format!("{key}='"))?
        .1
        .split('\'')
        .next()
        .map(str::to_owned)
}
async fn events(State(node): State<Node>, Query(params): Query<Value>) -> Json<Value> {
    let query = serde_json::from_str::<String>(params["query"].as_str().unwrap_or_default())
        .unwrap_or_default();
    log(&node, format!("events:{query}"));
    if !query.ends_with(" AND tx.height <= 1000")
        || params["order_by"] != "\"desc\""
        || params["per_page"] != "20"
    {
        return Json(json!({"error":{}}));
    }
    let funded = query.starts_with("wasm-deal_funded.buyer=");
    let found: Vec<_> = node
        .records
        .iter()
        .rev()
        .filter(|r| {
            if funded {
                value_after(&query, "wasm-deal_funded.buyer").is_some_and(|v| r["buyer"] == v)
            } else if let Some(address) = value_after(&query, "wasm-offer_created.deal") {
                r["address"] == address && r["factory"] == ADDRESS
            } else {
                value_after(&query, "wasm-offer_created.host").is_some_and(|v| r["host"] == v)
                    && r["factory"] == ADDRESS
            }
        })
        .collect();
    let page = params["page"]
        .as_str()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let txs:Vec<_>=found.iter().enumerate().skip((page-1)*20).take(20).map(|(i,r)| {
        let mut ev=vec![event(r,funded)];
        if node.mode.load(Ordering::Relaxed)==3 {ev[0]["attributes"][0]["value"]=json!(address(999));}
        if node.mode.load(Ordering::Relaxed)==4 {ev.insert(0,json!({"type":"message","attributes":[attr("sender",r["host"].as_str().unwrap_or_default())]}));}
        if node.mode.load(Ordering::Relaxed)==5 && i==0 {ev.splice(0..0,(0..1001).map(|_|json!({"type":"noise","attributes":[]})));}
        json!({"hash":format!("TX{i}"),"height":(900-i).to_string(),"tx_result":{"code":0,"events":ev}})
    }).collect();
    let total = if node.mode.load(Ordering::Relaxed) == 2 {
        found.len() + 1
    } else {
        found.len()
    };
    Json(json!({"result":{"total_count":total.to_string(),"txs":txs}}))
}
async fn smart(
    State(node): State<Node>,
    Path((address, encoded)): Path<(String, String)>,
) -> Json<Value> {
    let query: Value =
        serde_json::from_slice(&STANDARD.decode(encoded).unwrap_or_default()).unwrap_or_default();
    log(&node, format!("smart:{address}:{query}"));
    if address == ADDRESS {
        let found = node.records.iter().find(|r| r["id"] == query["deal"]["id"]);
        return Json(found.map_or_else(
            || json!({"error":{}}),
            |r| json!({"data":{"address":r["address"]}}),
        ));
    }
    let Some(r) = node.records.iter().find(|r| r["address"] == address) else {
        return Json(json!({"error":{}}));
    };
    if node.mode.load(Ordering::Relaxed) == 6 {
        return Json(json!({"error":{}}));
    }
    if query.get("config").is_some() {
        Json(
            json!({"data":{"deal_address":address,"host":r["host"],"factory":r["factory"],"target_epoch":500,"settlement_cw20":ADDRESS,"buyer_budget_micro_usdt":"340282366920938463463374607431768211455","price_micro_usdt_per_gnk":"200000"}}),
        )
    } else {
        Json(json!({"data":{"status":r["status"],"buyer":r["buyer"]}}))
    }
}
