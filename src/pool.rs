//! Bounded, server-configured pool. A lease pins REST and RPC for an entire operation.
use crate::{
    chain::{ChainClient, ChainError},
    rpc::RpcClient,
};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};

#[derive(Clone)]
pub struct NodePool(Arc<Inner>);
struct Inner {
    nodes: Vec<Node>,
    health: Mutex<Vec<Health>>,
}
struct Node {
    id: String,
    chain: ChainClient,
    rpc: RpcClient,
}
#[derive(Default)]
struct Health {
    active: u32,
    latency_ms: u64,
    retry_at: Option<Instant>,
}

pub(crate) struct Lease {
    pool: NodePool,
    pub index: usize,
    started: Instant,
}
impl NodePool {
    /// Creates a pool of 1–16 trusted HTTP(S) base URLs, without credentials or queries.
    /// Node discovery is deliberately outside the request path.
    /// # Errors
    /// Rejects malformed/duplicate URLs and HTTP client initialization failures.
    pub fn new(bases: &[String]) -> Result<Self, Box<dyn std::error::Error>> {
        if bases.is_empty() || bases.len() > 16 {
            return Err("GONKA_NODE_URLS must contain 1 to 16 nodes".into());
        }
        let mut nodes = Vec::new();
        for base in bases {
            let url = reqwest::Url::parse(base).map_err(|_| "Invalid node URL")?;
            if !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
                || url.path() != "/"
            {
                return Err("Node URLs must be HTTP(S) origins without credentials".into());
            }
            let id = url.as_str().trim_end_matches('/').to_owned();
            if nodes.iter().any(|n: &Node| n.id == id) {
                return Err("Duplicate node URL".into());
            }
            nodes.push(Node {
                chain: ChainClient::new(&format!("{id}/chain-api"))?,
                rpc: RpcClient::new(&format!("{id}/chain-rpc"))?,
                id,
            });
        }
        let health = Mutex::new((0..nodes.len()).map(|_| Health::default()).collect());
        Ok(Self(Arc::new(Inner { nodes, health })))
    }

    pub(crate) fn acquire(&self, pinned: Option<&str>, tried: &[usize]) -> Option<Lease> {
        let mut health = self.0.health.lock().ok()?;
        let now = Instant::now();
        let index = health
            .iter()
            .enumerate()
            .filter(|(i, h)| {
                !tried.contains(i)
                    && pinned.is_none_or(|id| self.0.nodes[*i].id == id)
                    && h.retry_at.is_none_or(|at| at <= now && h.active == 0)
            })
            .min_by_key(|(_, h)| (h.active, h.latency_ms))?
            .0;
        health[index].active += 1;
        Some(Lease {
            pool: self.clone(),
            index,
            started: now,
        })
    }

    pub(crate) fn contains(&self, id: &str) -> bool {
        self.0.nodes.iter().any(|n| n.id == id)
    }

