//! Optional Redis backend for cross-process pub/sub and queues.
//!
//! Provides [`RedisPubSub`] for multi-process topic pub/sub and
//! [`RedisQueue`] for a distributed priority queue backed by Redis sorted sets.
//!
//! Requires the `redis-backend` feature flag.
//!
//! # Connection
//!
//! All types accept a `redis::Client` which manages connection pooling.
//! Connect with:
//!
//! ```rust,no_run
//! let client = redis::Client::open("redis://127.0.0.1/").unwrap();
//! ```

use redis::AsyncCommands;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::sync::broadcast;
use tracing::{debug, trace, warn};

use crate::error::MajraError;

// ---------------------------------------------------------------------------
// Redis Pub/Sub
// ---------------------------------------------------------------------------

/// Cross-process pub/sub backed by Redis Pub/Sub.
///
/// Messages published on one process are delivered to subscribers on any
/// connected process. Uses JSON serialization over the wire.
pub struct RedisPubSub {
    client: redis::Client,
    prefix: String,
}

impl RedisPubSub {
    /// Create a new Redis-backed pub/sub hub.
    ///
    /// `prefix` is prepended to all Redis channel names to namespace majra
    /// channels (e.g. `"majra:"` → Redis channel `"majra:events/node-1"`).
    pub fn new(client: redis::Client, prefix: impl Into<String>) -> Self {
        Self {
            client,
            prefix: prefix.into(),
        }
    }

    /// Publish a JSON-serializable message to a topic.
    ///
    /// Returns the number of Redis subscribers that received the message.
    pub async fn publish<T: Serialize>(
        &self,
        topic: &str,
        payload: &T,
    ) -> Result<usize, MajraError> {
        let channel = format!("{}{}", self.prefix, topic);
        let data = serde_json::to_string(payload).map_err(|e| MajraError::PubSub(e.to_string()))?;

        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::PubSub(format!("redis connection: {e}")))?;

        let receivers: usize = conn
            .publish(&channel, &data)
            .await
            .map_err(|e| MajraError::PubSub(format!("redis publish: {e}")))?;

