use crate::deal::DealState;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone)]
pub struct ChainClient {
    http: reqwest::Client,
    endpoint: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChainError {
    InvalidAddress,
    Upstream,
}

#[derive(Deserialize)]
struct SmartResponse<T> {
    data: T,
}

impl ChainClient {
    /// Builds a reusable client for a trusted REST endpoint.
    /// # Errors
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(endpoint: &str) -> Result<Self, reqwest::Error> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            endpoint: endpoint.trim_end_matches('/').to_owned(),
        })
    }

    /// Reads state; address filtering is lexical, not a Bech32 checksum check.
    /// # Errors
    /// Rejects malformed addresses and transport, HTTP or JSON failures.
    pub async fn state(&self, address: &str) -> Result<DealState, ChainError> {
        self.query(address, json!({"state": {}})).await
    }

    /// Reads immutable conditions of a Deal.
    /// # Errors
    /// Rejects malformed addresses and upstream failures.
    pub async fn config(&self, address: &str) -> Result<crate::config::DealConfig, ChainError> {
        self.query(address, json!({"config": {}})).await
    }

    /// Reads one registry page from the configured pilot Factory.
    /// # Errors
    /// Returns upstream errors; each page contains at most 50 deals.
    pub async fn deals(&self, after: Option<u64>) -> Result<crate::factory::DealPage, ChainError> {
        self.query(
            crate::factory::ADDRESS,
            json!({"list_deals": {"start_after": after, "limit": 50}}),
        )
        .await
    }

    pub(crate) async fn query<T: DeserializeOwned>(
        &self,
        address: &str,
        query: Value,
    ) -> Result<T, ChainError> {
        if address.len() != 64
            || !address.starts_with("gonka1")
            || !address[6..]
                .bytes()
                .all(|b| b"qpzry9x8gf2tvdw0s3jn54khce6mua7l".contains(&b))
        {
            return Err(ChainError::InvalidAddress);
        }
        let operation = match query
            .as_object()
            .and_then(|o| o.keys().next())
            .map(String::as_str)
        {
            Some("state") => "chain.state",
            Some("config") => "chain.config",
            Some("list_deals") => "chain.list_deals",
            _ => "chain.smart_query",
        };
        let query = STANDARD.encode(query.to_string());
        let url = format!(
            "{}/cosmwasm/wasm/v1/contract/{address}/smart/{query}",
            self.endpoint
        );
        let response = self
            .http
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|error| {
                crate::diagnostics::request_error(operation, &error);
                ChainError::Upstream
            })?;
        let body: SmartResponse<T> = response.json().await.map_err(|error| {
            crate::diagnostics::request_error(operation, &error);
            ChainError::Upstream
        })?;
        Ok(body.data)
    }
}
