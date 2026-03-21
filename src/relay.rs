//! Sequenced, deduplicated inter-node message relay.
//!
//! Each node gets an atomic sequence counter. Receivers track the last seen
//! sequence per sender, dropping duplicates and out-of-order messages.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::{debug, trace};

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
    pub message: RelayMessage,
    pub is_broadcast: bool,
}

/// Relay statistics.
#[derive(Debug, Clone, Default)]
pub struct RelayStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub duplicates_dropped: u64,
}

/// Sequenced message relay with deduplication.
pub struct Relay {
    node_id: NodeId,
    next_seq: AtomicU64,
    seen: Mutex<HashMap<NodeId, u64>>,
    tx: broadcast::Sender<IncomingMessage>,
    stats: Mutex<RelayStats>,
}

impl Relay {
    /// Create a relay for the given node with a custom channel capacity.
    pub fn new(node_id: impl Into<NodeId>, capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            node_id: node_id.into(),
            next_seq: AtomicU64::new(1),
            seen: Mutex::new(HashMap::new()),
            tx,
            stats: Mutex::new(RelayStats::default()),
        }
    }

    /// Create a relay with default capacity (256).
    pub fn with_defaults(node_id: impl Into<NodeId>) -> Self {
        Self::new(node_id, DEFAULT_CAPACITY)
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

        let mut stats = self.stats.lock().unwrap();
        stats.messages_sent += 1;

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
            let mut seen = self.seen.lock().unwrap();
            let last = seen.entry(msg.from.clone()).or_insert(0);
            if msg.seq <= *last {
                let mut stats = self.stats.lock().unwrap();
                stats.duplicates_dropped += 1;
                trace!(seq = msg.seq, from = %msg.from, "relay: duplicate dropped");
                return None;
            }
            *last = msg.seq;
        }

        let mut stats = self.stats.lock().unwrap();
        stats.messages_received += 1;

        let incoming = IncomingMessage {
            message: msg,
            is_broadcast,
        };

        let _ = self.tx.send(incoming.clone());
        Some(incoming)
    }

    /// This relay's node ID.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Current relay statistics.
    pub fn stats(&self) -> RelayStats {
        self.stats.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_increments_seq() {
        let relay = Relay::with_defaults("node-1");
        let s1 = relay.send("node-2", "test", serde_json::Value::Null);
        let s2 = relay.send("node-2", "test", serde_json::Value::Null);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
    }

    #[test]
    fn dedup_drops_old_seq() {
        let relay = Relay::with_defaults("node-1");

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
        let relay = Relay::with_defaults("node-1");
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
        let relay = Relay::with_defaults("node-1");
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
        let relay = Relay::with_defaults("node-1");
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
}
