//! Sequenced, deduplicated inter-node message relay.
//!
//! Each node gets an atomic sequence counter. Receivers track the last seen
//! sequence per sender, dropping duplicates and out-of-order messages.

use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, trace};

use crate::util::Counter;

/// A node identifier (typically a UUID or hostname hash).
pub type NodeId = String;

/// Default broadcast channel capacity.
const DEFAULT_CAPACITY: usize = 256;

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
}

/// Sequenced message relay with deduplication.
///
/// Thread-safe — uses [`DashMap`] for the dedup table and atomics for counters.
pub struct Relay {
    node_id: NodeId,
    next_seq: AtomicU64,
    seen: DashMap<NodeId, u64>,
    tx: broadcast::Sender<IncomingMessage>,
    messages_sent: Counter,
    messages_received: Counter,
    duplicates_dropped: Counter,
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
            messages_sent: Counter::new(),
            messages_received: Counter::new(),
            duplicates_dropped: Counter::new(),
        }
    }

    /// Backward-compatible alias for [`Relay::new`].
    #[deprecated(note = "use Relay::new() instead")]
    pub fn with_defaults(node_id: impl Into<NodeId>) -> Self {
        Self::new(node_id)
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
        };

        let is_broadcast = msg.to.is_empty();
        let _ = self.tx.send(IncomingMessage {
            message: msg,
            is_broadcast,
        });

        self.messages_sent.inc();
        debug!(seq, "relay: sent");
        seq
    }

    /// Broadcast to all nodes. Returns the sequence number.
    pub fn broadcast(&self, topic: impl Into<String>, payload: serde_json::Value) -> u64 {
        self.send("", topic, payload)
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
            let mut last = self.seen.entry(msg.from.clone()).or_insert(0);
            if msg.seq <= *last {
                self.duplicates_dropped.inc();
                trace!(seq = msg.seq, from = %msg.from, "relay: duplicate dropped");
                return None;
            }
            *last = msg.seq;
        }

        self.messages_received.inc();

        let incoming = IncomingMessage {
            message: msg,
            is_broadcast,
        };

        let _ = self.tx.send(incoming.clone());
        Some(incoming)
    }

    /// This relay's node ID.
    #[inline]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Current relay statistics.
    pub fn stats(&self) -> RelayStats {
        RelayStats {
            messages_sent: self.messages_sent.get(),
            messages_received: self.messages_received.get(),
            duplicates_dropped: self.duplicates_dropped.get(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let msg = RelayMessage {
            seq: 5,
            from: "node-2".into(),
            to: "node-1".into(),
            topic: "t".into(),
            payload: serde_json::Value::Null,
            timestamp: Utc::now(),
        };
        assert!(relay.receive(msg).is_some());

        let dup = RelayMessage {
            seq: 3,
            from: "node-2".into(),
            to: "node-1".into(),
            topic: "t".into(),
            payload: serde_json::Value::Null,
            timestamp: Utc::now(),
        };
        assert!(relay.receive(dup).is_none());

        let stats = relay.stats();
        assert_eq!(stats.messages_received, 1);
        assert_eq!(stats.duplicates_dropped, 1);
    }

    #[test]
    fn skips_own_messages() {
        let relay = Relay::new("node-1");
        let msg = RelayMessage {
            seq: 1,
            from: "node-1".into(),
            to: "".into(),
            topic: "t".into(),
            payload: serde_json::Value::Null,
            timestamp: Utc::now(),
        };
        assert!(relay.receive(msg).is_none());
    }

    #[test]
    fn skips_messages_for_other_nodes() {
        let relay = Relay::new("node-1");
        let msg = RelayMessage {
            seq: 1,
            from: "node-2".into(),
            to: "node-3".into(),
            topic: "t".into(),
            payload: serde_json::Value::Null,
            timestamp: Utc::now(),
        };
        assert!(relay.receive(msg).is_none());
    }

    #[test]
    fn broadcast_reaches_all() {
        let relay = Relay::new("node-1");
        let msg = RelayMessage {
            seq: 1,
            from: "node-2".into(),
            to: "".into(),
            topic: "announce".into(),
            payload: serde_json::Value::Null,
            timestamp: Utc::now(),
        };
        let result = relay.receive(msg).unwrap();
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

    #[allow(deprecated)]
    #[test]
    fn with_defaults_backward_compat() {
        let relay = Relay::with_defaults("node-1");
        assert_eq!(relay.node_id(), "node-1");
    }
}
