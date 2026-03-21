//! Multi-tier priority queue with DAG dependency scheduling.
//!
//! The queue supports two modes:
//! 1. **Priority FIFO** — 5-tier priority levels, dequeue pops from highest tier first.
//! 2. **DAG scheduling** — tasks respect a dependency graph; `ready_tasks()` returns
//!    only those whose predecessors have completed.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, Notify};
use tracing::instrument;
use uuid::Uuid;

use crate::error::MajraError;

/// Task priority tiers (highest = 4, lowest = 0).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[non_exhaustive]
#[repr(u8)]
pub enum Priority {
    Background = 0,
    Low = 1,
    #[default]
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// A unique task identifier.
pub type TaskId = Uuid;

/// A schedulable work item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem<T> {
    pub id: TaskId,
    pub priority: Priority,
    pub payload: T,
}

impl<T> QueueItem<T> {
    pub fn new(priority: Priority, payload: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            priority,
            payload,
        }
    }
}

/// A DAG specification: nodes and their dependency edges.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dag {
    /// Node key → list of keys this node depends on.
    pub edges: HashMap<String, Vec<String>>,
}

/// Multi-tier priority queue.
#[must_use]
pub struct PriorityQueue<T> {
    tiers: [VecDeque<QueueItem<T>>; 5],
}

impl<T> PriorityQueue<T> {
    pub fn new() -> Self {
        Self {
            tiers: Default::default(),
        }
    }

    /// Push an item into its priority tier.
    pub fn enqueue(&mut self, item: QueueItem<T>) {
        self.tiers[item.priority as usize].push_back(item);
    }

    /// Pop the highest-priority item.
    pub fn dequeue(&mut self) -> Option<QueueItem<T>> {
        for tier in self.tiers.iter_mut().rev() {
            if let Some(item) = tier.pop_front() {
                return Some(item);
            }
        }
        None
    }

    /// Total items across all tiers.
    pub fn len(&self) -> usize {
        self.tiers.iter().map(VecDeque::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.tiers.iter().all(VecDeque::is_empty)
    }
}

impl<T> Default for PriorityQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// DAG-aware scheduler that tracks dependencies and emits ready items.
pub struct DagScheduler {
    /// Reverse edges: node → set of nodes it depends on.
    dependencies: HashMap<String, HashSet<String>>,
}

impl DagScheduler {
    /// Load a DAG, validating that it is acyclic.
    pub fn new(dag: &Dag) -> crate::error::Result<Self> {
        // Validate acyclicity via topological sort.
        Self::topological_sort(dag)?;

        let mut dependencies: HashMap<String, HashSet<String>> = HashMap::new();

        for (node, deps) in &dag.edges {
            dependencies
                .entry(node.clone())
                .or_default()
                .extend(deps.iter().cloned());

            // Ensure every referenced dep node exists in the map.
            for dep in deps {
                dependencies.entry(dep.clone()).or_default();
            }
        }

        Ok(Self { dependencies })
    }

    /// Return node keys whose dependencies have all been completed.
    pub fn ready(&self, completed: &HashSet<String>) -> Vec<String> {
        let mut ready = Vec::new();
        for (node, deps) in &self.dependencies {
            if !completed.contains(node) && deps.iter().all(|d| completed.contains(d)) {
                ready.push(node.clone());
            }
        }
        ready.sort();
        ready
    }

    /// Kahn's algorithm — returns topological order or error on cycle.
    pub fn topological_sort(dag: &Dag) -> crate::error::Result<Vec<String>> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

        // Collect all nodes.
        for (node, deps) in &dag.edges {
            in_degree.entry(node.as_str()).or_insert(0);
            adjacency.entry(node.as_str()).or_default();
            for dep in deps {
                in_degree.entry(dep.as_str()).or_insert(0);
                adjacency
                    .entry(dep.as_str())
                    .or_default()
                    .push(node.as_str());
                *in_degree.entry(node.as_str()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(&k, _)| k)
            .collect();

        // Deterministic output.
        let mut queue_sorted: Vec<&str> = queue.drain(..).collect();
        queue_sorted.sort();
        queue.extend(queue_sorted);

        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node.to_string());
            if let Some(neighbors) = adjacency.get(node) {
                let mut next = Vec::new();
                for &n in neighbors {
                    let deg = in_degree.get_mut(n).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        next.push(n);
                    }
                }
                next.sort();
                queue.extend(next);
            }
        }

