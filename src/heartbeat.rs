//! TTL-based node health tracking.
//!
//! Tracks last-seen timestamps per node and transitions through:
//! `Online` → `Suspect` (after suspect timeout) → `Offline` (after offline timeout).
//!
//! This pattern is shared by AgnosAI's node registry, federation manager,
//! and SecureYeoman's A2A peer tracking.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::debug;

/// Default suspect threshold: 30 seconds without heartbeat.
const DEFAULT_SUSPECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default offline threshold: 90 seconds without heartbeat.
const DEFAULT_OFFLINE_TIMEOUT: Duration = Duration::from_secs(90);

/// Node health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Online,
    Suspect,
    Offline,
}

/// Configuration for health thresholds.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    pub suspect_after: Duration,
    pub offline_after: Duration,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            suspect_after: DEFAULT_SUSPECT_TIMEOUT,
            offline_after: DEFAULT_OFFLINE_TIMEOUT,
        }
    }
}

/// Tracked state for a single node.
#[derive(Debug, Clone)]
pub struct NodeState {
    pub status: Status,
    pub last_seen: Instant,
    /// Arbitrary metadata (capabilities, tags, etc.).
    pub metadata: serde_json::Value,
}

/// Health tracker for a set of nodes.
pub struct HeartbeatTracker {
    nodes: HashMap<String, NodeState>,
    config: HeartbeatConfig,
}

impl HeartbeatTracker {
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            nodes: HashMap::new(),
            config,
        }
    }

    /// Register a node (or re-register, resetting its state to Online).
    pub fn register(&mut self, id: impl Into<String>, metadata: serde_json::Value) {
        let id = id.into();
        debug!(node = %id, "heartbeat: registered");
        self.nodes.insert(
            id,
            NodeState {
                status: Status::Online,
                last_seen: Instant::now(),
                metadata,
            },
        );
    }

    /// Record a heartbeat from a node, resetting it to Online.
    pub fn heartbeat(&mut self, id: &str) -> bool {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = Status::Online;
            node.last_seen = Instant::now();
            true
        } else {
            false
        }
    }

    /// Remove a node from tracking.
    pub fn deregister(&mut self, id: &str) -> bool {
        self.nodes.remove(id).is_some()
    }

    /// Sweep all nodes, transitioning statuses based on elapsed time.
    ///
    /// Returns the IDs of nodes that transitioned to a new status.
    pub fn update_statuses(&mut self) -> Vec<(String, Status)> {
        let now = Instant::now();
        let mut transitions = Vec::new();

        for (id, node) in &mut self.nodes {
            let elapsed = now.duration_since(node.last_seen);
            let prev = node.status;

            node.status = if elapsed >= self.config.offline_after {
                Status::Offline
            } else if elapsed >= self.config.suspect_after {
                Status::Suspect
            } else {
                Status::Online
            };

            if node.status != prev {
                debug!(node = %id, from = ?prev, to = ?node.status, "heartbeat: transition");
                transitions.push((id.clone(), node.status));
            }
        }

        transitions
    }

    /// List nodes filtered by status.
    pub fn list_by_status(&self, status: Status) -> Vec<(&str, &NodeState)> {
        self.nodes
            .iter()
            .filter(|(_, n)| n.status == status)
            .map(|(id, n)| (id.as_str(), n))
            .collect()
    }

    /// List all online nodes.
    pub fn online(&self) -> Vec<(&str, &NodeState)> {
        self.list_by_status(Status::Online)
    }

    /// Get a node's current state.
    pub fn get(&self, id: &str) -> Option<&NodeState> {
        self.nodes.get(id)
    }

    /// Total tracked nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for HeartbeatTracker {
    fn default() -> Self {
        Self::new(HeartbeatConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_heartbeat() {
        let mut tracker = HeartbeatTracker::default();
        tracker.register("n1", serde_json::json!({"gpu": true}));
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.get("n1").unwrap().status, Status::Online);
        assert!(tracker.heartbeat("n1"));
        assert!(!tracker.heartbeat("unknown"));
    }

    #[test]
    fn deregister() {
        let mut tracker = HeartbeatTracker::default();
        tracker.register("n1", serde_json::Value::Null);
        assert!(tracker.deregister("n1"));
        assert!(!tracker.deregister("n1"));
        assert!(tracker.is_empty());
    }

    #[test]
    fn status_transitions() {
        let config = HeartbeatConfig {
            suspect_after: Duration::from_millis(10),
            offline_after: Duration::from_millis(50),
        };
        let mut tracker = HeartbeatTracker::new(config);
        tracker.register("n1", serde_json::Value::Null);

        // Immediately after registration: Online.
        let changes = tracker.update_statuses();
        assert!(changes.is_empty());

        // After suspect timeout.
        std::thread::sleep(Duration::from_millis(15));
        let changes = tracker.update_statuses();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], ("n1".to_string(), Status::Suspect));

        // Heartbeat resets to Online.
        tracker.heartbeat("n1");
        assert_eq!(tracker.get("n1").unwrap().status, Status::Online);
    }

    #[test]
    fn list_by_status() {
        let mut tracker = HeartbeatTracker::default();
        tracker.register("n1", serde_json::Value::Null);
        tracker.register("n2", serde_json::Value::Null);

        let online = tracker.online();
        assert_eq!(online.len(), 2);
    }
}
