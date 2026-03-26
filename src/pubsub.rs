//! Topic-based pub/sub with MQTT-style wildcard matching.
//!
//! Patterns use `/`-delimited segments:
//! - `*` matches exactly one segment
//! - `#` matches zero or more trailing segments
//!
//! Two variants are provided:
//! - [`PubSub`] — untyped (`serde_json::Value` payloads)
//! - [`TypedPubSub<T>`] — generic, type-safe payloads
//!
//! ```
//! use majra::pubsub::{PubSub, matches_pattern};
//!
//! assert!(matches_pattern("crew/*/status", "crew/abc/status"));
//! assert!(matches_pattern("crew/#", "crew/abc/tasks/1"));
//! assert!(!matches_pattern("crew/*/status", "crew/abc/tasks/status"));
//! ```

use std::collections::VecDeque;
use std::sync::Arc;

use crate::util::Counter;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::trace;

/// Default per-pattern broadcast channel capacity.
const CHANNEL_CAPACITY: usize = 256;

/// Maximum topic hierarchy depth to prevent stack overflow in matching.
const MAX_MATCH_DEPTH: usize = 32;

/// Default number of publishes between automatic dead-subscriber cleanups.
const DEFAULT_CLEANUP_INTERVAL: u64 = 1_000;

/// Returns true if the pattern contains wildcard characters (`*` or `#`).
#[inline]
fn is_wildcard_pattern(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('#')
}

/// Subscriber count per pattern above which `tokio::sync::broadcast` send
/// latency degrades quadratically. We emit a tracing warning at this threshold.
const SUBSCRIBER_WARN_THRESHOLD: usize = 40;

// ---------------------------------------------------------------------------
// Backpressure policy
// ---------------------------------------------------------------------------

/// Behaviour when a subscriber's channel is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackpressurePolicy {
    /// Drop the oldest message in the channel (default `broadcast` behaviour).
    DropOldest,
    /// Drop the new message instead of sending.
    DropNewest,
}

// ---------------------------------------------------------------------------
// Untyped PubSub (original)
// ---------------------------------------------------------------------------

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
///
/// Dead subscribers (receivers that have been dropped) are cleaned up
/// automatically every 1 000 publishes. Call [`PubSub::cleanup_dead_subscribers`]
/// manually for immediate cleanup.
pub struct PubSub {
    subscriptions: DashMap<String, broadcast::Sender<TopicMessage>>,
    capacity: usize,
    messages_published: Counter,
    cleanup_interval: u64,
    max_subscriptions: usize,
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
            messages_published: Counter::new(),
            cleanup_interval: DEFAULT_CLEANUP_INTERVAL,
            max_subscriptions: 0,
        }
    }

    /// Set the number of publishes between automatic dead-subscriber cleanups.
    ///
    /// Set to `0` to disable automatic cleanup entirely.
    pub fn set_cleanup_interval(&mut self, interval: u64) {
        self.cleanup_interval = interval;
    }

    /// Set the maximum number of subscription patterns allowed.
    ///
    /// Set to `0` (default) for unbounded.
    pub fn set_max_subscriptions(&mut self, max: usize) {
        self.max_subscriptions = max;
    }

    /// Subscribe to a topic pattern. Returns a receiver for matching messages.
    pub fn subscribe(&self, pattern: &str) -> broadcast::Receiver<TopicMessage> {
        self.subscriptions
            .entry(pattern.to_string())
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .subscribe()
    }

    /// Subscribe with capacity check. Returns `Err` if `max_subscriptions`
    /// is set and the limit has been reached (existing patterns can always
    /// add more receivers).
    pub fn try_subscribe(
        &self,
        pattern: &str,
    ) -> crate::error::Result<broadcast::Receiver<TopicMessage>> {
        if self.max_subscriptions > 0
            && !self.subscriptions.contains_key(pattern)
            && self.subscriptions.len() >= self.max_subscriptions
        {
            return Err(crate::error::MajraError::CapacityExceeded(format!(
                "max subscription patterns ({}) reached",
                self.max_subscriptions
            )));
        }

        Ok(self.subscribe(pattern))
    }

    /// Publish a message to a concrete topic.
    ///
    /// The message is delivered to every subscription whose pattern matches.
    /// Returns the number of subscriptions the message was delivered to.
    pub fn publish(&self, topic: &str, payload: serde_json::Value) -> usize {
        let msg = TopicMessage {
            topic: topic.to_string(),
            payload,
            timestamp: Utc::now(),
        };

        let mut delivered = 0usize;
        for entry in self.subscriptions.iter() {
            if matches_pattern(entry.key(), topic) {
                if entry.value().send(msg.clone()).is_err() {
                    trace!(
                        topic,
                        pattern = entry.key().as_str(),
                        "no active receivers for pattern"
                    );
                }
                delivered += 1;
            }
        }

        self.messages_published.inc();
        trace!(topic, delivered, "published");

        // Periodic automatic cleanup.
        if self.cleanup_interval > 0
            && self
                .messages_published
                .get()
                .is_multiple_of(self.cleanup_interval)
        {
            self.cleanup_dead_subscribers();
        }

        delivered
    }

    /// Remove all subscriptions for a pattern.
    pub fn unsubscribe_all(&self, pattern: &str) {
        self.subscriptions.remove(pattern);
    }

    /// Number of active subscription patterns.
    #[inline]
    pub fn pattern_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Total messages published since creation.
    #[inline]
    pub fn messages_published(&self) -> u64 {
        self.messages_published.get()
    }

    /// Remove subscription patterns whose receivers have all been dropped.
    ///
    /// Returns the number of dead patterns removed. Also runs automatically
    /// every 1,000 publishes (configurable via `set_cleanup_interval`).
    pub fn cleanup_dead_subscribers(&self) -> usize {
        let dead: Vec<String> = self
            .subscriptions
            .iter()
            .filter(|entry| entry.value().receiver_count() == 0)
            .map(|entry| entry.key().clone())
            .collect();

        let count = dead.len();
        for pattern in &dead {
            self.subscriptions
                .remove_if(pattern, |_, tx| tx.receiver_count() == 0);
        }

        if count > 0 {
            trace!(count, "pubsub: cleaned up dead patterns");
        }
        count
    }
}