        if order.len() != in_degree.len() {
            return Err(MajraError::DagCycle(
                "dependency graph contains a cycle".into(),
            ));
        }

        Ok(order)
    }
}

// ---------------------------------------------------------------------------
// Thread-safe variant
// ---------------------------------------------------------------------------

/// Thread-safe priority queue with async-aware locking.
///
/// Wraps [`PriorityQueue`] in a `tokio::sync::Mutex`. Supports blocking
/// dequeue via [`dequeue_wait`](ConcurrentPriorityQueue::dequeue_wait).
#[must_use]
pub struct ConcurrentPriorityQueue<T> {
    inner: tokio::sync::Mutex<PriorityQueue<T>>,
    notify: tokio::sync::Notify,
}

impl<T: Send> ConcurrentPriorityQueue<T> {
    pub fn new() -> Self {
        Self {
            inner: tokio::sync::Mutex::new(PriorityQueue::new()),
            notify: tokio::sync::Notify::new(),
        }
    }

    /// Push an item, waking any blocked `dequeue_wait` callers.
    pub async fn enqueue(&self, item: QueueItem<T>) {
        self.inner.lock().await.enqueue(item);
        self.notify.notify_one();
    }

    /// Pop the highest-priority item, or `None` if empty.
    pub async fn dequeue(&self) -> Option<QueueItem<T>> {
        self.inner.lock().await.dequeue()
    }

    /// Block until an item is available, then pop it.
    pub async fn dequeue_wait(&self) -> QueueItem<T> {
        loop {
            {
                let mut q = self.inner.lock().await;
                if let Some(item) = q.dequeue() {
                    return item;
                }
            }
            self.notify.notified().await;
        }
    }

    /// Total items across all tiers.
    pub async fn len(&self) -> usize {
        self.inner.lock().await.len()
    }

    pub async fn is_empty(&self) -> bool {
        self.inner.lock().await.is_empty()
    }
}

impl<T: Send> Default for ConcurrentPriorityQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Managed queue — resource-aware, lifecycle-tracked, concurrent
// ---------------------------------------------------------------------------

/// Optional resource requirements for a queue item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceReq {
    pub gpu_count: u32,
    pub vram_mb: u64,
}

/// Available resource pool for dequeue filtering.
#[derive(Debug, Clone, Default)]
pub struct ResourcePool {
    pub gpu_count: u32,
    pub vram_mb: u64,
}

impl ResourcePool {
    /// Check whether the pool can satisfy the given requirements.
    pub fn satisfies(&self, req: &ResourceReq) -> bool {
        self.gpu_count >= req.gpu_count && self.vram_mb >= req.vram_mb
    }
}

/// Job lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JobState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl JobState {
    /// Whether this is a terminal state.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// A managed queue item with resource requirements and lifecycle tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedItem<T> {
    pub id: TaskId,
    pub priority: Priority,
    pub payload: T,
    pub resource_req: Option<ResourceReq>,
    pub state: JobState,
    #[serde(skip)]
    pub enqueued_at: Option<Instant>,
    #[serde(skip)]
    pub started_at: Option<Instant>,
    #[serde(skip)]
    pub finished_at: Option<Instant>,
}

/// Events emitted by the managed queue.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum QueueEvent {
    Enqueued { id: TaskId },
    Dequeued { id: TaskId },
    StateChanged { id: TaskId, from: JobState, to: JobState },
}

/// Configuration for the managed queue.
#[derive(Debug, Clone)]
pub struct ManagedQueueConfig {
    /// Max concurrent running jobs (0 = unlimited).
    pub max_concurrency: usize,
    /// TTL for completed/failed/cancelled jobs before eviction.
    pub finished_ttl: Duration,
}

