//! Sequenced, deduplicated inter-node message relay.
//!
//! Each node gets an atomic sequence counter. Receivers track the last seen
//! sequence per sender, dropping duplicates and out-of-order messages.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, oneshot};
use tracing::{debug, trace};
use uuid::Uuid;

use crate::util::Counter;

/// A node identifier (typically a UUID or hostname hash).
pub type NodeId = String;

/// Default broadcast channel capacity.
const DEFAULT_CAPACITY: usize = 256;

/// Default maximum entries in the dedup table (0 = unbounded).
const DEFAULT_MAX_DEDUP_ENTRIES: usize = 0;

/// A message on the relay wire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayMessage {
    /// Monotonically increasing sequence number from the sender.
    pub seq: u64,
    /// Sender node.
    pub from: NodeId,
    /// Target node (empty string = broadcast).
    pub to: String,
    /// Topic for routing.
    pub topic: String,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// Creation timestamp.
    pub timestamp: DateTime<Utc>,
    /// Correlation ID for request-response pairing.
    ///
    /// When present on a request, the responder should copy this ID into the
    /// reply so the originator can match response to request.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// If `true`, this message is a reply to a prior request.
    #[serde(default)]
    pub is_reply: bool,
}

/// An inbound message after dedup filtering.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    /// The underlying relay message after dedup filtering.
    pub message: RelayMessage,
    /// `true` if the message was sent to all nodes (empty `to` field).
    pub is_broadcast: bool,
}

/// Relay statistics.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct RelayStats {
    /// Total number of messages sent from this node.
    pub messages_sent: u64,
    /// Total number of unique messages received (after dedup).
    pub messages_received: u64,
    /// Number of duplicate messages that were dropped.
    pub duplicates_dropped: u64,
    /// Number of dedup entries evicted by TTL.
    pub dedup_evicted: u64,
    /// Current number of entries in the dedup table.
    pub dedup_table_size: usize,
}

/// Sequenced message relay with deduplication.
///
/// Thread-safe — uses [`DashMap`] for the dedup table and atomics for counters.
pub struct Relay {
    node_id: NodeId,
    next_seq: AtomicU64,
    seen: DashMap<NodeId, (u64, Instant)>,
    tx: broadcast::Sender<IncomingMessage>,
    pending_requests: DashMap<String, (oneshot::Sender<RelayMessage>, Instant)>,
    max_dedup_entries: usize,
    messages_sent: Counter,
    messages_received: Counter,
    duplicates_dropped: Counter,
    dedup_evicted: Counter,
}

impl Relay {
    /// Create a relay for the given node with default capacity (256).
    pub fn new(node_id: impl Into<NodeId>) -> Self {
        Self::with_capacity(node_id, DEFAULT_CAPACITY)
    }

