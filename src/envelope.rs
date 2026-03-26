//! Message envelope — the universal wire type for majra.
//!
//! Every message flowing through pub/sub, relay, or IPC shares this structure.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Routing target for a message.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Target {
    /// Deliver to a specific node or agent.
    Node(String),
    /// Deliver to all subscribers of a topic.
    Topic(String),
    /// Deliver to all connected peers.
    Broadcast,
}

/// A message envelope carrying a JSON payload with routing metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Envelope {
    /// Unique message identifier.
    pub id: Uuid,
    /// Sender identifier (node ID, agent ID, etc.).
    pub from: String,
    /// Routing target.
    pub to: Target,
    /// Arbitrary JSON payload.
    pub payload: serde_json::Value,
    /// When the message was created.
    pub timestamp: DateTime<Utc>,
}

impl Envelope {
    /// Create a new envelope with a generated ID and current timestamp.
    pub fn new(from: impl Into<String>, to: Target, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            from: from.into(),
            to,
            payload,
            timestamp: Utc::now(),
        }
    }

    /// Returns `true` if this is a broadcast message.
    #[inline]
    pub fn is_broadcast(&self) -> bool {
        matches!(self.to, Target::Broadcast)
    }

    /// Returns `true` if this targets a topic.
    #[inline]
    pub fn is_topic(&self) -> bool {
        matches!(self.to, Target::Topic(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_roundtrip() {
        let env = Envelope::new(
            "node-1",
            Target::Topic("crew.events".into()),
            serde_json::json!({"status": "ok"}),
        );
        let json = serde_json::to_string(&env).unwrap();
        let decoded: Envelope = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.id, env.id);
        assert_eq!(decoded.from, "node-1");
        assert!(decoded.is_topic());
    }

    #[test]
    fn broadcast_detection() {
        let env = Envelope::new("a", Target::Broadcast, serde_json::Value::Null);
        assert!(env.is_broadcast());
        assert!(!env.is_topic());
    }

    #[test]
    fn node_target() {
        let env = Envelope::new("a", Target::Node("b".into()), serde_json::Value::Null);
        assert!(!env.is_broadcast());
        assert!(!env.is_topic());
    }
}
