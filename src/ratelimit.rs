//! Per-key token bucket rate limiter.
//!
//! Lazy refill — tokens are replenished on each `check()` call based on
//! elapsed time. No background tasks required.

use std::collections::HashMap;
use std::time::Instant;

/// State for a single key's bucket.
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Token bucket rate limiter with per-key tracking.
pub struct RateLimiter {
    /// Tokens replenished per second.
    rate: f64,
    /// Maximum token capacity (burst size).
    burst: usize,
    buckets: std::sync::Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    /// Create a limiter: `rate` tokens/sec, `burst` max tokens.
    pub fn new(rate: f64, burst: usize) -> Self {
        Self {
            rate,
            burst,
            buckets: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Check if a request for `key` is allowed. Consumes one token if yes.
    pub fn check(&self, key: &str) -> bool {
        let mut buckets = self.buckets.lock().unwrap();
        let now = Instant::now();
        let burst = self.burst as f64;

        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: burst,
            last_refill: now,
        });

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
        self.buckets.lock().unwrap().len()
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
}