impl Default for PubSub {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Typed PubSub<T>
// ---------------------------------------------------------------------------

/// A typed message published to a topic.
#[derive(Debug, Clone)]
pub struct TypedMessage<T> {
    /// The concrete topic the message was published to.
    pub topic: String,
    /// Typed payload.
    pub payload: T,
    /// Publication timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Configuration for a typed pub/sub hub.
#[derive(Debug, Clone)]
pub struct TypedPubSubConfig {
    /// Per-pattern broadcast channel capacity.
    pub channel_capacity: usize,
    /// Backpressure policy when channels are full.
    pub backpressure: BackpressurePolicy,
    /// Replay buffer capacity (0 = disabled).
    pub replay_capacity: usize,
    /// Number of publishes between automatic dead-subscriber cleanups (0 = disabled).
    pub cleanup_interval: u64,
    /// Maximum number of subscription patterns allowed (0 = unbounded).
    pub max_subscriptions: usize,
}

impl Default for TypedPubSubConfig {
    fn default() -> Self {
        Self {
            channel_capacity: CHANNEL_CAPACITY,
            backpressure: BackpressurePolicy::DropOldest,
            replay_capacity: 0,
            cleanup_interval: DEFAULT_CLEANUP_INTERVAL,
            max_subscriptions: 0,
        }
    }
}

/// A boxed predicate filter for subscription filtering.
type SubscriptionFilter<T> = Arc<dyn Fn(&T) -> bool + Send + Sync>;

/// Internal subscription state for a single pattern.
struct TypedSubscription<T: Clone + Send + Sync + 'static> {
    sender: broadcast::Sender<TypedMessage<T>>,
    filter: Option<SubscriptionFilter<T>>,
}

/// Thread-safe, typed pub/sub hub with backpressure, replay, and filters.
///
/// Unlike [`PubSub`] which uses `serde_json::Value`, this variant is generic
/// over the payload type `T`, giving compile-time type safety.
///
/// **Performance note**: `tokio::sync::broadcast` send latency degrades
/// quadratically past ~40 subscribers per pattern. For high fan-out
/// scenarios, consider fewer patterns with broader wildcards.
pub struct TypedPubSub<T: Clone + Send + Sync + 'static> {
    /// Exact-topic subscriptions — O(1) direct lookup on publish. Fast path.
    exact_subscriptions: DashMap<String, Vec<TypedSubscription<T>>>,
    /// Wildcard-pattern subscriptions — iterated on publish. Slow path.
    pattern_subscriptions: DashMap<String, Vec<TypedSubscription<T>>>,
    config: TypedPubSubConfig,
    messages_published: Counter,
    messages_dropped: Counter,
    replay_buffer: DashMap<String, VecDeque<TypedMessage<T>>>,
}