    /// Collects one complete snapshot, restarting on another node after a failure.
    /// # Errors
    /// Returns an upstream error if no node succeeds within the bounded attempts.
    pub async fn collect(&self) -> Result<crate::cache::Snapshot, ChainError> {
        let mut tried = Vec::new();
        for _ in 0..3 {
            let Some(lease) = self.acquire(None, &tried) else {
                break;
            };
            tried.push(lease.index);
            let result = tokio::time::timeout(Duration::from_secs(20), async {
                lease.rpc().check_pool_node().await?;
                crate::collector::collect(&lease.chain(), &lease.rpc()).await
            })
            .await
            .unwrap_or(Err(ChainError::Upstream));
            lease.finish(result.is_ok());
            if result.is_ok() {
                return result;
            }
        }
        Err(ChainError::Upstream)
    }
}
impl Lease {
    pub fn id(&self) -> &str {
        &self.pool.0.nodes[self.index].id
    }
    pub fn chain(&self) -> ChainClient {
        self.pool.0.nodes[self.index].chain.clone()
    }
    pub fn rpc(&self) -> RpcClient {
        self.pool.0.nodes[self.index].rpc.clone()
    }
    pub fn finish(&self, success: bool) {
        if let Ok(mut health) = self.pool.0.health.lock() {
            let h = &mut health[self.index];
            if success {
                let elapsed = u64::try_from(self.started.elapsed().as_millis())
                    .unwrap_or(u64::MAX)
                    .max(1);
                h.latency_ms = if h.latency_ms == 0 {
                    elapsed
                } else {
                    h.latency_ms.saturating_mul(3).saturating_add(elapsed) / 4
                };
                // A success from an older in-flight operation must not undo a
                // newer failure; only the single recovery probe closes the circuit.
                if h.retry_at.is_none_or(|at| at <= self.started) {
                    h.retry_at = None;
                }
            } else {
                h.retry_at = Some(Instant::now() + Duration::from_secs(30));
                crate::diagnostics::pool_failure(self.index);
            }
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if let Ok(mut health) = self.pool.0.health.lock() {
            health[self.index].active = health[self.index].active.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    type TestResult = Result<(), Box<dyn std::error::Error>>;
    fn pool() -> Result<NodePool, Box<dyn std::error::Error>> {
        NodePool::new(&["http://127.0.0.1:1".into(), "http://127.0.0.1:2".into()])
    }
    #[tokio::test(start_paused = true)]
    async fn load_latency_cooldown_recovery_and_cancellation() -> TestResult {
        let pool = pool()?;
        let first = pool.acquire(None, &[]).ok_or("no first node")?;
        assert_eq!(first.index, 0);
        let second = pool.acquire(None, &[]).ok_or("no second node")?;
        assert_eq!(second.index, 1, "prefer fewer active operations");
        second.finish(false);
        drop(second);
        tokio::time::advance(Duration::from_millis(100)).await;
        first.finish(true);
        drop(first);
        let selected = pool.acquire(None, &[]).ok_or("no healthy node")?;
        assert_eq!(selected.index, 0);
        drop(selected);
        assert!(pool.acquire(None, &[0]).is_none());
        tokio::time::advance(Duration::from_secs(30)).await;
        let recovered = pool.acquire(None, &[]).ok_or("no recovery probe")?;
        assert_eq!(recovered.index, 1);
        tokio::time::advance(Duration::from_millis(200)).await;
        recovered.finish(true);
        drop(recovered);
        let faster = pool.acquire(None, &[]).ok_or("no faster node")?;
        assert_eq!(faster.index, 0);
        drop(faster); // Cancellation releases load without changing health.
        let health = pool.0.health.lock().map_err(|e| e.to_string())?;
        assert!(health.iter().all(|h| h.active == 0));
        Ok(())
    }
    #[tokio::test(start_paused = true)]
    async fn only_one_recovery_probe_and_old_success_cannot_close_circuit() -> TestResult {
        let pool = pool()?;
        let old = pool.acquire(Some("http://127.0.0.1:1"), &[]).ok_or("old")?;
        let failed = pool
            .acquire(Some("http://127.0.0.1:1"), &[])
            .ok_or("failed")?;
        failed.finish(false);
        drop(failed);
        old.finish(true);
        drop(old);
        assert!(pool.acquire(Some("http://127.0.0.1:1"), &[]).is_none());
        tokio::time::advance(Duration::from_secs(30)).await;
        let probe = pool
            .acquire(Some("http://127.0.0.1:1"), &[])
            .ok_or("probe")?;
        assert!(pool.acquire(Some("http://127.0.0.1:1"), &[]).is_none());
        probe.finish(true);
        drop(probe);
        assert!(pool.acquire(Some("http://127.0.0.1:1"), &[]).is_some());
        Ok(())
    }

    #[test]
    fn validates_pool_configuration() {
        for urls in [
            vec![],
            vec!["https://example.com?secret=x".into()],
            vec!["https://user:secret@example.com".into()],
            vec!["file:///tmp/node".into()],
            vec!["https://example.com/chain-rpc".into()],
            vec!["https://example.com".into(), "https://example.com/".into()],
            vec!["https://example.com".into(); 17],
        ] {
            assert!(NodePool::new(&urls).is_err());
        }
    }
}
