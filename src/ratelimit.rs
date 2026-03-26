//! Per-key token bucket rate limiter.
//!
//! Lazy refill — tokens are replenished on each `check()` call based on
//! elapsed time. No background tasks required.
//!
//! Supports stale-key eviction and usage statistics.

use std::time::{Duration, Instant};

use dashmap::DashMap;
use tracing::debug;

use crate::util::{Counter, evict_from_dashmap};

/// State for a single key's bucket.
struct Bucket {
    tokens: f64,
    /// Last time this bucket was accessed (used for both refill and staleness eviction).
    last_access: Instant,
}

/// Rate limiter usage statistics.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct RateLimitStats {
    /// Total number of allowed requests.
    pub total_allowed: u64,
    /// Total number of rejected requests.
    pub total_rejected: u64,
    /// Currently tracked keys.
    pub active_keys: usize,
    /// Total keys evicted since creation.
    pub total_evicted: u64,
}

/// Token bucket rate limiter with per-key tracking.
///
/// Thread-safe — backed by [`DashMap`] for concurrent per-key access.
/// Supports stale-key eviction to prevent unbounded memory growth.
#[must_use]
pub struct RateLimiter {
    /// Tokens replenished per second.
    rate: f64,
    /// Maximum token capacity (burst size).
    burst: usize,
    buckets: DashMap<String, Bucket>,
    total_allowed: Counter,
    total_rejected: Counter,
    total_evicted: Counter,
}

impl RateLimiter {
    /// Create a limiter: `rate` tokens/sec, `burst` max tokens.
    pub fn new(rate: f64, burst: usize) -> Self {
        Self {
            rate,
            burst,
            buckets: DashMap::new(),
            total_allowed: Counter::new(),
            total_rejected: Counter::new(),
            total_evicted: Counter::new(),
        }
    }

    /// Check if a request for `key` is allowed. Consumes one token if yes.
    #[must_use]
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let burst = self.burst as f64;

        let mut entry = self.buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: burst,
            last_access: now,
        });

        let bucket = entry.value_mut();

        let elapsed = now.duration_since(bucket.last_access).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.rate).min(burst);
        bucket.last_access = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            self.total_allowed.inc();
            true
        } else {
            self.total_rejected.inc();
            false
        }
    }

    /// Number of tracked keys.
    #[inline]
    #[must_use]
    pub fn key_count(&self) -> usize {
        self.buckets.len()
    }

    /// Evict keys that have not been checked for at least `max_idle`.
    ///
    /// Returns the number of keys evicted.
    #[must_use]
    pub fn evict_stale(&self, max_idle: Duration) -> usize {
        let now = Instant::now();
        let count = evict_from_dashmap(&self.buckets, |_key, bucket| {
            now.duration_since(bucket.last_access) >= max_idle
        });
        self.total_evicted.add(count as u64);
        if count > 0 {
            debug!(count, "ratelimit: evicted stale keys");
        }
        count
    }

    /// Current usage statistics.
    pub fn stats(&self) -> RateLimitStats {
        RateLimitStats {
            total_allowed: self.total_allowed.get(),
            total_rejected: self.total_rejected.get(),
            active_keys: self.buckets.len(),
            total_evicted: self.total_evicted.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_burst() {
        let limiter = RateLimiter::new(1.0, 3);
        assert!(limiter.check("a"));
        assert!(limiter.check("a"));
        assert!(limiter.check("a"));
        assert!(!limiter.check("a"));
    }

    #[test]
    fn separate_keys_independent() {
        let limiter = RateLimiter::new(1.0, 1);
        assert!(limiter.check("a"));
        assert!(limiter.check("b"));
        assert!(!limiter.check("a"));
        assert!(!limiter.check("b"));
    }

    #[test]
    fn refills_over_time() {
        let limiter = RateLimiter::new(100.0, 1);
        assert!(limiter.check("a"));
        assert!(!limiter.check("a"));

        std::thread::sleep(Duration::from_millis(20));
        assert!(limiter.check("a"));
    }

    #[test]
    fn key_count() {
        let limiter = RateLimiter::new(1.0, 1);
        let _ = limiter.check("a");
        let _ = limiter.check("b");
        assert_eq!(limiter.key_count(), 2);
    }

    #[test]
    fn concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let limiter = Arc::new(RateLimiter::new(1000.0, 10));
        let mut handles = Vec::new();

        for i in 0..4 {
            let l = limiter.clone();
            handles.push(thread::spawn(move || {
                let key = format!("key-{i}");
                let mut allowed = 0;
                for _ in 0..20 {
                    if l.check(&key) {
                        allowed += 1;
                    }
                }
                allowed
            }));
        }

        let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
        assert!(total >= 40, "expected at least 40 allowed, got {total}");
        assert_eq!(limiter.key_count(), 4);
    }

    #[test]
    fn stats_tracking() {
        let limiter = RateLimiter::new(1.0, 2);
        let _ = limiter.check("a"); // allowed
        let _ = limiter.check("a"); // allowed
        let _ = limiter.check("a"); // rejected

        let stats = limiter.stats();
        assert_eq!(stats.total_allowed, 2);
        assert_eq!(stats.total_rejected, 1);
        assert_eq!(stats.active_keys, 1);
    }

    #[test]
    fn evict_stale_keys() {
        let limiter = RateLimiter::new(1.0, 1);
        let _ = limiter.check("fresh");
        let _ = limiter.check("stale");

        // Wait for stale to become idle.
        std::thread::sleep(Duration::from_millis(20));

        // Touch fresh again to keep it alive.
        let _ = limiter.check("fresh");

        let evicted = limiter.evict_stale(Duration::from_millis(15));
        assert_eq!(evicted, 1);
        assert_eq!(limiter.key_count(), 1); // only "fresh" remains

        let stats = limiter.stats();
        assert_eq!(stats.total_evicted, 1);
    }

    #[test]
    fn evict_stale_no_keys() {
        let limiter = RateLimiter::new(1.0, 1);
        let _ = limiter.check("a");
        let evicted = limiter.evict_stale(Duration::from_secs(60));
        assert_eq!(evicted, 0);
    }

    #[test]
    fn evict_all_stale() {
        let limiter = RateLimiter::new(1.0, 1);
        let _ = limiter.check("a");
        let _ = limiter.check("b");
        let _ = limiter.check("c");

        std::thread::sleep(Duration::from_millis(15));

        let evicted = limiter.evict_stale(Duration::from_millis(10));
        assert_eq!(evicted, 3);
        assert_eq!(limiter.key_count(), 0);
    }
}
