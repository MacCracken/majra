//! TTL-based node health tracking.
//!
//! Tracks last-seen timestamps per node and transitions through:
//! `Online` → `Suspect` (after suspect timeout) → `Offline` (after offline timeout).
//!
//! This pattern is shared by AgnosAI's node registry, federation manager,
//! and SecureYeoman's A2A peer tracking.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Default suspect threshold: 30 seconds without heartbeat.
const DEFAULT_SUSPECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Classify a node's status based on time since last heartbeat.
#[inline]
fn classify_status(elapsed: Duration, config: &HeartbeatConfig) -> Status {
    if elapsed >= config.offline_after {
        Status::Offline
    } else if elapsed >= config.suspect_after {
        Status::Suspect
    } else {
        Status::Online
    }
}

/// Default offline threshold: 90 seconds without heartbeat.
const DEFAULT_OFFLINE_TIMEOUT: Duration = Duration::from_secs(90);

/// Node health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Status {
    /// Node is healthy and responding within the suspect timeout.
    Online,
    /// Node has not responded within the suspect timeout but is not yet offline.
    Suspect,
    /// Node has not responded within the offline timeout.
    Offline,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Online => write!(f, "online"),
            Self::Suspect => write!(f, "suspect"),
            Self::Offline => write!(f, "offline"),
        }
    }
}

/// Structured GPU telemetry data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuTelemetry {
    /// GPU compute utilization as a percentage (0.0–100.0).
    pub utilization_pct: f32,
    /// GPU memory currently in use, in megabytes.
    pub memory_used_mb: u64,
    /// Total GPU memory capacity, in megabytes.
    pub memory_total_mb: u64,
    /// GPU temperature in degrees Celsius, if available.
    pub temperature_c: Option<f32>,
}

/// Aggregated fleet statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[must_use]
pub struct FleetStats {
    /// Total number of tracked nodes across all statuses.
    pub total_nodes: usize,
    /// Number of nodes currently online.
    pub online: usize,
    /// Number of nodes in suspect state.
    pub suspect: usize,
    /// Number of nodes currently offline.
    pub offline: usize,
    /// Total number of GPUs across all nodes.
    pub total_gpus: usize,
    /// Total VRAM capacity across all GPUs, in megabytes.
    pub total_vram_mb: u64,
    /// Available (unused) VRAM across all GPUs, in megabytes.
    pub available_vram_mb: u64,
}

/// Eviction policy configuration.
#[derive(Debug, Clone)]
pub struct EvictionPolicy {
    /// Evict after this many consecutive offline sweep cycles.
    pub offline_cycles: u32,
    /// Channel to receive evicted node IDs (optional).
    pub eviction_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
}

/// Configuration for health thresholds.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Duration after which a silent node is marked suspect.
    pub suspect_after: Duration,
    /// Duration after which a silent node is marked offline.
    pub offline_after: Duration,
    /// Optional policy for auto-evicting persistently offline nodes.
    pub eviction_policy: Option<EvictionPolicy>,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            suspect_after: DEFAULT_SUSPECT_TIMEOUT,
            offline_after: DEFAULT_OFFLINE_TIMEOUT,
            eviction_policy: None,
        }
    }
}

/// Tracked state for a single node.
#[derive(Debug, Clone)]
pub struct NodeState {
    /// Current health status of the node.
    pub status: Status,
    /// Timestamp of the last received heartbeat.
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
    /// Create a new tracker with the given configuration.
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