    /// Create a relay with a custom channel capacity.
    pub fn with_capacity(node_id: impl Into<NodeId>, capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            node_id: node_id.into(),
            next_seq: AtomicU64::new(1),
            seen: DashMap::new(),
            tx,
            pending_requests: DashMap::new(),
            max_dedup_entries: DEFAULT_MAX_DEDUP_ENTRIES,
            messages_sent: Counter::new(),
            messages_received: Counter::new(),
            duplicates_dropped: Counter::new(),
            dedup_evicted: Counter::new(),
        }
    }

    /// Set the maximum number of entries in the dedup table.
    ///
    /// When the limit is reached, the oldest entry (by last-seen time) is
    /// evicted automatically on each new `receive()`. Set to `0` (default)
    /// for unbounded.
    pub fn set_max_dedup_entries(&mut self, max: usize) {
        self.max_dedup_entries = max;
    }

    /// Send a message to a specific node. Returns the sequence number.
    pub fn send(
        &self,
        to: impl Into<String>,
        topic: impl Into<String>,
        payload: serde_json::Value,
    ) -> u64 {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let msg = RelayMessage {
            seq,
            from: self.node_id.clone(),
            to: to.into(),
            topic: topic.into(),
            payload,
            timestamp: Utc::now(),
            correlation_id: None,
            is_reply: false,
        };

        let is_broadcast = msg.to.is_empty();
        if self
            .tx
            .send(IncomingMessage {
                message: msg,
                is_broadcast,
            })
            .is_err()
        {
            trace!(seq, "relay: no subscribers for sent message");
        }

        self.messages_sent.inc();
        debug!(seq, "relay: sent");
        seq
    }

    /// Broadcast to all nodes. Returns the sequence number.
    pub fn broadcast(&self, topic: impl Into<String>, payload: serde_json::Value) -> u64 {
        self.send("", topic, payload)
    }

    /// Send a request and await a correlated reply.
    ///
    /// Returns `(sequence_number, receiver)`. The receiver resolves when the
    /// remote node calls [`Relay::reply`] with the matching correlation ID.
    ///
    /// The caller is responsible for adding a timeout (e.g. via
    /// `tokio::time::timeout`) to avoid waiting forever if the peer never
    /// replies.
    pub fn send_request(
        &self,
        to: impl Into<String>,
        topic: impl Into<String>,
        payload: serde_json::Value,
    ) -> (u64, oneshot::Receiver<RelayMessage>) {
        let correlation_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.pending_requests
            .insert(correlation_id.clone(), (tx, Instant::now()));

        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let msg = RelayMessage {
            seq,
            from: self.node_id.clone(),
            to: to.into(),
            topic: topic.into(),
            payload,
            timestamp: Utc::now(),
            correlation_id: Some(correlation_id),
            is_reply: false,
        };

        let is_broadcast = msg.to.is_empty();
        if self
            .tx
            .send(IncomingMessage {
                message: msg,
                is_broadcast,
            })
            .is_err()
        {
            trace!(seq, "relay: no subscribers for request");
        }

        self.messages_sent.inc();
        debug!(seq, "relay: sent request");
        (seq, rx)
    }

    /// Send a reply to a previously received request.
    ///
    /// The `correlation_id` must match the one from the incoming request.
    /// Returns the sequence number.
    pub fn reply(
        &self,
        to: impl Into<String>,
        topic: impl Into<String>,
        correlation_id: String,
        payload: serde_json::Value,
    ) -> u64 {
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        let msg = RelayMessage {
            seq,
            from: self.node_id.clone(),
            to: to.into(),
            topic: topic.into(),
            payload,
            timestamp: Utc::now(),
            correlation_id: Some(correlation_id),
            is_reply: true,
        };

        let is_broadcast = msg.to.is_empty();
        if self
            .tx
            .send(IncomingMessage {
                message: msg,
                is_broadcast,
            })
            .is_err()
        {
            trace!(seq, "relay: no subscribers for reply");
        }

        self.messages_sent.inc();
        debug!(seq, "relay: sent reply");
        seq
    }

    /// Number of pending request-response correlations.
    #[inline]
    pub fn pending_requests(&self) -> usize {
        self.pending_requests.len()
    }

    /// Evict pending requests that have been waiting longer than `timeout`.
    ///
    /// Stale requests are dropped — their oneshot receivers will get
    /// `RecvError`. Call periodically to prevent unbounded growth when
    /// peers fail to reply.
    ///
    /// Returns the number of requests evicted.
    pub fn evict_stale_requests(&self, timeout: Duration) -> usize {
        let now = Instant::now();
        let stale: Vec<String> = self
            .pending_requests
            .iter()
            .filter(|entry| now.duration_since(entry.value().1) > timeout)
            .map(|entry| entry.key().clone())
            .collect();

        let count = stale.len();
        for key in &stale {
            self.pending_requests.remove(key);
        }

        if count > 0 {
            debug!(count, "relay: evicted stale pending requests");
        }
        count
    }

    /// Subscribe to inbound messages on this relay.
    pub fn subscribe(&self) -> broadcast::Receiver<IncomingMessage> {
        self.tx.subscribe()
    }

    /// Process an incoming message. Returns `Some` if it passes dedup, `None` if dropped.
    pub fn receive(&self, msg: RelayMessage) -> Option<IncomingMessage> {
        // Skip our own messages.
        if msg.from == self.node_id {
            return None;
        }

        // Skip messages targeted at a different node.
        let is_broadcast = msg.to.is_empty();
        if !is_broadcast && msg.to != self.node_id {
            return None;
        }

        // Dedup by sequence.
        {
            let mut last = self
                .seen
                .entry(msg.from.clone())
                .or_insert((0, Instant::now()));
            if msg.seq <= last.0 {
                self.duplicates_dropped.inc();
                trace!(seq = msg.seq, from = %msg.from, "relay: duplicate dropped");
                return None;
            }
            *last = (msg.seq, Instant::now());
        }

        // Enforce dedup table size limit.
        if self.max_dedup_entries > 0 && self.seen.len() > self.max_dedup_entries {
            // Evict the oldest entry by last-seen time.
            if let Some(oldest_key) = self
                .seen
                .iter()
                .min_by_key(|entry| entry.value().1)
                .map(|entry| entry.key().clone())
            {
                self.seen.remove(&oldest_key);
                self.dedup_evicted.inc();
                trace!(%oldest_key, "relay: evicted oldest dedup entry (capacity)");
            }
        }

        self.messages_received.inc();

        // Resolve pending request-response correlation.
        if msg.is_reply
            && let Some(ref cid) = msg.correlation_id
            && let Some((_, (tx, _))) = self.pending_requests.remove(cid)
        {
            let _ = tx.send(msg.clone());
            trace!(
                correlation_id = cid.as_str(),
                "relay: resolved pending request"
            );
        }

        let incoming = IncomingMessage {
            message: msg,
            is_broadcast,
        };

        if self.tx.send(incoming.clone()).is_err() {
            trace!(
                seq = incoming.message.seq,
                "relay: no subscribers for received message"
            );
        }
        Some(incoming)
    }

    /// This relay's node ID.
    #[inline]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Shrink the dedup table by rebuilding it, reclaiming memory from
    /// removed entries. DashMap shards grow via power-of-2 doubling but
    /// never shrink — call this periodically in long-running processes
    /// with high key churn to avoid fragmentation.
    pub fn compact_dedup(&self) {
        self.seen.shrink_to_fit();
        self.pending_requests.shrink_to_fit();
    }

    /// Evict dedup entries that have been idle longer than `max_idle`.
    ///
    /// Call periodically to prevent unbounded growth of the dedup table in
    /// long-running processes where peers come and go.
    ///
    /// Returns the number of entries evicted.
    pub fn evict_stale_dedup(&self, max_idle: Duration) -> usize {
        let now = Instant::now();
        let stale: Vec<NodeId> = self
            .seen
            .iter()
            .filter(|entry| now.duration_since(entry.value().1) > max_idle)
            .map(|entry| entry.key().clone())
            .collect();

        let count = stale.len();
        for key in &stale {
            self.seen.remove(key);
        }

        if count > 0 {
            self.dedup_evicted.add(count as u64);
            debug!(count, "relay: evicted stale dedup entries");
        }
        count
    }

    /// Number of entries currently in the dedup table.
    #[inline]
    pub fn dedup_table_size(&self) -> usize {
        self.seen.len()
    }

    /// Current relay statistics.
    pub fn stats(&self) -> RelayStats {
        RelayStats {
            messages_sent: self.messages_sent.get(),
            messages_received: self.messages_received.get(),
            duplicates_dropped: self.duplicates_dropped.get(),
            dedup_evicted: self.dedup_evicted.get(),
            dedup_table_size: self.seen.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(seq: u64, from: &str, to: &str, topic: &str) -> RelayMessage {
        RelayMessage {
            seq,
            from: from.into(),
            to: to.into(),
            topic: topic.into(),
            payload: serde_json::Value::Null,
            timestamp: Utc::now(),
            correlation_id: None,
            is_reply: false,
        }
    }

    #[test]
    fn send_increments_seq() {
        let relay = Relay::new("node-1");
        let s1 = relay.send("node-2", "test", serde_json::Value::Null);
        let s2 = relay.send("node-2", "test", serde_json::Value::Null);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
    }

    #[test]
    fn dedup_drops_old_seq() {
        let relay = Relay::new("node-1");
        assert!(relay.receive(msg(5, "node-2", "node-1", "t")).is_some());
        assert!(relay.receive(msg(3, "node-2", "node-1", "t")).is_none());

        let stats = relay.stats();
        assert_eq!(stats.messages_received, 1);
        assert_eq!(stats.duplicates_dropped, 1);
    }

    #[test]
    fn skips_own_messages() {
        let relay = Relay::new("node-1");
        assert!(relay.receive(msg(1, "node-1", "", "t")).is_none());
    }

    #[test]
    fn skips_messages_for_other_nodes() {
        let relay = Relay::new("node-1");
        assert!(relay.receive(msg(1, "node-2", "node-3", "t")).is_none());
    }

    #[test]
    fn broadcast_reaches_all() {
        let relay = Relay::new("node-1");
        let result = relay.receive(msg(1, "node-2", "", "announce")).unwrap();
        assert!(result.is_broadcast);
    }

    #[test]
    fn concurrent_receive() {
        use std::sync::Arc;
        use std::thread;

        let relay = Arc::new(Relay::new("node-1"));
        let mut handles = Vec::new();

        for sender_idx in 0..4 {
            let r = relay.clone();
            handles.push(thread::spawn(move || {
                let from = format!("node-{}", sender_idx + 10);
                let mut received = 0;
                for seq in 1..=10u64 {
                    let msg = RelayMessage {
                        seq,
                        from: from.clone(),
                        to: "node-1".into(),
                        topic: "t".into(),
                        payload: serde_json::Value::Null,
                        timestamp: Utc::now(),
                        correlation_id: None,
                        is_reply: false,
                    };
                    if r.receive(msg).is_some() {
                        received += 1;
                    }
                }
                received
            }));
        }

        let total: usize = handles.into_iter().map(|h| h.join().unwrap()).sum();
        assert_eq!(total, 40); // 4 senders * 10 unique messages each
        let stats = relay.stats();
        assert_eq!(stats.messages_received, 40);
        assert_eq!(stats.duplicates_dropped, 0);
    }

    #[test]
    fn broadcast_sends_and_counts() {
        let relay = Relay::new("node-1");
        let seq = relay.broadcast("announce", serde_json::json!({"event": "up"}));
        assert_eq!(seq, 1);
        assert_eq!(relay.stats().messages_sent, 1);
    }

    #[test]
    fn subscribe_receives_sent_messages() {
        let relay = Relay::new("node-1");
        let mut rx = relay.subscribe();
        relay.send("node-2", "test", serde_json::Value::Null);
        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.message.seq, 1);
        assert_eq!(msg.message.topic, "test");
    }

    #[test]
    fn dedup_table_tracks_size() {
        let relay = Relay::new("node-1");
        assert_eq!(relay.dedup_table_size(), 0);

        relay.receive(msg(1, "node-2", "node-1", "t"));
        assert_eq!(relay.dedup_table_size(), 1);

        relay.receive(msg(1, "node-3", "node-1", "t"));
        assert_eq!(relay.dedup_table_size(), 2);
    }

    #[test]
    fn evict_stale_dedup_removes_old_entries() {
        let relay = Relay::new("node-1");

        // Receive from two senders.
        for sender in &["node-2", "node-3"] {
            relay.receive(msg(1, sender, "node-1", "t"));
        }
        assert_eq!(relay.dedup_table_size(), 2);

        // Zero duration evicts everything.
        let evicted = relay.evict_stale_dedup(Duration::ZERO);
        assert_eq!(evicted, 2);
        assert_eq!(relay.dedup_table_size(), 0);
        assert_eq!(relay.stats().dedup_evicted, 2);
    }

    #[test]
    fn evict_stale_dedup_keeps_fresh_entries() {
        let relay = Relay::new("node-1");
        relay.receive(msg(1, "node-2", "node-1", "t"));

        // Large TTL should keep everything.
        let evicted = relay.evict_stale_dedup(Duration::from_secs(3600));
        assert_eq!(evicted, 0);
        assert_eq!(relay.dedup_table_size(), 1);
    }

    #[test]
    fn stats_includes_dedup_fields() {
        let relay = Relay::new("node-1");
        let stats = relay.stats();
        assert_eq!(stats.dedup_evicted, 0);
        assert_eq!(stats.dedup_table_size, 0);
    }

    #[test]
    fn send_request_registers_pending() {
        let relay = Relay::new("node-1");
        let (_seq, _rx) = relay.send_request("node-2", "rpc/ping", serde_json::Value::Null);
        assert_eq!(relay.pending_requests(), 1);
        assert_eq!(relay.stats().messages_sent, 1);
    }

    #[test]
    fn reply_resolves_pending_request() {
        let requester = Relay::new("node-1");

        // Subscribe first so we can capture the request's correlation_id.
        let mut sub = requester.subscribe();

        // node-1 sends a request.
        let (seq, mut rx) = requester.send_request("node-2", "rpc/ping", serde_json::Value::Null);
        assert_eq!(requester.pending_requests(), 1);

        // Read the request from the broadcast to get its correlation_id.
        let request_msg = sub.try_recv().unwrap();
        let cid = request_msg.message.correlation_id.unwrap();

        // Simulate the reply arriving at node-1 via receive().
        let reply_msg = RelayMessage {
            seq: 1,
            from: "node-2".into(),
            to: "node-1".into(),
            topic: "rpc/ping".into(),
            payload: serde_json::json!({"pong": true}),
            timestamp: Utc::now(),
            correlation_id: Some(cid),
            is_reply: true,
        };
        requester.receive(reply_msg);

        // The pending request should be resolved.
        assert_eq!(requester.pending_requests(), 0);

        // The oneshot channel should have the response.
        let response = rx.try_recv().unwrap();
        assert_eq!(response.payload, serde_json::json!({"pong": true}));
        assert!(response.is_reply);
        assert!(seq > 0);
    }

    #[test]
    fn max_dedup_entries_evicts_oldest() {
        let mut relay = Relay::new("node-1");
        relay.set_max_dedup_entries(2);

        relay.receive(msg(1, "node-a", "node-1", "t"));
        relay.receive(msg(1, "node-b", "node-1", "t"));
        assert_eq!(relay.dedup_table_size(), 2);

        // Third sender should evict the oldest.
        relay.receive(msg(1, "node-c", "node-1", "t"));
        assert_eq!(relay.dedup_table_size(), 2);
        assert_eq!(relay.stats().dedup_evicted, 1);
    }

    #[test]
    fn reply_has_correlation_id_and_is_reply_flag() {
        let relay = Relay::new("node-1");
        let mut sub = relay.subscribe();
        relay.reply(
            "node-2",
            "rpc/ack",
            "corr-123".into(),
            serde_json::Value::Null,
        );
        let incoming = sub.try_recv().unwrap();
        assert!(incoming.message.is_reply);
        assert_eq!(incoming.message.correlation_id.as_deref(), Some("corr-123"));
    }
}