        trace!(channel = channel.as_str(), receivers, "redis: published");
        Ok(receivers)
    }

    /// Subscribe to a topic pattern and receive messages on a broadcast channel.
    ///
    /// Spawns a background task that listens on the Redis subscription and
    /// forwards messages to the returned receiver. The task runs until the
    /// receiver is dropped or the Redis connection fails.
    ///
    /// `pattern` supports Redis glob patterns (`*`, `?`, `[...]`).
    pub fn subscribe<T: DeserializeOwned + Clone + Send + 'static>(
        &self,
        pattern: &str,
        capacity: usize,
    ) -> Result<broadcast::Receiver<(String, T)>, MajraError> {
        let (tx, rx) = broadcast::channel(capacity);
        let channel_pattern = format!("{}{}", self.prefix, pattern);
        let client = self.client.clone();
        let prefix_len = self.prefix.len();

        tokio::spawn(async move {
            let mut pubsub_conn = match client.get_async_pubsub().await {
                Ok(c) => c,
                Err(e) => {
                    warn!(error = %e, "redis subscribe: connection failed");
                    return;
                }
            };

            if let Err(e) = pubsub_conn.psubscribe(&channel_pattern).await {
                warn!(error = %e, "redis subscribe: psubscribe failed");
                return;
            }

            debug!(pattern = channel_pattern.as_str(), "redis: subscribed");

            use futures_util::StreamExt;
            let mut msg_stream = pubsub_conn.on_message();

            while let Some(msg) = msg_stream.next().await {
                let channel: String = match msg.get_channel() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let payload_str: String = match msg.get_payload() {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                // Strip prefix to get the original topic.
                let topic = if channel.len() > prefix_len {
                    channel[prefix_len..].to_string()
                } else {
                    channel.clone()
                };

                match serde_json::from_str::<T>(&payload_str) {
                    Ok(value) => {
                        if tx.send((topic, value)).is_err() {
                            // All receivers dropped.
                            break;
                        }
                    }
                    Err(e) => {
                        trace!(error = %e, "redis: failed to deserialize message");
                    }
                }
            }
        });

        Ok(rx)
    }

    /// Get the configured key prefix.
    #[inline]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

// ---------------------------------------------------------------------------
// Redis Queue
// ---------------------------------------------------------------------------

/// Cross-process priority queue backed by Redis sorted sets.
///
/// Jobs are stored as JSON strings in a Redis sorted set, with the score
/// derived from priority (higher priority = lower score = dequeued first).
/// Multiple processes can enqueue and dequeue from the same queue.
pub struct RedisQueue {
    client: redis::Client,
    key: String,
}

impl RedisQueue {
    /// Create a new Redis-backed queue.
    ///
    /// `key` is the Redis key for the sorted set (e.g. `"majra:queue:jobs"`).
    pub fn new(client: redis::Client, key: impl Into<String>) -> Self {
        Self {
            client,
            key: key.into(),
        }
    }

    /// Enqueue a job with the given priority.
    ///
    /// Higher priority values are dequeued first (Critical=4 before Background=0).
    /// Returns the queue length after insertion.
    pub async fn enqueue<T: Serialize>(
        &self,
        priority: u8,
        payload: &T,
    ) -> Result<usize, MajraError> {
        let data = serde_json::to_string(payload).map_err(|e| MajraError::Queue(e.to_string()))?;

        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Queue(format!("redis connection: {e}")))?;

        // Score = -priority so that higher priority sorts first (ZPOPMIN).
        let score = -(i64::from(priority));

        conn.zadd::<_, _, _, ()>(&self.key, &data, score)
            .await
            .map_err(|e| MajraError::Queue(format!("redis zadd: {e}")))?;

        let len: usize = conn
            .zcard(&self.key)
            .await
            .map_err(|e| MajraError::Queue(format!("redis zcard: {e}")))?;

        trace!(priority, len, "redis queue: enqueued");
        Ok(len)
    }

    /// Dequeue the highest-priority job.
    ///
    /// Uses `ZPOPMIN` for atomic pop. Returns `None` if the queue is empty.
    pub async fn dequeue<T: DeserializeOwned>(&self) -> Result<Option<T>, MajraError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Queue(format!("redis connection: {e}")))?;

        // ZPOPMIN returns the member with the lowest score (highest priority).
        let result: Vec<(String, f64)> = redis::cmd("ZPOPMIN")
            .arg(&self.key)
            .arg(1)
            .query_async(&mut conn)
            .await
            .map_err(|e| MajraError::Queue(format!("redis zpopmin: {e}")))?;

        match result.first() {
            Some((data, _score)) => {
                let value: T = serde_json::from_str(data)
                    .map_err(|e| MajraError::Queue(format!("deserialize: {e}")))?;
                trace!("redis queue: dequeued");
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Current queue length.
    pub async fn len(&self) -> Result<usize, MajraError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Queue(format!("redis connection: {e}")))?;

        let len: usize = conn
            .zcard(&self.key)
            .await
            .map_err(|e| MajraError::Queue(format!("redis zcard: {e}")))?;

        Ok(len)
    }

    /// Check if the queue is empty.
    pub async fn is_empty(&self) -> Result<bool, MajraError> {
        self.len().await.map(|n| n == 0)
    }

    /// Peek at the highest-priority item without removing it.
    pub async fn peek<T: DeserializeOwned>(&self) -> Result<Option<T>, MajraError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Queue(format!("redis connection: {e}")))?;

        let result: Vec<(String, f64)> = conn
            .zrangebyscore_limit_withscores(&self.key, "-inf", "+inf", 0, 1)
            .await
            .map_err(|e| MajraError::Queue(format!("redis zrangebyscore: {e}")))?;

        match result.first() {
            Some((data, _)) => {
                let value: T = serde_json::from_str(data)
                    .map_err(|e| MajraError::Queue(format!("deserialize: {e}")))?;
                Ok(Some(value))
            }
            None => Ok(None),
        }
    }

    /// Clear all items from the queue.
    pub async fn clear(&self) -> Result<(), MajraError> {
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Queue(format!("redis connection: {e}")))?;

        conn.del::<_, ()>(&self.key)
            .await
            .map_err(|e| MajraError::Queue(format!("redis del: {e}")))?;

        debug!(key = self.key.as_str(), "redis queue: cleared");
        Ok(())
    }

    /// Get the Redis key for this queue.
    #[inline]
    pub fn key(&self) -> &str {
        &self.key
    }
}

