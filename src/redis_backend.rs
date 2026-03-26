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
}