impl<T: Clone + Send + Sync + 'static> TypedPubSub<T> {
    /// Create a new typed hub with default configuration.
    pub fn new() -> Self {
        Self::with_config(TypedPubSubConfig::default())
    }

    /// Create a new typed hub with custom configuration.
    pub fn with_config(config: TypedPubSubConfig) -> Self {
        Self {
            exact_subscriptions: DashMap::new(),
            pattern_subscriptions: DashMap::new(),
            messages_published: Counter::new(),
            messages_dropped: Counter::new(),
            replay_buffer: DashMap::new(),
            config,
        }
    }

    /// Subscribe to a topic pattern. Returns a receiver for matching messages.
    pub fn subscribe(&self, pattern: &str) -> broadcast::Receiver<TypedMessage<T>> {
        let (tx, rx) = self.get_or_create_sender(pattern, None);

        // Replay buffered messages to this new subscriber.
        if self.config.replay_capacity > 0 {
            self.replay_to(&tx, pattern);
        }

        rx
    }

    /// Subscribe with capacity check. Returns `Err` if `max_subscriptions`
    /// is set and the limit has been reached.
    pub fn try_subscribe(
        &self,
        pattern: &str,
    ) -> crate::error::Result<broadcast::Receiver<TypedMessage<T>>> {
        if self.config.max_subscriptions > 0
            && (self.exact_subscriptions.len() + self.pattern_subscriptions.len())
                >= self.config.max_subscriptions
        {
            return Err(crate::error::MajraError::CapacityExceeded(format!(
                "max subscription patterns ({}) reached",
                self.config.max_subscriptions
            )));
        }
        Ok(self.subscribe(pattern))
    }

    /// Subscribe with a predicate filter. Only messages where `filter(&payload)`
    /// returns `true` will be delivered to this receiver.
    pub fn subscribe_filtered(
        &self,
        pattern: &str,
        filter: impl Fn(&T) -> bool + Send + Sync + 'static,
    ) -> broadcast::Receiver<TypedMessage<T>> {
        let (_tx, rx) = self.get_or_create_sender(pattern, Some(Arc::new(filter)));
        rx
    }

    fn get_or_create_sender(
        &self,
        pattern: &str,
        filter: Option<SubscriptionFilter<T>>,
    ) -> (
        broadcast::Sender<TypedMessage<T>>,
        broadcast::Receiver<TypedMessage<T>>,
    ) {
        let (tx, rx) = broadcast::channel(self.config.channel_capacity);
        let sub = TypedSubscription {
            sender: tx.clone(),
            filter,
        };
        let map = if is_wildcard_pattern(pattern) {
            &self.pattern_subscriptions
        } else {
            &self.exact_subscriptions
        };
        let mut entry = map.entry(pattern.to_string()).or_default();
        entry.push(sub);
        let count = entry.len();
        drop(entry);

        if count == SUBSCRIBER_WARN_THRESHOLD {
            tracing::warn!(
                pattern,
                count,
                "pubsub: subscriber count per pattern exceeds {SUBSCRIBER_WARN_THRESHOLD} \
                 — tokio::broadcast send latency may degrade quadratically"
            );
        }
        (tx, rx)
    }

    fn replay_to(&self, tx: &broadcast::Sender<TypedMessage<T>>, pattern: &str) {
        // Fast path: exact topic (no wildcards) — direct DashMap lookup.
        if !pattern.contains('*') && !pattern.contains('#') {
            if let Some(buf) = self.replay_buffer.get(pattern) {
                for msg in buf.value().iter() {
                    if tx.send(msg.clone()).is_err() {
                        return;
                    }
                }
            }
            return;
        }

        // Slow path: wildcard pattern — scan all topics.
        for entry in self.replay_buffer.iter() {
            if matches_pattern(pattern, entry.key()) {
                for msg in entry.value().iter() {
                    if tx.send(msg.clone()).is_err() {
                        trace!(
                            pattern,
                            topic = msg.topic.as_str(),
                            "replay: no active receivers"
                        );
                        return;
                    }
                }
            }
        }
    }

    /// Publish a typed message to a concrete topic.
    ///
    /// Returns the number of subscriptions the message was delivered to.
    pub fn publish(&self, topic: &str, payload: T) -> usize {
        let msg = TypedMessage {
            topic: topic.to_string(),
            payload,
            timestamp: Utc::now(),
        };

        // Store in replay buffer if enabled.
        if self.config.replay_capacity > 0 {
            let mut buf = self.replay_buffer.entry(msg.topic.clone()).or_default();
            buf.push_back(msg.clone());
            while buf.len() > self.config.replay_capacity {
                buf.pop_front();
            }
        }

        let mut delivered = 0usize;

        // Fast path: exact-topic subscribers — O(1) DashMap lookup.
        if let Some(subs) = self.exact_subscriptions.get(topic) {
            for sub in subs.value().iter() {
                if sub.filter.as_ref().is_some_and(|f| !f(&msg.payload)) {
                    continue;
                }
                match self.config.backpressure {
                    BackpressurePolicy::DropOldest => {
                        if sub.sender.send(msg.clone()).is_err() {
                            trace!(topic, "typed: no active receivers");
                        }
                        delivered += 1;
                    }
                    BackpressurePolicy::DropNewest => {
                        if sub.sender.len() < self.config.channel_capacity {
                            if sub.sender.send(msg.clone()).is_err() {
                                trace!(topic, "typed: no active receivers");
                            }
                            delivered += 1;
                        } else {
                            self.messages_dropped.inc();
                        }
                    }
                }
            }
        }

        // Slow path: wildcard-pattern subscribers — iterate only patterns.
        for entry in self.pattern_subscriptions.iter() {
            if matches_pattern(entry.key(), topic) {
                for sub in entry.value().iter() {
                    if sub.filter.as_ref().is_some_and(|f| !f(&msg.payload)) {
                        continue;
                    }

                    match self.config.backpressure {
                        BackpressurePolicy::DropOldest => {
                            if sub.sender.send(msg.clone()).is_err() {
                                trace!(topic, "typed: no active receivers");
                            }
                            delivered += 1;
                        }
                        BackpressurePolicy::DropNewest => {
                            if sub.sender.len() < self.config.channel_capacity {
                                if sub.sender.send(msg.clone()).is_err() {
                                    trace!(topic, "typed: no active receivers");
                                }
                                delivered += 1;
                            } else {
                                self.messages_dropped.inc();
                            }
                        }
                    }
                }
            }
        }

        self.messages_published.inc();
        trace!(topic, delivered, "typed: published");

        // Periodic automatic cleanup.
        if self.config.cleanup_interval > 0
            && self
                .messages_published
                .get()
                .is_multiple_of(self.config.cleanup_interval)
        {
            self.cleanup_dead_subscribers();
        }

        delivered
    }

    /// Remove all subscriptions for a pattern.
    pub fn unsubscribe_all(&self, pattern: &str) {
        self.exact_subscriptions.remove(pattern);
        self.pattern_subscriptions.remove(pattern);
    }

    /// Number of active subscription patterns (exact + wildcard).
    #[inline]
    pub fn pattern_count(&self) -> usize {
        self.exact_subscriptions.len() + self.pattern_subscriptions.len()
    }

    /// Total messages published since creation.
    #[inline]
    pub fn messages_published(&self) -> u64 {
        self.messages_published.get()
    }

    /// Total messages dropped due to backpressure (DropNewest policy only).
    #[inline]
    pub fn messages_dropped(&self) -> u64 {
        self.messages_dropped.get()
    }

    /// Clear the replay buffer for all topics.
    pub fn clear_replay(&self) {
        self.replay_buffer.clear();
    }

    /// Remove subscriptions whose receivers have all been dropped.
    ///
    /// Returns the number of dead subscriptions removed. Also runs automatically
    /// every `cleanup_interval` publishes (configurable via [`TypedPubSubConfig`]).
    pub fn cleanup_dead_subscribers(&self) -> usize {
        let mut removed = 0usize;

        for map in [&self.exact_subscriptions, &self.pattern_subscriptions] {
            let mut empty_patterns = Vec::new();
            for mut entry in map.iter_mut() {
                let before = entry.value().len();
                entry
                    .value_mut()
                    .retain(|sub| sub.sender.receiver_count() > 0);
                removed += before - entry.value().len();
                if entry.value().is_empty() {
                    empty_patterns.push(entry.key().clone());
                }
            }
            for pattern in &empty_patterns {
                map.remove_if(pattern, |_, subs| subs.is_empty());
            }
        }

        if removed > 0 {
            trace!(removed, "typed: cleaned up dead subscribers");
        }
        removed
    }

    /// Number of individual subscriptions across all patterns.
    pub fn subscriber_count(&self) -> usize {
        self.exact_subscriptions
            .iter()
            .chain(self.pattern_subscriptions.iter())
            .map(|entry| entry.value().len())
            .sum()
    }
}

