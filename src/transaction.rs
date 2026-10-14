use crate::event::ContractEvent;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Transaction {
    pub hash: String,
    pub height: String,
    pub tx_result: Execution,
}

#[derive(Debug, Deserialize)]
pub struct Execution {
    pub code: u32,
    pub events: Vec<ContractEvent>,
}

#[derive(Debug, Serialize)]
pub struct EventRecord<'a> {
    pub transaction_hash: &'a str,
    pub height: &'a str,
    pub event_index: usize,
    pub event: &'a ContractEvent,
}

impl Transaction {
    /// Keeps original event positions and excludes failed executions.
    pub fn events_for<'a>(&'a self, address: &'a str) -> impl Iterator<Item = EventRecord<'a>> {
        self.tx_result
            .events
            .iter()
            .enumerate()
            .filter(move |(_, event)| self.tx_result.code == 0 && event.emitted_by(address))
            .map(|(event_index, event)| EventRecord {
                transaction_hash: &self.hash,
                height: &self.height,
                event_index,
                event,
            })
    }
}
