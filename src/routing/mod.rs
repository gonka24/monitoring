//! Live, read-only routing observations. No runtime compatibility/signing assertion.
mod api;
mod model;
mod transport;

use crate::search::valid_address;
pub(crate) use api::handler;
use base64::{Engine, engine::general_purpose::STANDARD};
pub use model::{DealRoutingResult, DealRoutingState, RoutingEvidence, parse_deal_routing_result};
use model::{classify, uint};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
pub use tokio_util::sync::CancellationToken;
pub use transport::{HttpRoutingTransport, RoutingTransport};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RoutingError {
    InvalidAddress,
    InvalidArgument,
    InvalidDeployment,
    WrongChain,
    StaleRpc,
    DealMismatch,
    DealUnregistered,
    RoutingInvalid,
    Incomplete,
    RpcError,
    RpcUnavailable,
    RpcTimeout,
    Aborted,
    ObservationChanged,
    RateLimited,
}
impl std::fmt::Display for RoutingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for RoutingError {}

/// Trusted pins, never taken from an HTTP routing request.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingDeployment {
    pub id: String,
    pub chain_id: String,
    pub nodes: Vec<String>,
    pub factory: String,
    pub deal_code_id: u64,
    pub token: String,
    pub fee_recipient: String,
    pub fee_bps: u16,
    pub proto_sha: String,
}
impl RoutingDeployment {
    /// Reads the pinned profile shipped with forward-tools 0.2.0.
    /// # Errors
    /// Returns an error if the embedded profile is malformed.
    pub fn mainnet() -> Result<Self, RoutingError> {
        serde_json::from_str(include_str!("mainnet-v1.json"))
            .map_err(|_| RoutingError::InvalidDeployment)
    }
    fn validate(&self) -> Result<(), RoutingError> {
        if self.id.is_empty()
            || self.chain_id.is_empty()
            || self.nodes.is_empty()
            || !self.nodes.iter().all(|n| model::node_url(n))
            || ![&self.factory, &self.token, &self.fee_recipient]
                .into_iter()
                .all(|a| valid_address(a))
            || self.deal_code_id == 0
            || self.fee_bps > 10000
            || self.proto_sha.len() != 40
            || !self
                .proto_sha
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(RoutingError::InvalidDeployment);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct RoutingOptions {
    pub timeout_ms: u64,
    pub cancellation: CancellationToken,
}
impl Default for RoutingOptions {
    fn default() -> Self {
        Self {
            timeout_ms: 12000,
            cancellation: CancellationToken::new(),
        }
    }
}

#[derive(Clone)]
pub struct RoutingClient {
    deployment: Arc<RoutingDeployment>,
    node: String,
    transport: Arc<dyn RoutingTransport>,
    pub(super) capacity: Arc<tokio::sync::Semaphore>,
}

/// Verified immutable identity, reusable without loading state/payments/events.
#[derive(Debug)]
pub struct DealIdentity {
    pub host: String,
    pub target_epoch: u64,
}

impl RoutingClient {
    /// Pins one node for the entire observation, including the optional full retry.
    /// # Errors
    /// Rejects malformed deployment pins and nodes outside the deployment.
    pub fn new(
        deployment: RoutingDeployment,
        node: &str,
        transport: Arc<dyn RoutingTransport>,
    ) -> Result<Self, RoutingError> {
        deployment.validate()?;
        if !deployment.nodes.iter().any(|n| n == node) {
            return Err(RoutingError::InvalidDeployment);
        }
        Ok(Self {
            deployment: Arc::new(deployment),
            node: node.into(),
            transport,
            capacity: Arc::new(tokio::sync::Semaphore::new(8)),
        })
    }

    /// One live observation with at most one full retry after an epoch transition.
    /// Dropping this future cancels its requests. The explicit token returns `ABORTED`.
    /// # Errors
    /// Returns stable errors; never returns partial observations or cached results.
    pub async fn get_deal_routing(
        &self,
        deal: &str,
        options: RoutingOptions,
    ) -> Result<DealRoutingResult, RoutingError> {
        if !valid_address(deal) {
            return Err(RoutingError::InvalidAddress);
        }
        if options.timeout_ms == 0 {
            return Err(RoutingError::InvalidArgument);
        }
        let deadline = tokio::time::Instant::now()
            .checked_add(Duration::from_millis(options.timeout_ms))
            .ok_or(RoutingError::InvalidArgument)?;
        tokio::select! {
            biased;
            () = options.cancellation.cancelled() => Err(RoutingError::Aborted),
            result = tokio::time::timeout_at(deadline, self.observe(deal)) => result.map_err(|_| RoutingError::RpcTimeout)?,
        }
    }

    async fn get(&self, path: &str) -> Result<Value, RoutingError> {
        self.get_url(&format!("{}{path}", self.node)).await
    }

    async fn get_url(&self, url: &str) -> Result<Value, RoutingError> {
        let body = self.transport.get(url).await?;
        if body.get("error").is_some_and(|e| !e.is_null())
            || body.get("code").is_some_and(|v| v != 0 && v != "0")
        {
            return Err(RoutingError::RpcError);
        }
        if !body.is_object() {
            return Err(RoutingError::Incomplete);
        }
        Ok(body)
    }
    async fn smart(&self, address: &str, msg: Value) -> Result<Value, RoutingError> {
        let encoded = STANDARD.encode(msg.to_string());
        let mut url = reqwest::Url::parse(&format!(
            "{}/chain-api/cosmwasm/wasm/v1/contract/{address}/smart/",
            self.node
        ))
        .map_err(|_| RoutingError::InvalidDeployment)?;
        url.path_segments_mut()
            .map_err(|()| RoutingError::InvalidDeployment)?
            .pop_if_empty()
            .push(&encoded);
        let body = self.get_url(url.as_str()).await?;
        body.get("data").cloned().ok_or(RoutingError::Incomplete)
    }

    /// Checks the complete pinned identity and exact Factory registration.
    /// This low-level helper inherits cancellation/deadline from its enclosing future.
    /// # Errors
    /// Rejects invalid addresses, incomplete responses and any identity mismatch.
    pub async fn deal_identity(&self, deal: &str) -> Result<DealIdentity, RoutingError> {
        if !valid_address(deal) {
            return Err(RoutingError::InvalidAddress);
        }
        let d = &self.deployment;
        let info = self
            .get(&format!("/chain-api/cosmwasm/wasm/v1/contract/{deal}"))
            .await?;
        let config = self.smart(deal, json!({"config":{}})).await?;
        let ci = &info["contract_info"];
        if info["address"] != deal
            || uint(&ci["code_id"])? != d.deal_code_id
            || ci["creator"] != d.factory
            || ci["admin"] != ""
            || config["factory"] != d.factory
            || config["deal_address"] != deal
            || config["settlement_cw20"] != d.token
            || config["fee_recipient"] != d.fee_recipient
            || uint(&config["fee_bps"])? != u64::from(d.fee_bps)
            || config["pinned_gonka_sha"] != d.proto_sha
        {
            return Err(RoutingError::DealMismatch);
        }
        let host = config["host"]
            .as_str()
            .filter(|s| valid_address(s))
            .ok_or(RoutingError::DealMismatch)?
            .to_owned();
        let target_epoch = uint(&config["target_epoch"])?;
        let registration = self
            .smart(
                &d.factory,
                json!({"deal_by_host_epoch":{"host":host,"epoch":target_epoch}}),
            )
            .await?;
        if registration["address"] != deal {
            return Err(RoutingError::DealUnregistered);
        }
        Ok(DealIdentity { host, target_epoch })
    }

    async fn tip(&self) -> Result<u64, RoutingError> {
        let tip = self.get("/chain-rpc/status").await?;
        let result = &tip["result"];
        let network = result["node_info"]["network"]
            .as_str()
            .ok_or(RoutingError::Incomplete)?;
        if network != self.deployment.chain_id {
            return Err(RoutingError::WrongChain);
        }
        let sync = &result["sync_info"];
        if sync["catching_up"] != false {
            return Err(RoutingError::StaleRpc);
        }
        let time = sync["latest_block_time"]
            .as_str()
            .ok_or(RoutingError::StaleRpc)?;
        let time =
            time::OffsetDateTime::parse(time, &time::format_description::well_known::Rfc3339)
                .map_err(|_| RoutingError::StaleRpc)?;
        if (time::OffsetDateTime::now_utc() - time).abs() >= time::Duration::seconds(180) {
            return Err(RoutingError::StaleRpc);
        }
        let height = uint(&sync["latest_block_height"])?;
        if height == 0 {
            return Err(RoutingError::Incomplete);
        }
        Ok(height)
    }
    async fn epoch(&self) -> Result<u64, RoutingError> {
        uint(
            &self
                .get("/chain-api/productscience/inference/inference/get_current_epoch")
                .await?["epoch"],
        )
    }
    async fn recipient(&self, identity: &DealIdentity) -> Result<Option<String>, RoutingError> {
        let body = self
            .get(&format!(
                "/chain-api/productscience/inference/inference/list_claim_recipients/{}",
                identity.host
            ))
            .await?;
        let entries = body["entries"].as_array().ok_or(RoutingError::Incomplete)?;
        let mut seen = BTreeSet::new();
        let mut recipient = None;
        for entry in entries {
            let epoch = uint(&entry["epoch"]).map_err(|_| RoutingError::RoutingInvalid)?;
            let address = entry["recipient"]
                .as_str()
                .filter(|s| valid_address(s))
                .ok_or(RoutingError::RoutingInvalid)?;
            if !seen.insert(epoch) {
                return Err(RoutingError::RoutingInvalid);
            }
            if epoch == identity.target_epoch {
                recipient = Some(address.to_owned());
            }
        }
        Ok(recipient)
    }
    async fn observe(&self, deal: &str) -> Result<DealRoutingResult, RoutingError> {
        let mut previous_end = 0;
        for _ in 0..2 {
            let start = self.tip().await?;
            if start < previous_end {
                return Err(RoutingError::ObservationChanged);
            }
            let current = self.epoch().await?;
            let identity = self.deal_identity(deal).await?;
            let recipient = self.recipient(&identity).await?;
            let end_epoch = self.epoch().await?;
            let end = self.tip().await?;
            if end < start {
                return Err(RoutingError::ObservationChanged);
            }
            if current != end_epoch {
                previous_end = end;
                continue;
            }
            return Ok(DealRoutingResult {
                schema: model::SCHEMA.into(),
                deployment_id: self.deployment.id.clone(),
                chain_id: self.deployment.chain_id.clone(),
                deal: deal.into(),
                host: identity.host,
                target_epoch: identity.target_epoch.to_string(),
                current_epoch: current.to_string(),
                state: classify(deal, recipient.as_deref(), identity.target_epoch, current),
                recipient,
                checked_at: crate::cache::now(),
                source: "chain".into(),
                consistency: "live_state".into(),
                evidence: RoutingEvidence {
                    node: self.node.clone(),
                    start_height: start.to_string(),
                    end_height: end.to_string(),
                    factory: self.deployment.factory.clone(),
                    registration_verified: true,
                },
            });
        }
        Err(RoutingError::ObservationChanged)
    }
}
