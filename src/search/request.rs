use bech32::{Bech32, primitives::decode::CheckedHrpstring};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::SearchError;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Filters {
    pub hosts: Vec<String>,
    pub buyers: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub filters: Filters,
    pub limit: usize,
    pub cursor: Option<String>,
}

/// Gonka account (20 bytes) and contract (32 bytes) addresses, canonical lowercase.
#[must_use]
pub fn valid_address(address: &str) -> bool {
    let Ok(decoded) = CheckedHrpstring::new::<Bech32>(address) else {
        return false;
    };
    let bytes: Vec<_> = decoded.byte_iter().collect();
    decoded.hrp().as_str() == "gonka"
        && matches!(bytes.len(), 20 | 32)
        && bech32::encode::<Bech32>(decoded.hrp(), &bytes).is_ok_and(|s| s == address)
}

impl SearchRequest {
    /// Parses the entire request; invalid or unknown fields never broaden a search.
    /// # Errors
    /// Returns a field-specific error for any invalid input.
    pub fn parse(value: &Value) -> Result<Self, SearchError> {
        let object = value
            .as_object()
            .ok_or_else(|| SearchError::filter("body"))?;
        for key in object.keys() {
            if !matches!(key.as_str(), "hosts" | "buyers" | "limit" | "cursor") {
                return Err(SearchError::filter(key));
            }
        }
        let limit = match object.get("limit") {
            None => 50,
            Some(v) => v
                .as_u64()
                .filter(|v| (1..=100).contains(v))
                .and_then(|v| usize::try_from(v).ok())
                .ok_or(SearchError::InvalidLimit)?,
        };
        let cursor = match object.get("cursor") {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) if !s.is_empty() && s.len() <= 20000 => Some(s.clone()),
            _ => return Err(SearchError::InvalidCursor),
        };
        Ok(Self {
            filters: Filters {
                hosts: addresses(value, "hosts")?,
                buyers: addresses(value, "buyers")?,
            },
            limit,
            cursor,
        })
    }
}

fn addresses(value: &Value, field: &str) -> Result<Vec<String>, SearchError> {
    let Some(value) = value.get(field) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|a| a.len() <= 100)
        .ok_or_else(|| SearchError::filter(field))?;
    let mut addresses = values
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| valid_address(s))
                .map(str::to_owned)
                .ok_or_else(|| SearchError::filter(field))
        })
        .collect::<Result<Vec<_>, _>>()?;
    addresses.sort_unstable();
    addresses.dedup();
    Ok(addresses)
}
