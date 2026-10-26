use super::RoutingError;
use crate::search::valid_address;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA: &str = "gonka.forward.deal-routing.v1";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DealRoutingState {
    Configured,
    OtherRecipient,
    NotConfigured,
    HistoryUnknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoutingEvidence {
    pub node: String,
    pub start_height: String,
    pub end_height: String,
    pub factory: String,
    pub registration_verified: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DealRoutingResult {
    pub schema: String,
    pub deployment_id: String,
    pub chain_id: String,
    pub deal: String,
    pub host: String,
    pub target_epoch: String,
    pub current_epoch: String,
    pub recipient: Option<String>,
    pub state: DealRoutingState,
    pub checked_at: u64,
    pub source: String,
    pub consistency: String,
    pub evidence: RoutingEvidence,
}

pub(super) fn decimal(s: &str) -> Result<u64, RoutingError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || (s.len() > 1 && s.starts_with('0'))
    {
        return Err(RoutingError::Incomplete);
    }
    s.parse().map_err(|_| RoutingError::Incomplete)
}

pub(super) fn uint(value: &Value) -> Result<u64, RoutingError> {
    match value {
        Value::String(s) => decimal(s),
        Value::Number(n) => n.as_u64().ok_or(RoutingError::Incomplete),
        _ => Err(RoutingError::Incomplete),
    }
}

pub(super) fn classify(
    deal: &str,
    recipient: Option<&str>,
    target: u64,
    current: u64,
) -> DealRoutingState {
    match recipient {
        Some(r) if r == deal => DealRoutingState::Configured,
        Some(_) => DealRoutingState::OtherRecipient,
        None if current < target => DealRoutingState::NotConfigured,
        None => DealRoutingState::HistoryUnknown,
    }
}

pub(super) fn node_url(node: &str) -> bool {
    reqwest::Url::parse(node).is_ok_and(|u| {
        matches!(u.scheme(), "https" | "http")
            && u.host_str().is_some()
            && u.username().is_empty()
            && u.password().is_none()
            && u.query().is_none()
            && u.fragment().is_none()
            && u.path() == "/"
            && !node.ends_with('/')
    })
}

/// Pure validation of the JSON wire contract, including classification invariants.
/// # Errors
/// Returns `INCOMPLETE` for malformed results or inconsistent evidence.
pub fn parse_deal_routing_result(value: Value) -> Result<DealRoutingResult, RoutingError> {
    // Option<T> alone accepts a missing field; null must be explicit on the wire.
    if value.get("recipient").is_none() {
        return Err(RoutingError::Incomplete);
    }
    let r: DealRoutingResult =
        serde_json::from_value(value).map_err(|_| RoutingError::Incomplete)?;
    let target = decimal(&r.target_epoch)?;
    let current = decimal(&r.current_epoch)?;
    let start = decimal(&r.evidence.start_height)?;
    let end = decimal(&r.evidence.end_height)?;
    if r.schema != SCHEMA
        || r.source != "chain"
        || r.consistency != "live_state"
        || r.deployment_id.is_empty()
        || r.chain_id.is_empty()
        || r.checked_at == 0
        || !valid_address(&r.deal)
        || !valid_address(&r.host)
        || !valid_address(&r.evidence.factory)
        || !r.evidence.registration_verified
        || !node_url(&r.evidence.node)
        || start == 0
        || end < start
        || r.recipient.as_ref().is_some_and(|s| !valid_address(s))
        || r.state != classify(&r.deal, r.recipient.as_deref(), target, current)
    {
        return Err(RoutingError::Incomplete);
    }
    Ok(r)
}
