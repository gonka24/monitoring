use super::{SearchError, SearchRequest, cursor::Cursor, details::Candidate};
use crate::{
    chain::ChainClient,
    factory,
    rpc::{RpcClient, TxPage},
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Duration};
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct NetworkSearch {
    pub(super) chain: ChainClient,
    pub(super) rpc: RpcClient,
    capacity: Arc<Semaphore>,
    pool: Option<crate::pool::NodePool>,
}
impl NetworkSearch {
    #[must_use]
    pub fn new(chain: ChainClient, rpc: RpcClient) -> Self {
        Self {
            chain,
            rpc,
            capacity: Arc::new(Semaphore::new(8)),
            pool: None,
        }
    }

    /// Uses a shared pool for new searches; continuation cursors stay on their source.
    /// # Errors
    /// Returns an error if the pool cannot supply an initial client pair.
    pub fn with_pool(pool: crate::pool::NodePool) -> Result<Self, SearchError> {
        let lease = pool.acquire(None, &[]).ok_or(SearchError::Unavailable)?;
        let mut search = Self::new(lease.chain(), lease.rpc());
        search.pool = Some(pool);
        Ok(search)
    }

    /// Searches indexed network events, then reads only verified candidates' details.
    /// # Errors
    /// Returns request/cursor errors, rate limiting, node inconsistency or timeout.
    pub async fn search(&self, request: &SearchRequest) -> Result<Value, SearchError> {
        if request.filters.hosts.is_empty() && request.filters.buyers.is_empty() {
            return Err(SearchError::filter("filters"));
        }
        let cursor = Cursor::read(request, u64::MAX)?;
        let _permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| SearchError::RateLimited)?;
        tokio::time::timeout(Duration::from_secs(30), self.dispatch(request, cursor))
            .await
            .map_err(|_| {
                crate::diagnostics::failure("search.collect", "deadline_exceeded");
                SearchError::Unavailable
            })?
    }

    async fn dispatch(
        &self,
        request: &SearchRequest,
        cursor: Cursor,
    ) -> Result<Value, SearchError> {
        let Some(pool) = &self.pool else {
            if cursor.node.is_some() {
                return Err(SearchError::InvalidCursor);
            }
            return self.collect(request, cursor).await;
        };
        let pinned = cursor.node;
        // Older cursors have no source identity: require a fresh search rather than
        // silently changing the indexed history underneath a continuation.
        if request.cursor.is_some() && pinned.as_ref().is_none_or(|id| !pool.contains(id)) {
            return Err(SearchError::InvalidCursor);
        }
        let mut tried = Vec::new();
        for _ in 0..3 {
            let Some(lease) = pool.acquire(pinned.as_deref(), &tried) else {
                break;
            };
            tried.push(lease.index);
            let mut cursor = Cursor::read(request, u64::MAX)?;
            cursor.node = Some(lease.id().to_owned());
            let worker = Self::new(lease.chain(), lease.rpc());
            let result = tokio::time::timeout(Duration::from_secs(12), async {
                worker.rpc.check_pool_node().await?;
                worker.collect(request, cursor).await
            })
            .await
            .unwrap_or(Err(SearchError::Unavailable));
            lease.finish(!matches!(result, Err(SearchError::Unavailable)));
            if !matches!(result, Err(SearchError::Unavailable)) {
                return result;
            }
        }
        Err(SearchError::Unavailable)
    }

    async fn collect(
        &self,
        request: &SearchRequest,
        mut cursor: Cursor,
    ) -> Result<Value, SearchError> {
        let updated_at = crate::cache::now();
        let height = self.rpc.search_height().await?;
        if request.cursor.is_none() {
            cursor.height = height;
        }
        if cursor.height > height {
            return Err(SearchError::InvalidCursor);
        }
        let mut budget = 256_usize;
        let mut examined = 0;
        let mut seen = BTreeSet::new();
        let mut deals = Vec::new();
        let mut loaded: Option<((usize, u32), TxPage)> = None;
        while cursor.wallet < cursor.wallets().len()
            && deals.len() < request.limit
            && examined < 1000
            && budget > 4
        {
            let position = (cursor.wallet, cursor.page);
            if loaded.as_ref().is_none_or(|(key, _)| *key != position) {
                let page = self
                    .rpc
                    .search_events(&event_query(&cursor), cursor.page, cursor.height)
                    .await?;
                budget -= 1;
                validate_page(&page, &mut cursor)?;
                loaded = Some((position, page));
            }
            let page = &loaded.as_ref().ok_or(SearchError::Unavailable)?.1;
            if page.txs.is_empty() {
                cursor.next_wallet();
                continue;
            }
            let tx = page
                .txs
                .get(cursor.transaction)
                .ok_or(SearchError::InvalidCursor)?;
            if cursor.event > tx.tx_result.events.len() {
                return Err(SearchError::InvalidCursor);
            }
            if tx.tx_result.code != 0 || cursor.event == tx.tx_result.events.len() {
                cursor.next_transaction(page.txs.len())?;
                continue;
            }
            let event = &tx.tx_result.events[cursor.event];
            if let Some(candidate) = Candidate::from_event(event, &cursor)
                && seen.insert(candidate.address.clone())
            {
                budget -= 4;
                if let Some(deal) = self.details(&candidate, &cursor, updated_at).await? {
                    deals.push(deal);
                }
            }
            examined += 1;
            cursor.event += 1;
            if cursor.event == tx.tx_result.events.len() {
                cursor.next_transaction(page.txs.len())?;
            }
        }
        let next_cursor = cursor.encode()?;
        Ok(
            json!({"deals":deals,"next_cursor":next_cursor,"search_height":cursor.height.to_string(),
            "updated_at":updated_at,"stale":false,"source":"chain","history_completeness":"unknown","consistency":"live_state",
            "partial_page":deals.len()<request.limit && next_cursor.is_some()}),
        )
    }
}

