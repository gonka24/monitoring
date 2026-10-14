use crate::deal::DealState;
use serde::Deserialize;
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
struct StateResponse {
    data: DealState,
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
        if address.len() != 64
            || !address.starts_with("gonka1")
            || !address[6..]
                .bytes()
                .all(|b| b"qpzry9x8gf2tvdw0s3jn54khce6mua7l".contains(&b))
        {
            return Err(ChainError::InvalidAddress);
        }
        let url = format!(
            "{}/cosmwasm/wasm/v1/contract/{address}/smart/eyJzdGF0ZSI6e319",
            self.endpoint
        );
        let response = self
            .http
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| ChainError::Upstream)?;
        let body: StateResponse = response.json().await.map_err(|_| ChainError::Upstream)?;
        Ok(body.data)
    }
}
