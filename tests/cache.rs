use monitoring::cache::{Cache, Snapshot, now};
use std::collections::BTreeMap;

#[test]
fn preserves_snapshot_after_failed_refresh() {
    let cache = Cache::default();
    assert!(cache.read().is_none());
    cache.publish(Snapshot {
        histories: BTreeMap::new(),
        deals: BTreeMap::new(),
        updated_at: now(),
    });
    let Some((old, stale)) = cache.read() else {
        panic!("snapshot missing")
    };
    assert!(!stale);
    cache.mark_failed();
    let Some((current, stale)) = cache.read() else {
        panic!("snapshot lost")
    };
    assert!(stale);
    assert!(std::sync::Arc::ptr_eq(&old, &current));
    cache.publish(Snapshot {
        histories: BTreeMap::new(),
        deals: BTreeMap::new(),
        updated_at: 0,
    });
    assert!(cache.read().is_some_and(|(_, stale)| stale));
    cache.publish(Snapshot {
        histories: BTreeMap::new(),
        deals: BTreeMap::new(),
        updated_at: now(),
    });
    assert!(cache.read().is_some_and(|(_, stale)| !stale));
}
