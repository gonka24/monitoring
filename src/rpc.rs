use crate::{chain::ChainError, transaction::Transaction};
use serde::Deserialize;
use std::{num::NonZeroU32, time::Duration};

#[derive(Clone)]
pub struct RpcClient {
    http: reqwest::Client,
    endpoint: String,
}

#[derive(Debug, Deserialize)]
pub struct TxPage {
    #[serde(deserialize_with = "transactions")]
    pub txs: Vec<Transaction>,
    pub total_count: String,
}

#[derive(Deserialize)]
struct Response {
    result: Option<TxPage>,
    error: Option<serde_json::Value>,
}

impl RpcClient {
    /// Builds a client for a trusted `CometBFT` RPC endpoint.
    /// # Errors
    /// Returns HTTP client initialization errors.
    pub fn new(endpoint: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            endpoint: endpoint.trim_end_matches('/').to_owned(),
        })
    }

    /// Returns the indexed node's current height, rejecting a syncing/unindexed node.
    /// # Errors
    /// Rejects transport, RPC, indexing and synchronization failures.
    pub async fn search_height(&self) -> Result<u64, ChainError> {
        let body: serde_json::Value = self
            .http
            .get(format!("{}/status", self.endpoint))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| {
                crate::diagnostics::request_error("rpc.status", &error);
                ChainError::Upstream
            })?
            .json()
            .await
            .map_err(|error| {
                crate::diagnostics::request_error("rpc.status", &error);
                ChainError::Upstream
            })?;
        if body.get("error").is_some_and(|v| !v.is_null())
            || body["result"]["sync_info"]["catching_up"] != false
            || body["result"]["node_info"]["other"]["tx_index"] != "on"
        {
            crate::diagnostics::failure("rpc.status", "invalid_response");
            return Err(ChainError::Upstream);
        }
        body["result"]["sync_info"]["latest_block_height"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|h| *h > 0)
            .ok_or_else(|| {
                crate::diagnostics::failure("rpc.status", "invalid_response");
                ChainError::Upstream
            })
    }

    pub(crate) async fn search_events(
        &self,
        query: &str,
        page: u32,
        height: u64,
    ) -> Result<TxPage, ChainError> {
        let query = format!("{query} AND tx.height <= {height}");
        let response = self.http.get(format!("{}/tx_search", self.endpoint))
            .query(&serde_json::json!({"query":serde_json::to_string(&query).map_err(|_| ChainError::Upstream)?,
                "page":page.to_string(),"per_page":"20","order_by":"\"desc\"","prove":"false"}))
            .send().await.and_then(reqwest::Response::error_for_status).map_err(|error| { crate::diagnostics::request_error("rpc.search_events", &error); ChainError::Upstream })?;
        let body: Response = response.json().await.map_err(|error| {
            crate::diagnostics::request_error("rpc.search_events", &error);
            ChainError::Upstream
        })?;
        body.result.filter(|_| body.error.is_none()).ok_or_else(|| {
            crate::diagnostics::failure("rpc.search_events", "invalid_response");
            ChainError::Upstream
        })
    }

    /// Reads up to 20 indexed transactions in ascending order, with pages starting at 1.
    /// # Errors
    /// Rejects malformed addresses and transport, HTTP, JSON or RPC errors.
    pub async fn page(&self, address: &str, page: NonZeroU32) -> Result<TxPage, ChainError> {
        if address.len() != 64
            || !address.starts_with("gonka1")
            || !address[6..]
                .bytes()
                .all(|b| b"qpzry9x8gf2tvdw0s3jn54khce6mua7l".contains(&b))
        {
            return Err(ChainError::InvalidAddress);
        }
        let response = self
            .http
            .get(format!("{}/tx_search", self.endpoint))
            .query(&serde_json::json!({
                "query": format!("\"execute._contract_address='{address}'\""),
                "page": page.to_string(), "per_page": "20", "order_by": "\"asc\"", "prove": "false"
            }))
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| {
                crate::diagnostics::request_error("rpc.history", &error);
                ChainError::Upstream
            })?;
        let body: Response = response.json().await.map_err(|error| {
            crate::diagnostics::request_error("rpc.history", &error);
            ChainError::Upstream
        })?;
        body.result.filter(|_| body.error.is_none()).ok_or_else(|| {
            crate::diagnostics::failure("rpc.history", "invalid_response");
            ChainError::Upstream
        })
    }
}

fn transactions<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Transaction>, D::Error> {
    Ok(Option::<Vec<Transaction>>::deserialize(deserializer)?.unwrap_or_default())
}
