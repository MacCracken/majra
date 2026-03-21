//! Per-key token bucket rate limiter.
//!
//! Lazy refill — tokens are replenished on each `check()` call based on
//! elapsed time. No background tasks required.

use std::time::Instant;

use dashmap::DashMap;

/// State for a single key's bucket.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Token bucket rate limiter with per-key tracking.
///
/// Thread-safe — backed by [`DashMap`] for concurrent per-key access.
pub struct RateLimiter {
    /// Tokens replenished per second.
    rate: f64,
    /// Maximum token capacity (burst size).
    burst: usize,
    buckets: DashMap<String, Bucket>,
}

impl RateLimiter {
    /// Create a limiter: `rate` tokens/sec, `burst` max tokens.
    pub fn new(rate: f64, burst: usize) -> Self {
        Self {
            rate,
            burst,
            buckets: DashMap::new(),
        }
    }

    /// Check if a request for `key` is allowed. Consumes one token if yes.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let burst = self.burst as f64;

        let mut entry = self.buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: burst,
            last_refill: now,
        });

        let bucket = entry.value_mut();

        // Refill based on elapsed time.
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.rate).min(burst);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Number of tracked keys.
    pub fn key_count(&self) -> usize {
        self.buckets.len()
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

        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(limiter.check("a"));
    }

    #[test]
    fn key_count() {
        let limiter = RateLimiter::new(1.0, 1);
        limiter.check("a");
        limiter.check("b");
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
        // Each key has burst=10, 4 keys = up to 40 allowed initially
        assert!(total >= 40, "expected at least 40 allowed, got {total}");
        assert_eq!(limiter.key_count(), 4);
    }
}
