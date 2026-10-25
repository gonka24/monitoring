use super::{NetworkSearch, SearchError, cursor::Cursor, valid_address};
use crate::{event::ContractEvent, factory};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) struct Candidate {
    pub address: String,
    pub host: String,
    pub id: Option<u64>,
}

impl Candidate {
    pub fn from_event(event: &ContractEvent, cursor: &Cursor) -> Option<Self> {
        let buyer_search = !cursor.filters.buyers.is_empty();
        let expected = if buyer_search {
            "wasm-deal_funded"
        } else {
            "wasm-offer_created"
        };
        if event.kind != expected {
            return None;
        }
        let address = event.unique("deal")?;
        let host = event.unique("host")?;
        if address.len() != 64 || !valid_address(address) || !valid_address(host) {
            return None;
        }
        let wallet = &cursor.wallets()[cursor.wallet];
        if buyer_search {
            if !event.emitted_by(address) || event.unique("buyer") != Some(wallet) {
                return None;
            }
        } else if !event.emitted_by(factory::ADDRESS) || host != wallet {
            return None;
        }
        if !cursor.filters.hosts.is_empty() && !cursor.filters.hosts.iter().any(|h| h == host) {
            return None;
        }
        let id = if buyer_search {
            None
        } else {
            Some(event.unique("deal_id")?.parse().ok()?)
        };
        Some(Self {
            address: address.into(),
            host: host.into(),
            id,
        })
    }
}

#[derive(Deserialize)]
struct RegisteredDeal {
    address: String,
}

impl NetworkSearch {
    pub(super) async fn details(
        &self,
        candidate: &Candidate,
        cursor: &Cursor,
        updated_at: u64,
    ) -> Result<Option<Value>, SearchError> {
        let id = if let Some(id) = candidate.id {
            id
        } else {
            let query = format!(
                "wasm-offer_created.deal='{}' AND wasm-offer_created._contract_address='{}'",
                candidate.address,
                factory::ADDRESS
            );
            let page = self.rpc.search_events(&query, 1, cursor.height).await?;
            if page.total_count == "0" && page.txs.is_empty() {
                return Ok(None);
            }
            if page.total_count != "1" || page.txs.len() != 1 {
                return Err(SearchError::Unavailable);
            }
            let transaction = &page.txs[0];
            if transaction.tx_result.code != 0
                || !transaction
                    .height
                    .parse::<u64>()
                    .is_ok_and(|h| h > 0 && h <= cursor.height)
            {
                return Err(SearchError::Unavailable);
            }
            let mut events = transaction.tx_result.events.iter().filter(|e| {
                e.kind == "wasm-offer_created"
                    && e.emitted_by(factory::ADDRESS)
                    && e.unique("deal") == Some(candidate.address.as_str())
            });
            let event = events.next().ok_or(SearchError::Unavailable)?;
            if events.next().is_some() || event.unique("host") != Some(candidate.host.as_str()) {
                return Err(SearchError::Unavailable);
            }
            event
                .unique("deal_id")
                .and_then(|s| s.parse().ok())
                .ok_or(SearchError::Unavailable)?
        };
        let registered: RegisteredDeal = self
            .chain
            .query(factory::ADDRESS, json!({"deal":{"id":id}}))
            .await?;
        if registered.address != candidate.address {
            return Ok(None);
        }
        let config = self.chain.config(&candidate.address).await?;
        if config.factory != factory::ADDRESS
            || config.deal_address != candidate.address
            || config.host != candidate.host
        {
            return Err(SearchError::Unavailable);
        }
        let state = self.chain.state(&candidate.address).await?;
        // Use the contract's current roles, never transaction sender/fee payer.
        if !cursor.filters.buyers.is_empty()
            && !state
                .buyer
                .as_ref()
                .is_some_and(|buyer| cursor.filters.buyers.contains(buyer))
        {
            return Ok(None);
        }
        Ok(Some(
            json!({"id":id.to_string(),"address":candidate.address,"host":config.host,"buyer":state.buyer,"status":state.status,
            "target_epoch":config.target_epoch.to_string(),"factory":config.factory,"settlement_cw20":config.settlement_cw20,
            "buyer_budget_micro_usdt":config.buyer_budget_micro_usdt,"price_micro_usdt_per_gnk":config.price_micro_usdt_per_gnk,
            "updated_at":updated_at,"stale":false}),
        ))
    }
}