// ---------------------------------------------------------------------------
// Tests — require a running Redis instance, so they're ignored by default.
// Run with: cargo test --features redis-backend -- --ignored --nocapture
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Distributed Rate Limiter
// ---------------------------------------------------------------------------

/// Distributed token-bucket rate limiter backed by Redis.
///
/// Uses an atomic Lua script for check-and-decrement to avoid race conditions
/// across multiple processes. Each key stores `{tokens, last_refill_ms}` as
/// a Redis hash.
///
/// Compatible with the in-process [`crate::ratelimit::RateLimiter`] API style.
pub struct RedisRateLimiter {
    client: redis::Client,
    /// Tokens per second.
    rate: f64,
    /// Maximum burst.
    burst: usize,
    /// Key prefix in Redis.
    prefix: String,
}

impl RedisRateLimiter {
    /// Create a distributed rate limiter.
    ///
    /// - `rate`: tokens per second
    /// - `burst`: maximum token capacity
    /// - `prefix`: Redis key prefix (e.g. `"majra:rl:"`)
    pub fn new(client: redis::Client, rate: f64, burst: usize, prefix: impl Into<String>) -> Self {
        Self {
            client,
            rate,
            burst,
            prefix: prefix.into(),
        }
    }

    /// Check whether a request for `key` is allowed.
    ///
    /// Atomically refills tokens based on elapsed time and decrements if
    /// allowed. Returns `true` if the request should proceed.
    pub async fn check(&self, key: &str) -> crate::error::Result<bool> {
        let redis_key = format!("{}{key}", self.prefix);
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Queue(e.to_string()))?;

        // Lua script: atomic token bucket check-and-decrement.
        // KEYS[1] = bucket key (hash with fields: tokens, last_ms)
        // ARGV[1] = rate (tokens/sec)
        // ARGV[2] = burst (max tokens)
        // ARGV[3] = now_ms (current time in milliseconds)
        // Returns: 1 if allowed, 0 if rejected
        let script = redis::Script::new(
            r#"
            local key = KEYS[1]
            local rate = tonumber(ARGV[1])
            local burst = tonumber(ARGV[2])
            local now_ms = tonumber(ARGV[3])

            local tokens = tonumber(redis.call('HGET', key, 'tokens') or burst)
            local last_ms = tonumber(redis.call('HGET', key, 'last_ms') or now_ms)

            local elapsed_s = (now_ms - last_ms) / 1000.0
            tokens = math.min(tokens + elapsed_s * rate, burst)

            if tokens >= 1 then
                tokens = tokens - 1
                redis.call('HSET', key, 'tokens', tostring(tokens), 'last_ms', tostring(now_ms))
                redis.call('EXPIRE', key, math.ceil(burst / rate) + 60)
                return 1
            else
                redis.call('HSET', key, 'tokens', tostring(tokens), 'last_ms', tostring(now_ms))
                redis.call('EXPIRE', key, math.ceil(burst / rate) + 60)
                return 0
            end
            "#,
        );

        let now_ms = chrono::Utc::now().timestamp_millis();
        let result: i32 = script
            .key(&redis_key)
            .arg(self.rate)
            .arg(self.burst)
            .arg(now_ms)
            .invoke_async(&mut conn)
            .await
            .map_err(|e| MajraError::Queue(e.to_string()))?;

