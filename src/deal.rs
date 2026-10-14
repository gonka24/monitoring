use serde::{Deserialize, Serialize};

/// Initial projection of a Deal's state query; financial fields are not modeled yet.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DealState {
    pub status: DealStatus,
    pub buyer: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DealStatus {
    Open,
    Funded,
    Locked,
    Releasing,
    Completed,
    Refunded,
    Cancelled,
    Expired,
}