fn event_query(cursor: &Cursor) -> String {
    let wallet = &cursor.wallets()[cursor.wallet];
    if cursor.filters.buyers.is_empty() {
        format!(
            "wasm-offer_created.host='{wallet}' AND wasm-offer_created._contract_address='{}'",
            factory::ADDRESS
        )
    } else {
        format!("wasm-deal_funded.buyer='{wallet}'")
    }
}

fn validate_page(page: &TxPage, cursor: &mut Cursor) -> Result<(), SearchError> {
    let total = page
        .total_count
        .parse::<u64>()
        .map_err(|_| SearchError::Unavailable)?;
    let start = u64::from(cursor.page - 1) * 20;
    let expected = total.saturating_sub(start).min(20);
    if start >= total && (total != 0 || cursor.page != 1) {
        return Err(SearchError::InvalidCursor);
    }
    if cursor.total.is_some_and(|previous| previous != total)
        || u64::try_from(page.txs.len()).ok() != Some(expected)
        || (page.txs.is_empty() && (cursor.transaction != 0 || cursor.event != 0))
    {
        return Err(SearchError::Unavailable);
    }
    let mut previous = cursor.height;
    let mut hashes = BTreeSet::new();
    for tx in &page.txs {
        let height = tx
            .height
            .parse::<u64>()
            .map_err(|_| SearchError::Unavailable)?;
        if height == 0 || height > previous || !hashes.insert(&tx.hash) {
            return Err(SearchError::Unavailable);
        }
        previous = height;
    }
    cursor.total = Some(total);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn overload_returns_retry_after_without_calling_node()
    -> Result<(), Box<dyn std::error::Error>> {
        let search = NetworkSearch::new(
            ChainClient::new("http://127.0.0.1:1")?,
            RpcClient::new("http://127.0.0.1:1")?,
        );
        let permit = search.capacity.clone().acquire_many_owned(8).await?;
        let app = crate::create_app(crate::cache::Cache::with_search(search));
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v2/deals/search")
                    .body(Body::from(json!({"hosts":[factory::ADDRESS]}).to_string()))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()["Retry-After"], "1");
        drop(permit);
        Ok(())
    }
}