    /// Record a heartbeat and update metadata.
    pub fn heartbeat_with_metadata(&mut self, id: &str, metadata: serde_json::Value) -> bool {
        if let Some(node) = self.nodes.get_mut(id) {
            node.status = Status::Online;
            node.last_seen = Instant::now();
            node.metadata = metadata;
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

            node.status = classify_status(elapsed, &self.config);

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
    #[inline]
    pub fn get(&self, id: &str) -> Option<&NodeState> {
        self.nodes.get(id)
    }

    /// Total tracked nodes.
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if no nodes are being tracked.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for HeartbeatTracker {
    fn default() -> Self {
        Self::new(HeartbeatConfig::default())
    }
}

// ---------------------------------------------------------------------------
// Thread-safe variant
// ---------------------------------------------------------------------------

/// Extended node state with optional structured GPU telemetry.
#[derive(Debug, Clone)]
struct NodeStateExt {
    status: Status,
    last_seen: Instant,
    metadata: serde_json::Value,
    gpu_telemetry: Option<Vec<GpuTelemetry>>,
    /// Consecutive offline sweep cycles (for eviction).
    offline_cycles: u32,
}

impl NodeStateExt {
    /// Project to the public `NodeState`.
    fn to_node_state(&self) -> NodeState {
        NodeState {
            status: self.status,
            last_seen: self.last_seen,
            metadata: self.metadata.clone(),
        }
    }
}

/// Thread-safe heartbeat tracker backed by [`DashMap`].
///
/// All methods take `&self` and are safe for concurrent use.
/// Methods that return node data return owned values (cloned) since
/// `DashMap` cannot return references that outlive the iterator guard.
///
/// Supports structured GPU telemetry, fleet statistics aggregation,
/// and configurable auto-eviction of persistently offline nodes.
pub struct ConcurrentHeartbeatTracker {
    nodes: DashMap<String, NodeStateExt>,
    config: HeartbeatConfig,
}

impl ConcurrentHeartbeatTracker {
    /// Create a new concurrent tracker with the given configuration.
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            nodes: DashMap::new(),
            config,
        }
    }

    /// Register a node (or re-register, resetting its state to Online).
    pub fn register(&self, id: impl Into<String>, metadata: serde_json::Value) {
        let id = id.into();
        debug!(node = %id, "heartbeat: registered");
        self.nodes.insert(
            id,
            NodeStateExt {
                status: Status::Online,
                last_seen: Instant::now(),
                metadata,
                gpu_telemetry: None,
                offline_cycles: 0,
            },
        );
    }

    /// Register a node with initial GPU telemetry.
    pub fn register_with_telemetry(
        &self,
        id: impl Into<String>,
        metadata: serde_json::Value,
        gpu_telemetry: Vec<GpuTelemetry>,
    ) {
        let id = id.into();
        debug!(node = %id, gpus = gpu_telemetry.len(), "heartbeat: registered with telemetry");
        self.nodes.insert(
            id,
            NodeStateExt {
                status: Status::Online,
                last_seen: Instant::now(),
                metadata,
                gpu_telemetry: Some(gpu_telemetry),
                offline_cycles: 0,
            },
        );
    }

    /// Record a heartbeat from a node, resetting it to Online.
    pub fn heartbeat(&self, id: &str) -> bool {
        if let Some(mut node) = self.nodes.get_mut(id) {
            node.status = Status::Online;
            node.last_seen = Instant::now();
            node.offline_cycles = 0;
            true
        } else {
            false
        }
    }

    /// Record a heartbeat and update metadata.
    pub fn heartbeat_with_metadata(&self, id: &str, metadata: serde_json::Value) -> bool {
        if let Some(mut node) = self.nodes.get_mut(id) {
            node.status = Status::Online;
            node.last_seen = Instant::now();
            node.metadata = metadata;
            node.offline_cycles = 0;
            true
        } else {
            false
        }
    }

    /// Record a heartbeat and update GPU telemetry.
    pub fn heartbeat_with_telemetry(&self, id: &str, gpu_telemetry: Vec<GpuTelemetry>) -> bool {
        if let Some(mut node) = self.nodes.get_mut(id) {
            node.status = Status::Online;
            node.last_seen = Instant::now();
            node.gpu_telemetry = Some(gpu_telemetry);
            node.offline_cycles = 0;
            true
        } else {
            false
        }
    }

    /// Remove a node from tracking.
    pub fn deregister(&self, id: &str) -> bool {
        self.nodes.remove(id).is_some()
    }

    /// Sweep all nodes, transitioning statuses based on elapsed time.
    ///
    /// Returns the IDs of nodes that transitioned to a new status.
    /// Also handles auto-eviction when an eviction policy is configured.
    pub fn update_statuses(&self) -> Vec<(String, Status)> {
        let now = Instant::now();
        let mut transitions = Vec::new();
        let mut evict_ids = Vec::new();

        for mut entry in self.nodes.iter_mut() {
            let elapsed = now.duration_since(entry.last_seen);
            let prev = entry.status;

            entry.status = classify_status(elapsed, &self.config);

            // Track offline cycles for eviction.
            if entry.status == Status::Offline {
                entry.offline_cycles += 1;
            } else {
                entry.offline_cycles = 0;
            }

            if entry.status != prev {
                debug!(node = %entry.key(), from = ?prev, to = ?entry.status, "heartbeat: transition");
                transitions.push((entry.key().clone(), entry.status));
            }

            // Check eviction threshold.
            if self
                .config
                .eviction_policy
                .as_ref()
                .is_some_and(|p| entry.offline_cycles >= p.offline_cycles)
            {
                evict_ids.push(entry.key().clone());
            }
        }

        // Perform evictions outside the iterator.
        for id in evict_ids {
            self.nodes.remove(&id);
            debug!(node = %id, "heartbeat: evicted");
            if let Some(tx) = self
                .config
                .eviction_policy
                .as_ref()
                .and_then(|p| p.eviction_tx.as_ref())
            {
                let _ = tx.send(id);
            }
        }

        transitions
    }

    /// List nodes filtered by status (returns owned tuples).
    pub fn list_by_status(&self, status: Status) -> Vec<(String, NodeState)> {
        self.nodes
            .iter()
            .filter(|entry| entry.value().status == status)
            .map(|entry| (entry.key().clone(), entry.value().to_node_state()))
            .collect()
    }

    /// List all online nodes (returns owned tuples).
    pub fn online(&self) -> Vec<(String, NodeState)> {
        self.list_by_status(Status::Online)
    }

    /// Get a node's current state (cloned).
    pub fn get(&self, id: &str) -> Option<NodeState> {
        self.nodes
            .get(id)
            .map(|entry| entry.value().to_node_state())
    }

    /// Get a node's GPU telemetry (cloned).
    pub fn get_gpu_telemetry(&self, id: &str) -> Option<Vec<GpuTelemetry>> {
        self.nodes
            .get(id)
            .and_then(|entry| entry.value().gpu_telemetry.clone())
    }

    /// Total tracked nodes.
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns `true` if no nodes are being tracked.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Aggregate fleet statistics across all tracked nodes.
    pub fn fleet_stats(&self) -> FleetStats {
        let mut stats = FleetStats::default();

        for entry in self.nodes.iter() {
            stats.total_nodes += 1;
            match entry.status {
                Status::Online => stats.online += 1,
                Status::Suspect => stats.suspect += 1,
                Status::Offline => stats.offline += 1,
            }

            if let Some(ref gpus) = entry.gpu_telemetry {
                stats.total_gpus += gpus.len();
                for gpu in gpus {
                    stats.total_vram_mb += gpu.memory_total_mb;
                    stats.available_vram_mb +=
                        gpu.memory_total_mb.saturating_sub(gpu.memory_used_mb);
                }
            }
        }

        stats
    }
}

impl Default for ConcurrentHeartbeatTracker {
    fn default() -> Self {
        Self::new(HeartbeatConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- HeartbeatTracker (original) ---

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
            suspect_after: Duration::from_millis(50),
            offline_after: Duration::from_millis(500),
            eviction_policy: None,
        };
        let mut tracker = HeartbeatTracker::new(config);
        tracker.register("n1", serde_json::Value::Null);

        // Immediately after registration: Online.
        let changes = tracker.update_statuses();
        assert!(changes.is_empty());

        // After suspect timeout.
        std::thread::sleep(Duration::from_millis(80));
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

    #[test]
    fn heartbeat_with_metadata_updates() {
        let mut tracker = HeartbeatTracker::default();
        tracker.register("n1", serde_json::json!({"v": 1}));
        assert!(tracker.heartbeat_with_metadata("n1", serde_json::json!({"v": 2})));
        assert_eq!(tracker.get("n1").unwrap().metadata["v"], 2);
        assert!(!tracker.heartbeat_with_metadata("unknown", serde_json::Value::Null));
    }

    // --- ConcurrentHeartbeatTracker ---

    #[test]
    fn concurrent_register_and_heartbeat() {
        let tracker = ConcurrentHeartbeatTracker::default();
        tracker.register("n1", serde_json::json!({"gpu": true}));
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.get("n1").unwrap().status, Status::Online);
        assert!(tracker.heartbeat("n1"));
        assert!(!tracker.heartbeat("unknown"));
    }

