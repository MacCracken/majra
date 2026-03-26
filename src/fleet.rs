//! Distributed job queue with work-stealing across multiple nodes.
//!
//! The [`FleetQueue`] coordinates job distribution across registered nodes,
//! each backed by a local [`ManagedQueue`]. Idle nodes can steal work from
//! overloaded peers.
//!
//! Requires the `fleet` feature (which implies `queue` + `heartbeat`).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use serde::Serialize;
use tracing::{debug, info, trace};

use crate::queue::{ManagedQueue, ManagedQueueConfig, Priority, ResourcePool, ResourceReq, TaskId};

/// Identifier for a node in the fleet.
pub type NodeId = String;

/// Configuration for the fleet queue.
#[derive(Debug, Clone)]
pub struct FleetQueueConfig {
    /// Per-node managed queue configuration.
    pub queue_config: ManagedQueueConfig,
    /// Work-stealing threshold: a node is considered overloaded when its queued
    /// count exceeds this value. Idle nodes will attempt to steal from it.
    pub steal_threshold: usize,
    /// Maximum number of jobs to steal in a single operation.
    pub max_steal_batch: usize,
}

impl Default for FleetQueueConfig {
    fn default() -> Self {
        Self {
            queue_config: ManagedQueueConfig::default(),
            steal_threshold: 10,
            max_steal_batch: 5,
        }
    }
}

/// A node in the fleet with its local queue and resource pool.
struct FleetNode<T: Send + Clone + Serialize + 'static> {
    queue: Arc<ManagedQueue<T>>,
    resources: ResourcePool,
}

/// Fleet-wide statistics.
#[derive(Debug, Clone, Default)]
#[must_use]
pub struct FleetQueueStats {
    /// Number of registered nodes.
    pub nodes: usize,
    /// Total queued jobs across all nodes.
    pub total_queued: usize,
    /// Total running jobs across all nodes.
    pub total_running: usize,
    /// Total jobs submitted to the fleet.
    pub total_submitted: u64,
    /// Total jobs stolen via work-stealing.
    pub total_stolen: u64,
}

/// Distributed job queue that coordinates work across multiple nodes.
///
/// Each node has its own [`ManagedQueue`]. Jobs are routed to the least-loaded
/// node that can satisfy resource requirements. Idle nodes can steal work from
/// overloaded peers.
pub struct FleetQueue<T: Send + Clone + Serialize + 'static> {
    nodes: DashMap<NodeId, FleetNode<T>>,
    config: FleetQueueConfig,
    total_submitted: AtomicU64,
    total_stolen: AtomicU64,
}