        trace!(key, allowed = result == 1, "redis-rl: check");
        Ok(result == 1)
    }

    /// Get the configured rate.
    #[inline]
    pub fn rate(&self) -> f64 {
        self.rate
    }

    /// Get the configured burst.
    #[inline]
    pub fn burst(&self) -> usize {
        self.burst
    }

    /// Get the key prefix.
    #[inline]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

// ---------------------------------------------------------------------------
// Distributed Heartbeat Tracker
// ---------------------------------------------------------------------------

/// Distributed heartbeat tracker backed by Redis.
///
/// Each node registers with a TTL-based key. Heartbeats refresh the TTL.
/// Status is derived from key presence: if the key exists, the node is
/// considered online; expired keys indicate offline nodes.
///
/// This enables cross-instance health coordination for edge deployments
/// where multiple majra processes need a shared view of fleet health.
pub struct RedisHeartbeatTracker {
    client: redis::Client,
    /// Key prefix (e.g. `"majra:hb:"`).
    prefix: String,
    /// TTL for heartbeat keys in seconds.
    ttl_secs: u64,
}

impl RedisHeartbeatTracker {
    /// Create a distributed heartbeat tracker.
    ///
    /// - `prefix`: Redis key prefix
    /// - `ttl_secs`: how long a heartbeat key survives without renewal
    pub fn new(client: redis::Client, prefix: impl Into<String>, ttl_secs: u64) -> Self {
        Self {
            client,
            prefix: prefix.into(),
            ttl_secs,
        }
    }

    /// Register a node with optional metadata. Sets the key with TTL.
    pub async fn register(
        &self,
        node_id: &str,
        metadata: &serde_json::Value,
    ) -> crate::error::Result<()> {
        let key = format!("{}{node_id}", self.prefix);
        let value =
            serde_json::to_string(metadata).map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        conn.set_ex::<_, _, ()>(&key, &value, self.ttl_secs)
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        debug!(node_id, ttl = self.ttl_secs, "redis-hb: registered");
        Ok(())
    }

    /// Send a heartbeat for a node, refreshing its TTL.
    pub async fn heartbeat(
        &self,
        node_id: &str,
        metadata: &serde_json::Value,
    ) -> crate::error::Result<()> {
        // Same as register — SET EX refreshes the key.
        self.register(node_id, metadata).await
    }

    /// Check if a node is online (key exists and hasn't expired).
    pub async fn is_online(&self, node_id: &str) -> crate::error::Result<bool> {
        let key = format!("{}{node_id}", self.prefix);
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        let exists: bool = conn
            .exists(&key)
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        Ok(exists)
    }

