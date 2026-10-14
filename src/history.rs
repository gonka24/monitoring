use crate::{chain::ChainError, rpc::RpcClient};
use serde_json::Value;
use std::{collections::BTreeSet, num::NonZeroU32, time::Duration};

/// Collects the node's indexed history, not a guaranteed complete or atomic chain history.
/// # Errors
/// Rejects inconsistent pages, repeated transactions, upstream failures and exceeded limits.
pub async fn collect(client: &RpcClient, address: &str) -> Result<Vec<Value>, ChainError> {
    tokio::time::timeout(Duration::from_secs(50), async {
        let mut events = Vec::new();
        let mut hashes = BTreeSet::new();
        let mut expected = None;
        let mut page_number = NonZeroU32::MIN;
        loop {
            let page = client.page(address, page_number).await?;
            let total: usize = page.total_count.parse().map_err(|_| ChainError::Upstream)?;
            if total > 1000
                || expected.is_some_and(|n| n != total)
                || page.txs.len() != (total.saturating_sub(hashes.len())).min(20)
            {
                return Err(ChainError::Upstream);
            }
            expected = Some(total);
            for tx in page.txs {
                if !hashes.insert(tx.hash.clone()) {
                    return Err(ChainError::Upstream);
                }
                for event in tx.events_for(address) {
                    if events.len() >= 10_000 {
                        return Err(ChainError::Upstream);
                    }
                    events.push(serde_json::to_value(event).map_err(|_| ChainError::Upstream)?);
                }
            }
            if hashes.len() == total {
                return Ok(events);
            }
            page_number = page_number.saturating_add(1);
        }
    })
    .await
    .map_err(|_| ChainError::Upstream)?
}
