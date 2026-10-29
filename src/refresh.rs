use crate::{cache::Cache, chain::ChainClient, collector::collect, rpc::RpcClient};
use std::{future::Future, time::Duration};
use tokio::time::{MissedTickBehavior, interval, timeout};

pub async fn run(client: ChainClient, rpc: RpcClient, cache: Cache) {
    refresh(cache, || collect(&client, &rpc)).await;
}

async fn refresh<F, T>(cache: Cache, mut collect: F)
where
    F: FnMut() -> T,
    T: Future<Output = Result<crate::cache::Snapshot, crate::chain::ChainError>>,
{
    let mut timer = interval(Duration::from_secs(60));
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        match timeout(Duration::from_secs(50), collect()).await {
            Ok(Ok(snapshot)) => cache.publish(snapshot),
            Ok(Err(_)) => {
                cache.mark_failed();
                crate::diagnostics::failure("snapshot.refresh", "collection_failed");
            }
            Err(_) => {
                cache.mark_failed();
                crate::diagnostics::failure("snapshot.refresh", "deadline_exceeded");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{Snapshot, now};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[tokio::test(start_paused = true)]
    async fn refreshes_on_schedule_and_recovers() {
        let cache = Cache::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let worker = tokio::spawn(refresh(cache.clone(), move || {
            let attempt = counter.fetch_add(1, Ordering::SeqCst);
            async move {
                if attempt == 1 {
                    std::future::pending::<()>().await;
                }
                if attempt == 2 {
                    return Err(crate::chain::ChainError::Upstream);
                }
                Ok(Snapshot {
                    histories: [("deal".into(), vec![serde_json::json!(attempt)])].into(),
                    deals: std::collections::BTreeMap::new(),
                    updated_at: now(),
                })
            }
        }));
        tokio::task::yield_now().await;
        assert!(cache.read().is_some_and(|(_, stale)| !stale));
        for (seconds, count, stale) in [
            (59, 1, false),
            (1, 2, false),
            (49, 2, false),
            (1, 2, true),
            (10, 3, true),
            (60, 4, false),
        ] {
            tokio::time::advance(Duration::from_secs(seconds)).await;
            tokio::task::yield_now().await;
            assert_eq!(calls.load(Ordering::SeqCst), count);
            assert_eq!(cache.read().map(|(_, failed)| failed), Some(stale));
            let expected = if count == 4 { 3 } else { 0 };
            assert_eq!(
                cache.read().map(|(s, _)| s.histories["deal"][0].clone()),
                Some(serde_json::json!(expected))
            );
        }
        worker.abort();
    }
}