    /// Get metadata for a node (returns `None` if offline/expired).
    pub async fn get_metadata(
        &self,
        node_id: &str,
    ) -> crate::error::Result<Option<serde_json::Value>> {
        let key = format!("{}{node_id}", self.prefix);
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        let value: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        match value {
            Some(v) => {
                let parsed: serde_json::Value =
                    serde_json::from_str(&v).map_err(|e| MajraError::Heartbeat(e.to_string()))?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }

    /// List all online nodes by scanning keys with the configured prefix.
    ///
    /// Returns `(node_id, metadata)` pairs.
    pub async fn list_online(&self) -> crate::error::Result<Vec<(String, serde_json::Value)>> {
        let pattern = format!("{}*", self.prefix);
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;

        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;

        let mut result = Vec::with_capacity(keys.len());
        for key in &keys {
            let value: Option<String> = conn
                .get(key)
                .await
                .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
            if let Some(v) = value {
                let node_id = key.strip_prefix(&self.prefix).unwrap_or(key);
                let metadata: serde_json::Value =
                    serde_json::from_str(&v).map_err(|e| MajraError::Heartbeat(e.to_string()))?;
                result.push((node_id.to_string(), metadata));
            }
        }
        Ok(result)
    }

    /// Deregister a node by deleting its key.
    pub async fn deregister(&self, node_id: &str) -> crate::error::Result<()> {
        let key = format!("{}{node_id}", self.prefix);
        let mut conn = self
            .client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        conn.del::<_, ()>(&key)
            .await
            .map_err(|e| MajraError::Heartbeat(e.to_string()))?;
        debug!(node_id, "redis-hb: deregistered");
        Ok(())
    }

    /// Get the configured TTL in seconds.
    #[inline]
    pub fn ttl_secs(&self) -> u64 {
        self.ttl_secs
    }

    /// Get the key prefix.
    #[inline]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> redis::Client {
        redis::Client::open("redis://127.0.0.1/").expect("redis client")
    }

    // --- RedisQueue ---

    #[tokio::test]
    #[ignore]
    async fn queue_enqueue_dequeue() {
        let client = test_client();
        let q = RedisQueue::new(client, format!("majra:test:queue:{}", uuid::Uuid::new_v4()));

        // Enqueue with priorities.
        q.enqueue(2u8, &serde_json::json!({"job": "normal"}))
            .await
            .unwrap();
        q.enqueue(4u8, &serde_json::json!({"job": "critical"}))
            .await
            .unwrap();
        q.enqueue(0u8, &serde_json::json!({"job": "background"}))
            .await
            .unwrap();

        assert_eq!(q.len().await.unwrap(), 3);

        // Dequeue should return highest priority first.
        let first: serde_json::Value = q.dequeue().await.unwrap().unwrap();
        assert_eq!(first["job"], "critical");

        let second: serde_json::Value = q.dequeue().await.unwrap().unwrap();
        assert_eq!(second["job"], "normal");

        let third: serde_json::Value = q.dequeue().await.unwrap().unwrap();
        assert_eq!(third["job"], "background");

        assert!(q.is_empty().await.unwrap());

        q.clear().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn queue_dequeue_empty() {
        let client = test_client();
        let q = RedisQueue::new(client, format!("majra:test:queue:{}", uuid::Uuid::new_v4()));

        let result: Option<serde_json::Value> = q.dequeue().await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn queue_peek() {
        let client = test_client();
        let q = RedisQueue::new(client, format!("majra:test:queue:{}", uuid::Uuid::new_v4()));

        q.enqueue(3u8, &serde_json::json!({"job": "high"}))
            .await
            .unwrap();

        let peeked: serde_json::Value = q.peek().await.unwrap().unwrap();
        assert_eq!(peeked["job"], "high");

        // Peek shouldn't remove.
        assert_eq!(q.len().await.unwrap(), 1);

        q.clear().await.unwrap();
    }

    // --- RedisPubSub ---

    #[tokio::test]
    #[ignore]
    async fn pubsub_publish() {
        let client = test_client();
        let hub = RedisPubSub::new(client, "majra:test:ps:");

        // Publish without subscribers — should return 0.
        let receivers = hub
            .publish("events/test", &serde_json::json!({"hello": "world"}))
            .await
            .unwrap();
        assert_eq!(receivers, 0);
    }

    #[test]
    fn pubsub_prefix() {
        let client = test_client();
        let hub = RedisPubSub::new(client, "majra:");
        assert_eq!(hub.prefix(), "majra:");
    }

    #[test]
    fn queue_key() {
        let client = test_client();
        let q = RedisQueue::new(client, "majra:queue:jobs");
        assert_eq!(q.key(), "majra:queue:jobs");
    }

    #[test]
    fn ratelimiter_config() {
        let client = test_client();
        let rl = RedisRateLimiter::new(client, 10.0, 100, "majra:rl:");
        assert_eq!(rl.rate(), 10.0);
        assert_eq!(rl.burst(), 100);
        assert_eq!(rl.prefix(), "majra:rl:");
    }

    #[test]
    fn heartbeat_tracker_config() {
        let client = test_client();
        let hb = RedisHeartbeatTracker::new(client, "majra:hb:", 30);
        assert_eq!(hb.ttl_secs(), 30);
        assert_eq!(hb.prefix(), "majra:hb:");
    }
}