impl<T: Clone + Send + Sync + 'static> Default for TypedPubSub<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// DirectChannel — zero-overhead pub/sub for maximum throughput
// ---------------------------------------------------------------------------

/// Zero-overhead broadcast channel for maximum throughput.
///
/// Unlike [`TypedPubSub`], this skips topic routing, timestamps, and message
/// wrapping — payloads are sent directly through `tokio::sync::broadcast`.
/// Use when topic matching is not needed and throughput is critical.
///
/// **Throughput**: ~20-50M msg/s (limited only by `broadcast::send` + `T::clone`).
///
/// ```rust
/// use majra::pubsub::DirectChannel;
///
/// let channel = DirectChannel::<i32>::new(1024);
/// let mut rx = channel.subscribe();
/// channel.publish(42);
/// ```
pub struct DirectChannel<T: Clone + Send + 'static> {
    tx: broadcast::Sender<T>,
    published: Counter,
}

impl<T: Clone + Send + 'static> DirectChannel<T> {
    /// Create a direct channel with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            published: Counter::new(),
        }
    }

    /// Subscribe to this channel.
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }

    /// Publish a value to all subscribers.
    ///
    /// Returns the number of active receivers that received the value.
    #[inline]
    pub fn publish(&self, value: T) -> usize {
        let count = self.tx.send(value).unwrap_or(0);
        self.published.inc();
        count
    }

    /// Number of active subscribers.
    #[inline]
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Total messages published since creation.
    #[inline]
    pub fn messages_published(&self) -> u64 {
        self.published.get()
    }
}