impl Default for ManagedQueueConfig {
    fn default() -> Self {
        Self {
            max_concurrency: 0,
            finished_ttl: Duration::from_secs(3600),
        }
    }
}

/// Thread-safe, resource-aware priority queue with job lifecycle management.
///
/// Combines priority scheduling, resource-aware dequeue, max concurrency
/// enforcement, and lifecycle state tracking in one type.
pub struct ManagedQueue<T: Send> {
    tiers: tokio::sync::Mutex<[VecDeque<TaskId>; 5]>,
    jobs: DashMap<TaskId, ManagedItem<T>>,
    config: ManagedQueueConfig,
    running_count: AtomicUsize,
    notify: Notify,
    events_tx: broadcast::Sender<QueueEvent>,
    #[cfg(feature = "sqlite")]
    backend: Option<std::sync::Arc<persistence::SqliteBackend>>,
}

impl<T: Send + Clone + Serialize + 'static> ManagedQueue<T> {
    pub fn new(config: ManagedQueueConfig) -> Self {
        let (events_tx, _) = broadcast::channel(256);
        Self {
            tiers: tokio::sync::Mutex::new(Default::default()),
            jobs: DashMap::new(),
            config,
            running_count: AtomicUsize::new(0),
            notify: Notify::new(),
            events_tx,
            #[cfg(feature = "sqlite")]
            backend: None,
        }
    }

    /// Subscribe to queue lifecycle events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<QueueEvent> {
        self.events_tx.subscribe()
    }

    /// Enqueue a job. Returns the assigned task ID.
    #[instrument(skip(self, payload, resource_req), fields(priority = ?priority))]
    pub async fn enqueue(
        &self,
        priority: Priority,
        payload: T,
        resource_req: Option<ResourceReq>,
    ) -> TaskId {
        let id = Uuid::new_v4();
        let item = ManagedItem {
            id,
            priority,
            payload,
            resource_req,
            state: JobState::Queued,
            enqueued_at: Some(Instant::now()),
            started_at: None,
            finished_at: None,
        };

        #[cfg(feature = "sqlite")]
        if let Some(ref backend) = self.backend {
            let _ = backend.persist(&item);
        }

        self.jobs.insert(id, item);
        self.tiers.lock().await[priority as usize].push_back(id);
        let _ = self.events_tx.send(QueueEvent::Enqueued { id });
        self.notify.notify_one();
        id
    }

    /// Dequeue the highest-priority job that fits within the given resource pool
    /// and respects max concurrency. Returns `None` if nothing eligible.
    #[instrument(skip(self, available))]
    pub async fn dequeue(&self, available: &ResourcePool) -> Option<ManagedItem<T>> {
        // Check concurrency limit.
        if self.config.max_concurrency > 0
            && self.running_count.load(Ordering::Relaxed) >= self.config.max_concurrency
        {
            return None;
        }

        let dequeued_id = {
            let mut tiers = self.tiers.lock().await;

            // Scan tiers highest-first, within each tier find first fitting item.
            let mut found = None;
            for tier in tiers.iter_mut().rev() {
                let pos = tier.iter().position(|id| {
                    if let Some(job) = self.jobs.get(id) {
                        match &job.resource_req {
                            Some(req) => available.satisfies(req),
                            None => true,
                        }
                    } else {
                        false
                    }
                });

                if let Some(idx) = pos {
                    found = Some(tier.remove(idx).unwrap());
                    break;
                }
            }
            found
        };

        let id = dequeued_id?;

        if let Some(mut job) = self.jobs.get_mut(&id) {
            job.state = JobState::Running;
            job.started_at = Some(Instant::now());
            let result = job.clone();
            drop(job);

            self.running_count.fetch_add(1, Ordering::Relaxed);

            #[cfg(feature = "sqlite")]
            if let Some(ref backend) = self.backend {
                let _ = backend.update_state(id, JobState::Running);
            }

            let _ = self.events_tx.send(QueueEvent::Dequeued { id });
            let _ = self.events_tx.send(QueueEvent::StateChanged {
                id,
                from: JobState::Queued,
                to: JobState::Running,
            });

            return Some(result);
        }

        None
    }

    /// Dequeue without resource constraints.
    pub async fn dequeue_any(&self) -> Option<ManagedItem<T>> {
        self.dequeue(&ResourcePool {
            gpu_count: u32::MAX,
            vram_mb: u64::MAX,
        })
        .await
    }

    /// Transition a job to a terminal state. Internal helper.
    #[instrument(skip(self), fields(%id, ?new_state))]
    fn finish_job(&self, id: TaskId, new_state: JobState) -> crate::error::Result<()> {
        let mut job = self
            .jobs
            .get_mut(&id)
            .ok_or_else(|| MajraError::Queue(format!("job {id} not found")))?;

        let from = job.state;
        let valid = matches!(
            (from, new_state),
            (JobState::Running, JobState::Completed)
                | (JobState::Running, JobState::Failed)
                | (JobState::Running, JobState::Cancelled)
                | (JobState::Queued, JobState::Cancelled)
        );

        if !valid {
            return Err(MajraError::InvalidStateTransition(format!(
                "{from:?} → {new_state:?}"
            )));
        }

        job.state = new_state;
        job.finished_at = Some(Instant::now());

        if from == JobState::Running {
            self.running_count.fetch_sub(1, Ordering::Relaxed);
            self.notify.notify_one();
        }

        #[cfg(feature = "sqlite")]
        if let Some(ref backend) = self.backend {
            let _ = backend.update_state(id, new_state);
        }

        let _ = self.events_tx.send(QueueEvent::StateChanged {
            id,
            from,
            to: new_state,
        });

        Ok(())
    }

    /// Mark a running job as completed.
    pub fn complete(&self, id: TaskId) -> crate::error::Result<()> {
        self.finish_job(id, JobState::Completed)
    }

    /// Mark a running job as failed.
    pub fn fail(&self, id: TaskId) -> crate::error::Result<()> {
        self.finish_job(id, JobState::Failed)
    }

    /// Cancel a job (from Queued or Running state).
    pub async fn cancel(&self, id: TaskId) -> crate::error::Result<()> {
        // Read state and priority, then drop the DashMap guard before awaiting.
        let (state, priority) = {
            let job = self
                .jobs
                .get(&id)
                .ok_or_else(|| MajraError::Queue(format!("job {id} not found")))?;
            (job.state, job.priority)
        };

        if state == JobState::Queued {
            let mut tiers = self.tiers.lock().await;
            tiers[priority as usize].retain(|tid| *tid != id);
        }

        self.finish_job(id, JobState::Cancelled)
    }

    /// Get the current state of a job.
    pub fn get_job(&self, id: &TaskId) -> Option<ManagedItem<T>> {
        self.jobs.get(id).map(|j| j.value().clone())
    }

    /// Number of currently running jobs.
    pub fn running_count(&self) -> usize {
        self.running_count.load(Ordering::Relaxed)
    }

    /// Number of tracked jobs (all states).
    pub fn job_count(&self) -> usize {
        self.jobs.len()
    }

    /// Number of queued (waiting) jobs.
    pub async fn queued_count(&self) -> usize {
        let tiers = self.tiers.lock().await;
        tiers.iter().map(VecDeque::len).sum()
    }

    /// Evict finished jobs older than the configured TTL.
    pub fn evict_expired(&self) {
        let now = Instant::now();
        let mut to_remove = Vec::new();

        for entry in self.jobs.iter() {
            if entry.state.is_terminal()
                && entry
                    .finished_at
                    .is_some_and(|t| now.duration_since(t) >= self.config.finished_ttl)
            {
                to_remove.push(*entry.key());
            }
        }

        #[cfg(feature = "sqlite")]
        if let Some(ref backend) = self.backend {
            let _ = backend.evict(&to_remove);
        }

        for id in &to_remove {
            self.jobs.remove(id);
        }
    }
}

