//! Topic-based pub/sub with MQTT-style wildcard matching.
//!
//! Patterns use `/`-delimited segments:
//! - `*` matches exactly one segment
//! - `#` matches zero or more trailing segments
//!
//! ```
//! use majra::pubsub::{PubSub, matches_pattern};
//!
//! assert!(matches_pattern("crew/*/status", "crew/abc/status"));
//! assert!(matches_pattern("crew/#", "crew/abc/tasks/1"));
//! assert!(!matches_pattern("crew/*/status", "crew/abc/tasks/status"));
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::trace;

/// Default per-pattern broadcast channel capacity.
const CHANNEL_CAPACITY: usize = 256;

/// Maximum topic hierarchy depth to prevent stack overflow in matching.
const MAX_MATCH_DEPTH: usize = 32;

/// A message published to a topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicMessage {
    /// The concrete topic the message was published to.
    pub topic: String,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// Publication timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Thread-safe pub/sub hub.
///
/// Subscribers register wildcard patterns; publishers send to concrete topics.
/// Delivery fans out to every pattern that matches the topic.
pub struct PubSub {
    subscriptions: DashMap<String, broadcast::Sender<TopicMessage>>,
    capacity: usize,
    messages_published: AtomicUsize,
}

impl PubSub {
    /// Create a new hub with the default channel capacity (256).
    pub fn new() -> Self {
        Self::with_capacity(CHANNEL_CAPACITY)
    }

    /// Create a new hub with a custom per-pattern channel capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            subscriptions: DashMap::new(),
            capacity,
            messages_published: AtomicUsize::new(0),
        }
    }

    /// Subscribe to a topic pattern. Returns a receiver for matching messages.
    pub fn subscribe(&self, pattern: &str) -> broadcast::Receiver<TopicMessage> {
        self.subscriptions
            .entry(pattern.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .subscribe()
    }

    /// Publish a message to a concrete topic.
    ///
    /// The message is delivered to every subscription whose pattern matches.
    pub fn publish(&self, topic: &str, payload: serde_json::Value) {
        let msg = TopicMessage {
            topic: topic.to_string(),
            payload,
            timestamp: Utc::now(),
        };

        let mut delivered = 0usize;
        for entry in self.subscriptions.iter() {
            if matches_pattern(entry.key(), topic) {
                // Ignore send errors — means no active receivers for this pattern.
                let _ = entry.value().send(msg.clone());
                delivered += 1;
            }
        }

        self.messages_published.fetch_add(1, Ordering::Relaxed);
        trace!(topic, delivered, "published");
    }

    /// Remove all subscriptions for a pattern.
    pub fn unsubscribe_all(&self, pattern: &str) {
        self.subscriptions.remove(pattern);
    }

    /// Number of active subscription patterns.
    pub fn pattern_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Total messages published since creation.
    pub fn messages_published(&self) -> usize {
        self.messages_published.load(Ordering::Relaxed)
    }
}

impl Default for PubSub {
    fn default() -> Self {
        Self::new()
    }
}

/// Check whether a wildcard `pattern` matches a concrete `topic`.
///
/// Segment separator is `/`. Wildcards:
/// - `*` matches exactly one segment
/// - `#` matches zero or more trailing segments (must be last)
pub fn matches_pattern(pattern: &str, topic: &str) -> bool {
    let pat_segs: Vec<&str> = pattern.split('/').collect();
    let top_segs: Vec<&str> = topic.split('/').collect();

    if pat_segs.len() > MAX_MATCH_DEPTH || top_segs.len() > MAX_MATCH_DEPTH {
        return false;
    }

    matches_recursive(&pat_segs, &top_segs)
}

fn matches_recursive(pattern: &[&str], topic: &[&str]) -> bool {
    match (pattern.first(), topic.first()) {
        (None, None) => true,
        (Some(&"#"), _) => true,
        (Some(&"*"), Some(_)) => matches_recursive(&pattern[1..], &topic[1..]),
        (Some(p), Some(t)) if p == t => matches_recursive(&pattern[1..], &topic[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        assert!(matches_pattern("a/b/c", "a/b/c"));
    }

    #[test]
    fn no_match() {
        assert!(!matches_pattern("a/b/c", "a/b/d"));
    }

    #[test]
    fn star_matches_one() {
        assert!(matches_pattern("a/*/c", "a/b/c"));
        assert!(!matches_pattern("a/*/c", "a/b/d/c"));
    }

    #[test]
    fn hash_matches_trailing() {
        assert!(matches_pattern("a/#", "a/b/c/d"));
        assert!(matches_pattern("a/#", "a"));
    }

    #[test]
    fn hash_at_root() {
        assert!(matches_pattern("#", "any/topic/at/all"));
    }

    #[test]
    fn depth_limit() {
        let deep = (0..MAX_MATCH_DEPTH + 1)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("/");
        assert!(!matches_pattern("#", &deep));
    }

    #[tokio::test]
    async fn publish_subscribe_roundtrip() {
        let hub = PubSub::new();
        let mut rx = hub.subscribe("crew/*/status");

        hub.publish("crew/abc/status", serde_json::json!({"done": true}));

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.topic, "crew/abc/status");
        assert_eq!(msg.payload["done"], true);
        assert_eq!(hub.messages_published(), 1);
    }

    #[tokio::test]
    async fn no_delivery_on_mismatch() {
        let hub = PubSub::new();
        let mut rx = hub.subscribe("crew/*/status");

        hub.publish("fleet/node-1/heartbeat", serde_json::Value::Null);

        // No message should be available.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn unsubscribe_removes_pattern() {
        let hub = PubSub::new();
        let _rx = hub.subscribe("a/b");
        assert_eq!(hub.pattern_count(), 1);
        hub.unsubscribe_all("a/b");
        assert_eq!(hub.pattern_count(), 0);
    }
}