    #[test]
    fn concurrent_deregister() {
        let tracker = ConcurrentHeartbeatTracker::default();
        tracker.register("n1", serde_json::Value::Null);
        assert!(tracker.deregister("n1"));
        assert!(!tracker.deregister("n1"));
        assert!(tracker.is_empty());
    }

    #[test]
    fn concurrent_status_transitions() {
        let config = HeartbeatConfig {
            suspect_after: Duration::from_millis(50),
            offline_after: Duration::from_millis(500),
            eviction_policy: None,
        };
        let tracker = ConcurrentHeartbeatTracker::new(config);
        tracker.register("n1", serde_json::Value::Null);

        let changes = tracker.update_statuses();
        assert!(changes.is_empty());

        std::thread::sleep(Duration::from_millis(80));
        let changes = tracker.update_statuses();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0], ("n1".to_string(), Status::Suspect));

        tracker.heartbeat("n1");
        assert_eq!(tracker.get("n1").unwrap().status, Status::Online);
    }

    #[test]
    fn concurrent_list_by_status() {
        let tracker = ConcurrentHeartbeatTracker::default();
        tracker.register("n1", serde_json::Value::Null);
        tracker.register("n2", serde_json::Value::Null);

        let online = tracker.online();
        assert_eq!(online.len(), 2);
    }

    #[test]
    fn concurrent_heartbeat_with_metadata() {
        let tracker = ConcurrentHeartbeatTracker::default();
        tracker.register("n1", serde_json::json!({"v": 1}));
        assert!(tracker.heartbeat_with_metadata("n1", serde_json::json!({"v": 2})));
        assert_eq!(tracker.get("n1").unwrap().metadata["v"], 2);
    }

    #[test]
    fn concurrent_multi_thread_heartbeat() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(ConcurrentHeartbeatTracker::default());

        for i in 0..8 {
            tracker.register(format!("n-{i}"), serde_json::Value::Null);
        }

        let mut handles = Vec::new();
        for i in 0..8 {
            let t = tracker.clone();
            handles.push(thread::spawn(move || {
                let id = format!("n-{i}");
                for _ in 0..100 {
                    t.heartbeat(&id);
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(tracker.len(), 8);
        assert_eq!(tracker.online().len(), 8);
    }

    // --- GPU telemetry & fleet stats ---

    fn sample_gpu() -> GpuTelemetry {
        GpuTelemetry {
            utilization_pct: 75.0,
            memory_used_mb: 6000,
            memory_total_mb: 8000,
            temperature_c: Some(65.0),
        }
    }

    #[test]
    fn register_with_telemetry() {
        let tracker = ConcurrentHeartbeatTracker::default();
        tracker.register_with_telemetry("gpu-node", serde_json::Value::Null, vec![sample_gpu()]);

        let tel = tracker.get_gpu_telemetry("gpu-node").unwrap();
        assert_eq!(tel.len(), 1);
        assert_eq!(tel[0].memory_total_mb, 8000);
    }

    #[test]
    fn heartbeat_with_telemetry_updates() {
        let tracker = ConcurrentHeartbeatTracker::default();
        tracker.register("gpu-node", serde_json::Value::Null);

        let new_gpu = GpuTelemetry {
            utilization_pct: 50.0,
            memory_used_mb: 4000,
            memory_total_mb: 16000,
            temperature_c: None,
        };
        assert!(tracker.heartbeat_with_telemetry("gpu-node", vec![new_gpu]));
        assert!(!tracker.heartbeat_with_telemetry("unknown", vec![]));

        let tel = tracker.get_gpu_telemetry("gpu-node").unwrap();
        assert_eq!(tel[0].memory_total_mb, 16000);
    }

    #[test]
    fn fleet_stats_aggregation() {
        let config = HeartbeatConfig {
            suspect_after: Duration::from_millis(50),
            offline_after: Duration::from_millis(500),
            eviction_policy: None,
        };
        let tracker = ConcurrentHeartbeatTracker::new(config);

        // Node with 2 GPUs.
        tracker.register_with_telemetry(
            "n1",
            serde_json::Value::Null,
            vec![
                GpuTelemetry {
                    utilization_pct: 80.0,
                    memory_used_mb: 6000,
                    memory_total_mb: 8000,
                    temperature_c: Some(70.0),
                },
                GpuTelemetry {
                    utilization_pct: 20.0,
                    memory_used_mb: 2000,
                    memory_total_mb: 8000,
                    temperature_c: Some(45.0),
                },
            ],
        );

        // Node with no GPU.
        tracker.register("n2", serde_json::Value::Null);

        let stats = tracker.fleet_stats();
        assert_eq!(stats.total_nodes, 2);
        assert_eq!(stats.online, 2);
        assert_eq!(stats.total_gpus, 2);
        assert_eq!(stats.total_vram_mb, 16000);
        assert_eq!(stats.available_vram_mb, 8000); // (8000-6000) + (8000-2000)

        // Make n2 go suspect.
        std::thread::sleep(Duration::from_millis(80));
        tracker.heartbeat("n1"); // keep n1 online
        tracker.update_statuses();

        let stats = tracker.fleet_stats();
        assert_eq!(stats.online, 1);
        assert_eq!(stats.suspect, 1);
    }

    #[test]
    fn eviction_policy() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let config = HeartbeatConfig {
            suspect_after: Duration::from_millis(30),
            offline_after: Duration::from_millis(60),
            eviction_policy: Some(EvictionPolicy {
                offline_cycles: 2,
                eviction_tx: Some(tx),
            }),
        };
        let tracker = ConcurrentHeartbeatTracker::new(config);
        tracker.register("n1", serde_json::Value::Null);

        // Wait for offline.
        std::thread::sleep(Duration::from_millis(80));
        tracker.update_statuses(); // cycle 1
        assert_eq!(tracker.len(), 1);

        tracker.update_statuses(); // cycle 2 — should evict
        assert_eq!(tracker.len(), 0);

        // Should have received eviction notification.
        let evicted = rx.try_recv().unwrap();
        assert_eq!(evicted, "n1");
    }

    #[test]
    fn eviction_resets_on_heartbeat() {
        let config = HeartbeatConfig {
            suspect_after: Duration::from_millis(30),
            offline_after: Duration::from_millis(60),
            eviction_policy: Some(EvictionPolicy {
                offline_cycles: 3,
                eviction_tx: None,
            }),
        };
        let tracker = ConcurrentHeartbeatTracker::new(config);
        tracker.register("n1", serde_json::Value::Null);

        std::thread::sleep(Duration::from_millis(80));
        tracker.update_statuses(); // cycle 1
        tracker.update_statuses(); // cycle 2

        // Heartbeat resets cycles.
        tracker.heartbeat("n1");
        std::thread::sleep(Duration::from_millis(80));
        tracker.update_statuses(); // cycle 1 again (reset)
        assert_eq!(tracker.len(), 1); // not evicted
    }

    #[test]
    fn gpu_telemetry_serialization() {
        let gpu = sample_gpu();
        let json = serde_json::to_string(&gpu).unwrap();
        let parsed: GpuTelemetry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.utilization_pct, 75.0);
        assert_eq!(parsed.memory_used_mb, 6000);
    }

    #[test]
    fn fleet_stats_empty() {
        let tracker = ConcurrentHeartbeatTracker::default();
        let stats = tracker.fleet_stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.total_gpus, 0);
    }
}