// ---------------------------------------------------------------------------
// HashedChannel — hashed topic routing with minimal overhead
// ---------------------------------------------------------------------------

/// Pre-computed topic hash for O(1) routing without string allocation.
///
/// Compute once at subscribe/publish time via [`TopicHash::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TopicHash(u64);

impl TopicHash {
    /// Hash a topic string into a `TopicHash`.
    #[inline]
    #[must_use]
    pub fn new(topic: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        topic.hash(&mut hasher);
        Self(hasher.finish())
    }

    /// The raw u64 hash value.
    #[inline]
    pub fn value(self) -> u64 {
        self.0
    }
}

/// A hashed message — minimal envelope with topic hash + coarse timestamp.
#[derive(Debug, Clone)]
pub struct HashedMessage<T> {
    /// Pre-computed topic hash (subscriber holds key map if string needed).
    pub topic_hash: TopicHash,
    /// Coarse monotonic timestamp (nanoseconds since channel creation).
    pub timestamp_ns: u64,
    /// Payload.
    pub payload: T,
}

/// High-throughput pub/sub with hashed topic routing.
///
/// Topics are pre-hashed to `u64` — no string allocation on publish.
/// Timestamps use a coarse monotonic clock — no syscall overhead.
/// Subscribers that need the original topic string maintain their own
/// hash-to-string map.
///
/// **Throughput**: ~30-50M msg/s (between [`DirectChannel`] and [`TypedPubSub`]).
///
/// ```rust
/// use majra::pubsub::{HashedChannel, TopicHash};
///
/// let channel = HashedChannel::<i32>::new(1024);
/// let topic = TopicHash::new("events/data");
/// let mut rx = channel.subscribe(topic);
/// channel.publish(topic, 42);
/// ```
pub struct HashedChannel<T: Clone + Send + 'static> {
    subscriptions: DashMap<u64, broadcast::Sender<HashedMessage<T>>>,
    epoch: std::time::Instant,
    capacity: usize,
    published: Counter,
}

impl<T: Clone + Send + 'static> HashedChannel<T> {
    /// Create a hashed channel with the given per-topic capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            subscriptions: DashMap::new(),
            epoch: std::time::Instant::now(),
            capacity,
            published: Counter::new(),
        }
    }

    /// Subscribe to a hashed topic.
    pub fn subscribe(&self, topic: TopicHash) -> broadcast::Receiver<HashedMessage<T>> {
        self.subscriptions
            .entry(topic.0)
            .or_insert_with(|| broadcast::channel(self.capacity).0)
            .subscribe()
    }

    /// Publish to a hashed topic. O(1) — single DashMap lookup, no string ops.
    ///
    /// Returns the number of receivers that got the message.
    #[inline]
    pub fn publish(&self, topic: TopicHash, payload: T) -> usize {
        let msg = HashedMessage {
            topic_hash: topic,
            timestamp_ns: self.epoch.elapsed().as_nanos() as u64,
            payload,
        };

        let delivered = if let Some(tx) = self.subscriptions.get(&topic.0) {
            tx.send(msg).unwrap_or(0)
        } else {
            0
        };

        self.published.inc();
        delivered
    }

    /// Number of active topic subscriptions.
    #[inline]
    pub fn topic_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Total messages published.
    #[inline]
    pub fn messages_published(&self) -> u64 {
        self.published.get()
    }

    /// Remove a topic subscription.
    pub fn unsubscribe(&self, topic: TopicHash) {
        self.subscriptions.remove(&topic.0);
    }
}

