//! Internal utilities shared across modules.

use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic counter with relaxed ordering.
///
/// Wraps `AtomicU64` to eliminate repeated `Ordering::Relaxed` boilerplate
/// across modules that track stats (relay, ratelimit, pubsub, queue).
#[derive(Debug, Default)]
pub struct Counter(AtomicU64);

impl Counter {
    /// Create a counter starting at zero.
    pub const fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Increment by one.
    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment by `n`.
    pub fn add(&self, n: u64) {
        self.0.fetch_add(n, Ordering::Relaxed);
    }

    /// Read the current value.
    pub fn get(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Evict entries from a `DashMap` where the predicate returns `true`.
///
/// Returns the number of entries evicted. This is a two-pass approach
/// (collect keys, then remove) to avoid holding shard locks during removal.
pub fn evict_from_dashmap<K, V, F>(map: &dashmap::DashMap<K, V>, should_evict: F) -> usize
where
    K: Eq + std::hash::Hash + Clone,
    F: Fn(&K, &V) -> bool,
{
    let keys: Vec<K> = map
        .iter()
        .filter(|entry| should_evict(entry.key(), entry.value()))
        .map(|entry| entry.key().clone())
        .collect();

    let count = keys.len();
    for key in &keys {
        map.remove(key);
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_basics() {
        let c = Counter::new();
        assert_eq!(c.get(), 0);
        c.inc();
        assert_eq!(c.get(), 1);
        c.add(5);
        assert_eq!(c.get(), 6);
    }

    #[test]
    fn evict_from_dashmap_works() {
        let map = dashmap::DashMap::new();
        map.insert("a", 1);
        map.insert("b", 2);
        map.insert("c", 3);

        let evicted = evict_from_dashmap(&map, |_k, v| *v > 1);
        assert_eq!(evicted, 2);
        assert_eq!(map.len(), 1);
        assert!(map.contains_key("a"));
    }

    #[test]
    fn evict_none() {
        let map: dashmap::DashMap<String, i32> = dashmap::DashMap::new();
        map.insert("a".into(), 1);
        let evicted = evict_from_dashmap(&map, |_k, _v| false);
        assert_eq!(evicted, 0);
        assert_eq!(map.len(), 1);
    }
}