// ---------------------------------------------------------------------------
// SQLite persistence
// ---------------------------------------------------------------------------

#[cfg(feature = "sqlite")]
pub mod persistence {
    use rusqlite::Connection;
    use serde::{de::DeserializeOwned, Serialize};

    use super::{JobState, ManagedItem, TaskId};

    /// SQLite persistence backend for [`super::ManagedQueue`].
    ///
    /// Uses WAL mode for concurrent reads and fast writes.
    pub struct SqliteBackend {
        conn: std::sync::Mutex<Connection>,
    }

    impl SqliteBackend {
        /// Open or create a WAL-mode SQLite database at the given path.
        pub fn open(path: &std::path::Path) -> crate::error::Result<Self> {
            let conn = Connection::open(path)
                .map_err(crate::error::persistence_err)?;
            Self::init(conn)
        }

        /// Open an in-memory database (for testing).
        pub fn in_memory() -> crate::error::Result<Self> {
            let conn = Connection::open_in_memory()
                .map_err(crate::error::persistence_err)?;
            Self::init(conn)
        }

        fn init(conn: Connection) -> crate::error::Result<Self> {
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 CREATE TABLE IF NOT EXISTS managed_queue (
                     id TEXT PRIMARY KEY,
                     priority INTEGER NOT NULL,
                     state TEXT NOT NULL,
                     payload TEXT NOT NULL,
                     resource_req TEXT,
                     enqueued_at INTEGER,
                     started_at INTEGER,
                     finished_at INTEGER
                 );",
            )
            .map_err(crate::error::persistence_err)?;
            Ok(Self {
                conn: std::sync::Mutex::new(conn),
            })
        }

