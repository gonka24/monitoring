use crate::{chain::ChainError, transaction::Transaction};
use serde::Deserialize;
use std::{num::NonZeroU32, time::Duration};

pub struct RpcClient {
    http: reqwest::Client,
    endpoint: String,
}

#[derive(Debug, Deserialize)]
pub struct TxPage {
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
            .map_err(|_| ChainError::Upstream)?;
        let body: Response = response.json().await.map_err(|_| ChainError::Upstream)?;
        body.result
            .filter(|_| body.error.is_none())
            .ok_or(ChainError::Upstream)
    }
}