// Also implement Serialize/Deserialize bridge for TypedMessage when T supports it.
impl<T: Clone + Send + Sync + Serialize + 'static> TypedMessage<T> {
    /// Convert to an untyped `TopicMessage` by serializing the payload to JSON.
    ///
    /// Returns `Err` if the payload cannot be serialized.
    pub fn to_untyped(&self) -> Result<TopicMessage, serde_json::Error> {
        Ok(TopicMessage {
            topic: self.topic.clone(),
            payload: serde_json::to_value(&self.payload)?,
            timestamp: self.timestamp,
        })
    }
}

impl<T: Clone + Send + Sync + DeserializeOwned + 'static> TypedMessage<T> {
    /// Try to convert from an untyped `TopicMessage`.
    ///
    /// Returns `Err` if the payload cannot be deserialized into `T`.
    pub fn from_untyped(msg: &TopicMessage) -> Result<Self, serde_json::Error> {
        let payload: T = serde_json::from_value(msg.payload.clone())?;
        Ok(Self {
            topic: msg.topic.clone(),
            payload,
            timestamp: msg.timestamp,
        })
    }
}

// ---------------------------------------------------------------------------
// Pattern matching (shared by both variants)
// ---------------------------------------------------------------------------

/// Check whether a wildcard `pattern` matches a concrete `topic`.
///
/// Segment separator is `/`. Wildcards:
/// - `*` matches exactly one segment
/// - `#` matches zero or more trailing segments (must be last)
#[inline]
#[must_use]
pub fn matches_pattern(pattern: &str, topic: &str) -> bool {
    let mut pat = pattern.split('/');
    let mut top = topic.split('/');
    let mut depth = 0usize;

    loop {
        match (pat.next(), top.next()) {
            (None, None) => return true,
            (Some("#"), top_seg) => {
                // "#" matches remaining segments — but enforce depth limit.
                // depth = segments matched so far. Add 1 for the current
                // topic segment (if any) plus remaining topic segments.
                let remaining = if top_seg.is_some() {
                    1 + top.count()
                } else {
                    0
                };
                return depth + remaining < MAX_MATCH_DEPTH;
            }
            (Some("*"), Some(_)) => {
                depth += 1;
                if depth > MAX_MATCH_DEPTH {
                    return false;
                }
            }
            (Some(p), Some(t)) if p == t => {
                depth += 1;
                if depth > MAX_MATCH_DEPTH {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Pattern matching ---

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

    // --- Untyped PubSub ---

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

    // --- TypedPubSub ---

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestEvent {
        kind: String,
        value: i32,
    }

    #[tokio::test]
    async fn typed_publish_subscribe() {
        let hub = TypedPubSub::<TestEvent>::new();
        let mut rx = hub.subscribe("events/*");

        hub.publish(
            "events/progress",
            TestEvent {
                kind: "progress".into(),
                value: 42,
            },
        );

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.topic, "events/progress");
        assert_eq!(msg.payload.value, 42);
        assert_eq!(hub.messages_published(), 1);
    }

    #[tokio::test]
    async fn typed_no_delivery_on_mismatch() {
        let hub = TypedPubSub::<TestEvent>::new();
        let mut rx = hub.subscribe("events/progress");

        hub.publish(
            "events/error",
            TestEvent {
                kind: "error".into(),
                value: 1,
            },
        );

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn typed_subscription_filter() {
        let hub = TypedPubSub::<TestEvent>::new();

        // Only receive events with value > 10.
        let mut rx = hub.subscribe_filtered("events/#", |e: &TestEvent| e.value > 10);

        hub.publish(
            "events/low",
            TestEvent {
                kind: "low".into(),
                value: 5,
            },
        );
        hub.publish(
            "events/high",
            TestEvent {
                kind: "high".into(),
                value: 50,
            },
        );

        let msg = rx.recv().await.unwrap();
        assert_eq!(msg.payload.value, 50);
        // The low-value event should not have been delivered.
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn typed_replay_buffer() {
        let config = TypedPubSubConfig {
            replay_capacity: 3,
            ..Default::default()
        };
        let hub = TypedPubSub::<TestEvent>::with_config(config);

        // Publish before subscribing.
        for i in 1..=5 {
            hub.publish(
                "events/data",
                TestEvent {
                    kind: "data".into(),
                    value: i,
                },
            );
        }

        // Late subscriber should get the last 3 messages from replay.
        let mut rx = hub.subscribe("events/data");

        // Replay messages are sent eagerly on subscribe.
        let m1 = rx.try_recv().unwrap();
        let m2 = rx.try_recv().unwrap();
        let m3 = rx.try_recv().unwrap();
        assert_eq!(m1.payload.value, 3);
        assert_eq!(m2.payload.value, 4);
        assert_eq!(m3.payload.value, 5);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn typed_backpressure_drop_newest() {
        let config = TypedPubSubConfig {
            channel_capacity: 2,
            backpressure: BackpressurePolicy::DropNewest,
            replay_capacity: 0,
            ..Default::default()
        };
        let hub = TypedPubSub::<TestEvent>::with_config(config);
        let _rx = hub.subscribe("events/#");

        // Fill the channel (capacity 2).
        hub.publish(
            "events/a",
            TestEvent {
                kind: "a".into(),
                value: 1,
            },
        );
        hub.publish(
            "events/b",
            TestEvent {
                kind: "b".into(),
                value: 2,
            },
        );
        // This should be dropped.
        hub.publish(
            "events/c",
            TestEvent {
                kind: "c".into(),
                value: 3,
            },
        );

        assert_eq!(hub.messages_published(), 3);
        assert!(hub.messages_dropped() > 0);
    }

    #[test]
    fn typed_unsubscribe() {
        let hub = TypedPubSub::<TestEvent>::new();
        let _rx = hub.subscribe("events/#");
        assert_eq!(hub.pattern_count(), 1);
        hub.unsubscribe_all("events/#");
        assert_eq!(hub.pattern_count(), 0);
    }

    #[tokio::test]
    async fn typed_message_to_untyped() {
        let msg = TypedMessage {
            topic: "test".into(),
            payload: TestEvent {
                kind: "test".into(),
                value: 99,
            },
            timestamp: Utc::now(),
        };
        let untyped = msg.to_untyped().unwrap();
        assert_eq!(untyped.payload["value"], 99);

        let back = TypedMessage::<TestEvent>::from_untyped(&untyped)
            .expect("deserialization should succeed");
        assert_eq!(back.payload.value, 99);
    }

    #[tokio::test]
    async fn typed_clear_replay() {
        let config = TypedPubSubConfig {
            replay_capacity: 10,
            ..Default::default()
        };
        let hub = TypedPubSub::<TestEvent>::with_config(config);

        hub.publish(
            "events/a",
            TestEvent {
                kind: "a".into(),
                value: 1,
            },
        );
        hub.clear_replay();

        let mut rx = hub.subscribe("events/#");
        assert!(rx.try_recv().is_err()); // replay cleared
    }

    #[tokio::test]
    async fn typed_multiple_subscribers_same_pattern() {
        let hub = TypedPubSub::<TestEvent>::new();
        let mut rx1 = hub.subscribe("events/#");
        let mut rx2 = hub.subscribe("events/#");

        hub.publish(
            "events/x",
            TestEvent {
                kind: "x".into(),
                value: 7,
            },
        );

        assert_eq!(rx1.recv().await.unwrap().payload.value, 7);
        assert_eq!(rx2.recv().await.unwrap().payload.value, 7);
    }

    #[tokio::test]
    async fn typed_wildcard_patterns() {
        let hub = TypedPubSub::<TestEvent>::new();
        let mut rx_star = hub.subscribe("events/*/done");
        let mut rx_hash = hub.subscribe("events/#");

        hub.publish(
            "events/train/done",
            TestEvent {
                kind: "done".into(),
                value: 1,
            },
        );

        assert_eq!(rx_star.recv().await.unwrap().payload.value, 1);
        assert_eq!(rx_hash.recv().await.unwrap().payload.value, 1);
    }

    #[test]
    fn pubsub_default_creates_hub() {
        let hub = PubSub::default();
        assert_eq!(hub.pattern_count(), 0);
        assert_eq!(hub.messages_published(), 0);
    }

    #[test]
    fn typed_pubsub_default_creates_hub() {
        let hub = TypedPubSub::<TestEvent>::default();
        assert_eq!(hub.pattern_count(), 0);
        assert_eq!(hub.messages_published(), 0);
        assert_eq!(hub.messages_dropped(), 0);
    }

    #[test]
    fn typed_from_untyped_fails_on_wrong_type() {
        let untyped = TopicMessage {
            topic: "test".into(),
            payload: serde_json::json!("not a TestEvent"),
            timestamp: Utc::now(),
        };
        assert!(TypedMessage::<TestEvent>::from_untyped(&untyped).is_err());
    }

    // --- Dead subscriber cleanup ---

    #[test]
    fn pubsub_cleanup_dead_subscribers() {
        let hub = PubSub::new();
        let rx1 = hub.subscribe("a/b");
        let _rx2 = hub.subscribe("c/d");
        assert_eq!(hub.pattern_count(), 2);

        // Drop rx1 — its pattern should be cleaned up.
        drop(rx1);
        let removed = hub.cleanup_dead_subscribers();
        assert_eq!(removed, 1);
        assert_eq!(hub.pattern_count(), 1);
    }

    #[test]
    fn pubsub_cleanup_no_dead() {
        let hub = PubSub::new();
        let _rx = hub.subscribe("a/b");
        assert_eq!(hub.cleanup_dead_subscribers(), 0);
        assert_eq!(hub.pattern_count(), 1);
    }

    #[test]
    fn typed_cleanup_dead_subscribers() {
        let hub = TypedPubSub::<TestEvent>::new();
        let rx1 = hub.subscribe("events/a");
        let _rx2 = hub.subscribe("events/b");
        let rx3 = hub.subscribe_filtered("events/#", |e: &TestEvent| e.value > 10);
        assert_eq!(hub.subscriber_count(), 3);

        // Drop two subscribers.
        drop(rx1);
        drop(rx3);
        let removed = hub.cleanup_dead_subscribers();
        assert_eq!(removed, 2);
        assert_eq!(hub.subscriber_count(), 1);
    }

    #[test]
    fn typed_cleanup_removes_empty_patterns() {
        let hub = TypedPubSub::<TestEvent>::new();
        let rx1 = hub.subscribe("events/a");
        let _rx2 = hub.subscribe("events/b");
        assert_eq!(hub.pattern_count(), 2);

        drop(rx1);
        hub.cleanup_dead_subscribers();
        // Pattern "events/a" should be removed entirely.
        assert_eq!(hub.pattern_count(), 1);
    }

    #[test]
    fn pubsub_auto_cleanup_on_publish() {
        let mut hub = PubSub::new();
        hub.set_cleanup_interval(5); // clean every 5 publishes

        let rx = hub.subscribe("a/b");
        drop(rx); // dead subscriber
        assert_eq!(hub.pattern_count(), 1);

        // Publish 4 times — no cleanup yet.
        for _ in 0..4 {
            hub.publish("a/b", serde_json::Value::Null);
        }
        assert_eq!(hub.pattern_count(), 1);

        // 5th publish triggers cleanup.
        hub.publish("a/b", serde_json::Value::Null);
        assert_eq!(hub.pattern_count(), 0);
    }

    #[test]
    fn typed_auto_cleanup_on_publish() {
        let config = TypedPubSubConfig {
            cleanup_interval: 3,
            ..Default::default()
        };
        let hub = TypedPubSub::<TestEvent>::with_config(config);

        let rx = hub.subscribe("events/#");
        drop(rx);
        assert_eq!(hub.pattern_count(), 1);

        for i in 0..2 {
            hub.publish(
                "events/x",
                TestEvent {
                    kind: "x".into(),
                    value: i,
                },
            );
        }
        assert_eq!(hub.pattern_count(), 1);

        // 3rd publish triggers cleanup.
        hub.publish(
            "events/x",
            TestEvent {
                kind: "x".into(),
                value: 3,
            },
        );
        assert_eq!(hub.pattern_count(), 0);
    }

    #[test]
    fn typed_cleanup_partial_pattern() {
        let hub = TypedPubSub::<TestEvent>::new();
        let rx1 = hub.subscribe("events/#");
        let _rx2 = hub.subscribe("events/#"); // Same pattern, different subscriber.
        assert_eq!(hub.subscriber_count(), 2);
        assert_eq!(hub.pattern_count(), 1);

        drop(rx1);
        let removed = hub.cleanup_dead_subscribers();
        assert_eq!(removed, 1);
        // Pattern still exists because rx2 is alive.
        assert_eq!(hub.pattern_count(), 1);
        assert_eq!(hub.subscriber_count(), 1);
    }
}
