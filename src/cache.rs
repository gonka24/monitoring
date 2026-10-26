use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Snapshot {
    pub deals: BTreeMap<u64, Value>,
    pub histories: BTreeMap<String, Vec<Value>>,
    pub updated_at: u64,
}

#[derive(Default)]
struct Stored {
    snapshot: Option<Arc<Snapshot>>,
    failed: bool,
}

#[derive(Clone, Default)]
pub struct Cache {
    inner: Arc<RwLock<Stored>>,
    search: Option<crate::search::NetworkSearch>,
    routing: Option<crate::routing::RoutingClient>,
}

#[must_use]
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Cache {
    #[must_use]
    pub fn with_routing(mut self, routing: crate::routing::RoutingClient) -> Self {
        self.routing = Some(routing);
        self
    }

    #[must_use]
    pub fn routing(&self) -> Option<crate::routing::RoutingClient> {
        self.routing.clone()
    }

    #[must_use]
    pub fn with_search(search: crate::search::NetworkSearch) -> Self {
        Self {
            search: Some(search),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn network_search(&self) -> Option<crate::search::NetworkSearch> {
        self.search.clone()
    }

    /// Returns the last complete snapshot and whether it needs refreshing.
    #[must_use]
    pub fn read(&self) -> Option<(Arc<Snapshot>, bool)> {
        let stored = self.inner.read().ok()?;
        let snapshot = stored.snapshot.clone()?;
        let stale = stored.failed || now().saturating_sub(snapshot.updated_at) >= 120;
        Some((snapshot, stale))
    }

    pub fn publish(&self, snapshot: Snapshot) {
        if let Ok(mut stored) = self.inner.write() {
            *stored = Stored {
                snapshot: Some(Arc::new(snapshot)),
                failed: false,
            };
        }
    }

    pub fn mark_failed(&self) {
        if let Ok(mut stored) = self.inner.write() {
            stored.failed = true;
        }
    }
}