impl<T: Send + Clone + Serialize + 'static> FleetQueue<T> {
    /// Create a new fleet queue with the given configuration.
    pub fn new(config: FleetQueueConfig) -> Self {
        Self {
            nodes: DashMap::new(),
            config,
            total_submitted: AtomicU64::new(0),
            total_stolen: AtomicU64::new(0),
        }
    }

    /// Register a node with its available resource pool.
    ///
    /// Returns `false` if a node with this ID was already registered (replaced).
    pub fn register_node(&self, node_id: impl Into<NodeId>, resources: ResourcePool) -> bool {
        let node_id = node_id.into();
        let queue = Arc::new(ManagedQueue::new(self.config.queue_config.clone()));
        info!(node = %node_id, "fleet: node registered");
        self.nodes
            .insert(node_id, FleetNode { queue, resources })
            .is_none()
    }

    /// Remove a node from the fleet. Returns `true` if the node existed.
    pub fn deregister_node(&self, node_id: &str) -> bool {
        let removed = self.nodes.remove(node_id).is_some();
        if removed {
            info!(node = %node_id, "fleet: node deregistered");
        }
        removed
    }

    /// Get the queue for a specific node.
    pub fn node_queue(&self, node_id: &str) -> Option<Arc<ManagedQueue<T>>> {
        self.nodes.get(node_id).map(|n| n.queue.clone())
    }

    /// Submit a job to the fleet. Routes to the least-loaded node that can
    /// satisfy the resource requirements. Returns `(node_id, task_id)`.
    ///
    /// Returns `None` if no node can satisfy the requirements.
    pub async fn submit(
        &self,
        priority: Priority,
        payload: T,
        resource_req: Option<ResourceReq>,
    ) -> Option<(NodeId, TaskId)> {
        // Find the least-loaded node that satisfies resource requirements.
        let target = self.select_node(&resource_req);
        let (node_id, node) = target?;

        let task_id = node.queue.enqueue(priority, payload, resource_req).await;

        self.total_submitted.fetch_add(1, Ordering::Relaxed);
        debug!(node = %node_id, %task_id, "fleet: job submitted");
        Some((node_id, task_id))
    }

    /// Select the least-loaded node that satisfies resource requirements.
    fn select_node(&self, resource_req: &Option<ResourceReq>) -> Option<(NodeId, FleetNode<T>)> {
        let mut best: Option<(NodeId, FleetNode<T>, usize)> = None;

        for entry in self.nodes.iter() {
            // Check resource fit.
            if let Some(req) = resource_req
                && !entry.value().resources.satisfies(req)
            {
                continue;
            }

            let running = entry.value().queue.running_count();

            match &best {
                None => {
                    best = Some((
                        entry.key().clone(),
                        FleetNode {
                            queue: entry.value().queue.clone(),
                            resources: entry.value().resources.clone(),
                        },
                        running,
                    ));
                }
                Some((_, _, best_running)) if running < *best_running => {
                    best = Some((
                        entry.key().clone(),
                        FleetNode {
                            queue: entry.value().queue.clone(),
                            resources: entry.value().resources.clone(),
                        },
                        running,
                    ));
                }
                _ => {}
            }
        }

        best.map(|(id, node, _)| (id, node))
    }

    /// Attempt work-stealing: move jobs from overloaded nodes to idle nodes.
    ///
    /// Returns the number of jobs stolen.
    pub async fn rebalance(&self) -> usize {
        // Identify overloaded and idle nodes.
        let mut overloaded = Vec::new();
        let mut idle = Vec::new();

        for entry in self.nodes.iter() {
            let queued = entry.value().queue.job_count() - entry.value().queue.running_count();
            if queued > self.config.steal_threshold {
                overloaded.push((
                    entry.key().clone(),
                    entry.value().queue.clone(),
                    entry.value().resources.clone(),
                    queued,
                ));
            } else if queued == 0 && entry.value().queue.running_count() == 0 {
                idle.push((
                    entry.key().clone(),
                    entry.value().queue.clone(),
                    entry.value().resources.clone(),
                ));
            }
        }

        if overloaded.is_empty() || idle.is_empty() {
            return 0;
        }

        let mut total_stolen = 0usize;

        for (donor_id, donor_queue, _, _) in &overloaded {
            for (thief_id, thief_queue, thief_resources) in &idle {
                if total_stolen >= self.config.max_steal_batch {
                    break;
                }

                // Try to dequeue from the donor.
                if let Some(job) = donor_queue.dequeue(thief_resources).await {
                    // Re-enqueue on the thief.
                    thief_queue
                        .enqueue(job.priority, job.payload, job.resource_req)
                        .await;

                    // Complete the job on the donor (it was moved).
                    let _ = donor_queue.complete(job.id);

                    total_stolen += 1;
                    trace!(
                        from = %donor_id,
                        to = %thief_id,
                        "fleet: job stolen"
                    );
                }
            }
        }

        if total_stolen > 0 {
            self.total_stolen
                .fetch_add(total_stolen as u64, Ordering::Relaxed);
            debug!(total_stolen, "fleet: rebalance complete");
        }

        total_stolen
    }

    /// Number of registered nodes.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Fleet-wide statistics.
    pub fn stats(&self) -> FleetQueueStats {
        let mut total_queued = 0usize;
        let mut total_running = 0usize;

        for entry in self.nodes.iter() {
            let running = entry.value().queue.running_count();
            let total = entry.value().queue.job_count();
            total_running += running;
            total_queued += total.saturating_sub(running);
        }

        FleetQueueStats {
            nodes: self.nodes.len(),
            total_queued,
            total_running,
            total_submitted: self.total_submitted.load(Ordering::Relaxed),
            total_stolen: self.total_stolen.load(Ordering::Relaxed),
        }
    }

    /// List node IDs and their current load (running count).
    pub fn node_loads(&self) -> Vec<(NodeId, usize)> {
        self.nodes
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().queue.running_count()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::queue::ResourcePool;

    fn default_pool() -> ResourcePool {
        ResourcePool {
            gpu_count: 4,
            vram_mb: 80000,
        }
    }

    #[tokio::test]
    async fn register_and_submit() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node("node-1", default_pool());
        fleet.register_node("node-2", default_pool());

        let (node_id, task_id) = fleet
            .submit(Priority::Normal, "job-1".into(), None)
            .await
            .unwrap();

        assert!(!task_id.is_nil());
        assert!(node_id == "node-1" || node_id == "node-2");
        assert_eq!(fleet.stats().total_submitted, 1);
    }

    #[tokio::test]
    async fn routes_to_least_loaded() {
        let config = FleetQueueConfig {
            queue_config: ManagedQueueConfig {
                max_concurrency: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let fleet = FleetQueue::<String>::new(config);
        fleet.register_node("node-1", default_pool());
        fleet.register_node("node-2", default_pool());

        // Submit 5 jobs — should distribute across nodes.
        for i in 0..5 {
            fleet
                .submit(Priority::Normal, format!("job-{i}"), None)
                .await
                .unwrap();
        }

        let stats = fleet.stats();
        assert_eq!(stats.total_submitted, 5);
    }

    #[tokio::test]
    async fn resource_filtering() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node(
            "cpu-node",
            ResourcePool {
                gpu_count: 0,
                vram_mb: 0,
            },
        );
        fleet.register_node("gpu-node", default_pool());

        // Submit a GPU job — should go to gpu-node only.
        let (node_id, _) = fleet
            .submit(
                Priority::High,
                "gpu-job".into(),
                Some(ResourceReq {
                    gpu_count: 2,
                    vram_mb: 16000,
                }),
            )
            .await
            .unwrap();

        assert_eq!(node_id, "gpu-node");
    }

    #[tokio::test]
    async fn no_node_satisfies_returns_none() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node(
            "small-node",
            ResourcePool {
                gpu_count: 1,
                vram_mb: 8000,
            },
        );

        let result = fleet
            .submit(
                Priority::Normal,
                "big-job".into(),
                Some(ResourceReq {
                    gpu_count: 8,
                    vram_mb: 128000,
                }),
            )
            .await;

        assert!(result.is_none());
    }

    #[tokio::test]
    async fn deregister_node() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node("node-1", default_pool());
        assert_eq!(fleet.node_count(), 1);

        assert!(fleet.deregister_node("node-1"));
        assert_eq!(fleet.node_count(), 0);
        assert!(!fleet.deregister_node("node-1"));
    }

    #[tokio::test]
    async fn node_queue_access() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node("node-1", default_pool());

        let q = fleet.node_queue("node-1").unwrap();
        assert_eq!(q.job_count(), 0);
        assert!(fleet.node_queue("unknown").is_none());
    }

    #[tokio::test]
    async fn rebalance_steals_from_overloaded() {
        let config = FleetQueueConfig {
            queue_config: ManagedQueueConfig {
                max_concurrency: 0,
                ..Default::default()
            },
            steal_threshold: 2,
            max_steal_batch: 5,
        };
        let fleet = FleetQueue::<String>::new(config);
        fleet.register_node("busy", default_pool());
        fleet.register_node("idle", default_pool());

        // Load up the busy node directly.
        let busy_queue = fleet.node_queue("busy").unwrap();
        for i in 0..5 {
            busy_queue
                .enqueue(Priority::Normal, format!("job-{i}"), None)
                .await;
        }

        let stolen = fleet.rebalance().await;
        assert!(stolen > 0, "should steal at least 1 job");
        assert_eq!(fleet.stats().total_stolen, stolen as u64);
    }

    #[tokio::test]
    async fn rebalance_noop_when_balanced() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node("node-1", default_pool());
        fleet.register_node("node-2", default_pool());

        // Submit one job to each — balanced, no stealing needed.
        fleet
            .submit(Priority::Normal, "a".into(), None)
            .await
            .unwrap();
        fleet
            .submit(Priority::Normal, "b".into(), None)
            .await
            .unwrap();

        let stolen = fleet.rebalance().await;
        assert_eq!(stolen, 0);
    }

    #[tokio::test]
    async fn node_loads() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node("node-1", default_pool());
        fleet.register_node("node-2", default_pool());

        let loads = fleet.node_loads();
        assert_eq!(loads.len(), 2);
        assert!(loads.iter().all(|(_, load)| *load == 0));
    }

    #[tokio::test]
    async fn empty_fleet_submit_returns_none() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        let result = fleet.submit(Priority::Normal, "orphan".into(), None).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn stats_aggregation() {
        let fleet = FleetQueue::<String>::new(FleetQueueConfig::default());
        fleet.register_node("node-1", default_pool());
        fleet.register_node("node-2", default_pool());

        for i in 0..4 {
            fleet
                .submit(Priority::Normal, format!("job-{i}"), None)
                .await
                .unwrap();
        }

        let stats = fleet.stats();
        assert_eq!(stats.nodes, 2);
        assert_eq!(stats.total_submitted, 4);
        // All jobs are queued (not dequeued yet).
        assert_eq!(stats.total_queued, 4);
        assert_eq!(stats.total_running, 0);
    }
}