        /// Persist a queued item.
        pub fn persist<T: Serialize>(&self, item: &ManagedItem<T>) -> crate::error::Result<()> {
            let conn = self.conn.lock().unwrap();
            let payload = serde_json::to_string(&item.payload)
                .map_err(crate::error::persistence_err)?;
            let resource_req = item
                .resource_req
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(crate::error::persistence_err)?;
            let state = serde_json::to_string(&item.state)
                .map_err(crate::error::persistence_err)?;
            conn.execute(
                "INSERT OR REPLACE INTO managed_queue (id, priority, state, payload, resource_req)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    item.id.to_string(),
                    item.priority as u8,
                    state,
                    payload,
                    resource_req,
                ],
            )
            .map_err(crate::error::persistence_err)?;
            Ok(())
        }

        /// Update the state of a persisted item.
        pub fn update_state(
            &self,
            id: TaskId,
            state: JobState,
        ) -> crate::error::Result<()> {
            let conn = self.conn.lock().unwrap();
            let state_str = serde_json::to_string(&state)
                .map_err(crate::error::persistence_err)?;
            conn.execute(
                "UPDATE managed_queue SET state = ?1 WHERE id = ?2",
                rusqlite::params![state_str, id.to_string()],
            )
            .map_err(crate::error::persistence_err)?;
            Ok(())
        }

        /// Load all non-terminal items (crash recovery).
        pub fn load_all<T: DeserializeOwned>(
            &self,
        ) -> crate::error::Result<Vec<ManagedItem<T>>> {
            let conn = self.conn.lock().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT id, priority, state, payload, resource_req FROM managed_queue
                     WHERE state IN ('\"queued\"', '\"running\"')",
                )
                .map_err(crate::error::persistence_err)?;
            let items = stmt
                .query_map([], |row| {
                    let id_str: String = row.get(0)?;
                    let priority_u8: u8 = row.get(1)?;
                    let state_str: String = row.get(2)?;
                    let payload_str: String = row.get(3)?;
                    let resource_req_str: Option<String> = row.get(4)?;
                    Ok((id_str, priority_u8, state_str, payload_str, resource_req_str))
                })
                .map_err(crate::error::persistence_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(crate::error::persistence_err)?;

            let mut result = Vec::new();
            for (id_str, priority_u8, state_str, payload_str, resource_req_str) in items {
                let id: TaskId = id_str
                    .parse()
                    .map_err(crate::error::persistence_err)?;
                let priority = match priority_u8 {
                    0 => super::Priority::Background,
                    1 => super::Priority::Low,
                    2 => super::Priority::Normal,
                    3 => super::Priority::High,
                    4 => super::Priority::Critical,
                    _ => super::Priority::Normal,
                };
                let state: JobState = serde_json::from_str(&state_str)
                    .map_err(crate::error::persistence_err)?;
                let payload: T = serde_json::from_str(&payload_str)
                    .map_err(crate::error::persistence_err)?;
                let resource_req = resource_req_str
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(crate::error::persistence_err)?;

                result.push(ManagedItem {
                    id,
                    priority,
                    payload,
                    resource_req,
                    state,
                    enqueued_at: None,
                    started_at: None,
                    finished_at: None,
                });
            }

            Ok(result)
        }

        /// Delete evicted items.
        pub fn evict(&self, ids: &[TaskId]) -> crate::error::Result<()> {
            let conn = self.conn.lock().unwrap();
            for id in ids {
                conn.execute(
                    "DELETE FROM managed_queue WHERE id = ?1",
                    rusqlite::params![id.to_string()],
                )
                .map_err(crate::error::persistence_err)?;
            }
            Ok(())
        }
    }

    impl super::ManagedQueue<serde_json::Value> {
        /// Create a managed queue with SQLite persistence.
        pub fn with_sqlite(
            config: super::ManagedQueueConfig,
            backend: std::sync::Arc<SqliteBackend>,
        ) -> Self {
            let (events_tx, _) = tokio::sync::broadcast::channel(256);
            Self {
                tiers: tokio::sync::Mutex::new(Default::default()),
                jobs: dashmap::DashMap::new(),
                config,
                running_count: std::sync::atomic::AtomicUsize::new(0),
                notify: tokio::sync::Notify::new(),
                events_tx,
                backend: Some(backend),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering() {
        let mut q = PriorityQueue::new();
        q.enqueue(QueueItem::new(Priority::Low, "low"));
        q.enqueue(QueueItem::new(Priority::Critical, "crit"));
        q.enqueue(QueueItem::new(Priority::Normal, "norm"));

        assert_eq!(q.dequeue().unwrap().payload, "crit");
        assert_eq!(q.dequeue().unwrap().payload, "norm");
        assert_eq!(q.dequeue().unwrap().payload, "low");
        assert!(q.is_empty());
    }

    #[test]
    fn fifo_within_tier() {
        let mut q = PriorityQueue::new();
        q.enqueue(QueueItem::new(Priority::Normal, "first"));
        q.enqueue(QueueItem::new(Priority::Normal, "second"));

        assert_eq!(q.dequeue().unwrap().payload, "first");
        assert_eq!(q.dequeue().unwrap().payload, "second");
    }

    #[test]
    fn dag_ready_tasks() {
        let dag = Dag {
            edges: HashMap::from([
                ("a".into(), vec![]),
                ("b".into(), vec!["a".into()]),
                ("c".into(), vec!["a".into()]),
                ("d".into(), vec!["b".into(), "c".into()]),
            ]),
        };
        let sched = DagScheduler::new(&dag).unwrap();

        let ready = sched.ready(&HashSet::new());
        assert_eq!(ready, vec!["a"]);

        let ready = sched.ready(&HashSet::from(["a".into()]));
        assert_eq!(ready, vec!["b", "c"]);

        let ready = sched.ready(&HashSet::from(["a".into(), "b".into(), "c".into()]));
        assert_eq!(ready, vec!["d"]);
    }

    #[test]
    fn dag_cycle_detected() {
        let dag = Dag {
            edges: HashMap::from([
                ("a".into(), vec!["b".into()]),
                ("b".into(), vec!["a".into()]),
            ]),
        };
        assert!(DagScheduler::new(&dag).is_err());
    }

    #[test]
    fn topological_sort_linear() {
        let dag = Dag {
            edges: HashMap::from([
                ("a".into(), vec![]),
                ("b".into(), vec!["a".into()]),
                ("c".into(), vec!["b".into()]),
            ]),
        };
        let order = DagScheduler::topological_sort(&dag).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn queue_len() {
        let mut q: PriorityQueue<&str> = PriorityQueue::new();
        assert_eq!(q.len(), 0);
        q.enqueue(QueueItem::new(Priority::High, "x"));
        assert_eq!(q.len(), 1);
    }

    // --- ConcurrentPriorityQueue ---

    #[tokio::test]
    async fn concurrent_priority_ordering() {
        let q = ConcurrentPriorityQueue::new();
        q.enqueue(QueueItem::new(Priority::Low, "low")).await;
        q.enqueue(QueueItem::new(Priority::Critical, "crit")).await;
        q.enqueue(QueueItem::new(Priority::Normal, "norm")).await;

        assert_eq!(q.dequeue().await.unwrap().payload, "crit");
        assert_eq!(q.dequeue().await.unwrap().payload, "norm");
        assert_eq!(q.dequeue().await.unwrap().payload, "low");
        assert!(q.is_empty().await);
    }

    #[tokio::test]
    async fn concurrent_dequeue_wait() {
        use std::sync::Arc;

        let q = Arc::new(ConcurrentPriorityQueue::new());
        let q2 = q.clone();

        let handle = tokio::spawn(async move {
            q2.dequeue_wait().await.payload
        });

        // Small delay to ensure the waiter is parked.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        q.enqueue(QueueItem::new(Priority::Normal, "woke")).await;

        let result = handle.await.unwrap();
        assert_eq!(result, "woke");
    }

    #[tokio::test]
    async fn concurrent_multi_producer() {
        use std::sync::Arc;

        let q = Arc::new(ConcurrentPriorityQueue::new());
        let mut handles = Vec::new();

        for i in 0..4 {
            let q2 = q.clone();
            handles.push(tokio::spawn(async move {
                for j in 0..25 {
                    q2.enqueue(QueueItem::new(Priority::Normal, i * 25 + j)).await;
                }
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        assert_eq!(q.len().await, 100);

        let mut count = 0;
        while q.dequeue().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 100);
    }

    // --- ManagedQueue ---

    #[tokio::test]
    async fn managed_enqueue_dequeue() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        let id = q.enqueue(Priority::Normal, "job-1".to_string(), None).await;

        let item = q.dequeue_any().await.unwrap();
        assert_eq!(item.id, id);
        assert_eq!(item.state, JobState::Running);
        assert_eq!(q.running_count(), 1);
    }

    #[tokio::test]
    async fn managed_priority_ordering() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        q.enqueue(Priority::Low, "low".to_string(), None).await;
        q.enqueue(Priority::Critical, "crit".to_string(), None).await;
        q.enqueue(Priority::Normal, "norm".to_string(), None).await;

        assert_eq!(q.dequeue_any().await.unwrap().payload, "crit");
        assert_eq!(q.dequeue_any().await.unwrap().payload, "norm");
        assert_eq!(q.dequeue_any().await.unwrap().payload, "low");
    }

    #[tokio::test]
    async fn managed_max_concurrency() {
        let config = ManagedQueueConfig {
            max_concurrency: 2,
            ..Default::default()
        };
        let q = ManagedQueue::new(config);
        let id1 = q.enqueue(Priority::Normal, "a".to_string(), None).await;
        q.enqueue(Priority::Normal, "b".to_string(), None).await;
        q.enqueue(Priority::Normal, "c".to_string(), None).await;

        // Dequeue 2 — should succeed.
        assert!(q.dequeue_any().await.is_some());
        assert!(q.dequeue_any().await.is_some());
        // Third blocked by concurrency limit.
        assert!(q.dequeue_any().await.is_none());
        // Complete one, third should now be available.
        q.complete(id1).unwrap();
        assert!(q.dequeue_any().await.is_some());
    }

    #[tokio::test]
    async fn managed_resource_filtering() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        q.enqueue(
            Priority::Critical,
            "big-gpu".to_string(),
            Some(ResourceReq { gpu_count: 4, vram_mb: 32000 }),
        ).await;
        q.enqueue(
            Priority::Normal,
            "small".to_string(),
            Some(ResourceReq { gpu_count: 1, vram_mb: 8000 }),
        ).await;

        // Small pool: can only fit the small job despite big one being higher priority.
        let pool = ResourcePool { gpu_count: 1, vram_mb: 8000 };
        let item = q.dequeue(&pool).await.unwrap();
        assert_eq!(item.payload, "small");
        assert_eq!(q.queued_count().await, 1);
    }

    #[tokio::test]
    async fn managed_lifecycle_transitions() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        let id = q.enqueue(Priority::Normal, "job".to_string(), None).await;

        // Can't complete a Queued job.
        assert!(q.complete(id).is_err());

        // Dequeue → Running.
        q.dequeue_any().await.unwrap();
        assert_eq!(q.get_job(&id).unwrap().state, JobState::Running);

        // Complete → Completed.
        q.complete(id).unwrap();
        assert_eq!(q.get_job(&id).unwrap().state, JobState::Completed);

        // Can't complete again.
        assert!(q.complete(id).is_err());
    }

    #[tokio::test]
    async fn managed_cancel_queued() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        let id = q.enqueue(Priority::Normal, "cancel-me".to_string(), None).await;

        q.cancel(id).await.unwrap();
        assert_eq!(q.get_job(&id).unwrap().state, JobState::Cancelled);
        assert_eq!(q.queued_count().await, 0);
    }

    #[tokio::test]
    async fn managed_cancel_running() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        let id = q.enqueue(Priority::Normal, "cancel-me".to_string(), None).await;
        q.dequeue_any().await.unwrap();

        q.cancel(id).await.unwrap();
        assert_eq!(q.get_job(&id).unwrap().state, JobState::Cancelled);
        assert_eq!(q.running_count(), 0);
    }

    #[tokio::test]
    async fn managed_fail() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        let id = q.enqueue(Priority::Normal, "fail-me".to_string(), None).await;
        q.dequeue_any().await.unwrap();

        q.fail(id).unwrap();
        assert_eq!(q.get_job(&id).unwrap().state, JobState::Failed);
        assert_eq!(q.running_count(), 0);
    }

    #[tokio::test]
    async fn managed_evict_expired() {
        let config = ManagedQueueConfig {
            max_concurrency: 0,
            finished_ttl: Duration::from_millis(10),
        };
        let q = ManagedQueue::new(config);
        let id = q.enqueue(Priority::Normal, "evict-me".to_string(), None).await;
        q.dequeue_any().await.unwrap();
        q.complete(id).unwrap();

        assert_eq!(q.job_count(), 1);
        std::thread::sleep(Duration::from_millis(15));
        q.evict_expired();
        assert_eq!(q.job_count(), 0);
    }

    #[tokio::test]
    async fn managed_events() {
        let q = ManagedQueue::new(ManagedQueueConfig::default());
        let mut rx = q.subscribe_events();

        let id = q.enqueue(Priority::Normal, "evt".to_string(), None).await;
        q.dequeue_any().await.unwrap();
        q.complete(id).unwrap();

        // Should receive: Enqueued, Dequeued, StateChanged(Queued→Running), StateChanged(Running→Completed)
        let mut event_count = 0;
        while rx.try_recv().is_ok() {
            event_count += 1;
        }
        assert_eq!(event_count, 4);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn managed_sqlite_persist_and_load() {
        use std::sync::Arc;
        let backend = Arc::new(persistence::SqliteBackend::in_memory().unwrap());

        let q = ManagedQueue::<serde_json::Value>::with_sqlite(
            ManagedQueueConfig::default(),
            backend.clone(),
        );
        let _id = q.enqueue(Priority::High, serde_json::json!("test-payload"), None).await;

        let loaded = backend.load_all::<serde_json::Value>().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].payload, serde_json::json!("test-payload"));
        assert_eq!(loaded[0].state, JobState::Queued);
    }
}
