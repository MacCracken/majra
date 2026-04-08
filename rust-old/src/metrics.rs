//! Observability hooks for majra primitives.
//!
//! The [`MajraMetrics`] trait provides a no-op default implementation.
//! Consumers wire it to their preferred metrics backend (Prometheus, OpenTelemetry, etc.).

/// Trait for reporting majra metrics to an external system.
///
/// All methods have default no-op implementations, so consumers only need
/// to override the metrics they care about.
///
/// # Example
///
/// ```rust
/// use majra::metrics::MajraMetrics;
///
/// struct PrometheusMetrics;
///
/// impl MajraMetrics for PrometheusMetrics {
///     fn queue_enqueued(&self, queue_name: &str, priority: u8) {
///         // prometheus::QUEUE_ENQUEUED.with_label_values(&[queue_name, &priority.to_string()]).inc();
///     }
/// }
/// ```
pub trait MajraMetrics: Send + Sync {
    // --- Queue ---

    /// A job was enqueued.
    fn queue_enqueued(&self, _queue_name: &str, _priority: u8) {}

    /// A job was dequeued and started running.
    fn queue_dequeued(&self, _queue_name: &str, _priority: u8) {}

    /// A job reached a terminal state.
    fn queue_state_changed(&self, _queue_name: &str, _from: &str, _to: &str) {}

    /// Current queue depth (queued items).
    fn queue_depth(&self, _queue_name: &str, _depth: usize) {}

    /// Current running count.
    fn queue_running(&self, _queue_name: &str, _count: usize) {}

    // --- PubSub ---

    /// A message was published.
    fn pubsub_published(&self, _topic: &str, _subscriber_count: usize) {}

    /// A message was dropped due to backpressure.
    fn pubsub_dropped(&self, _topic: &str) {}

    // --- Heartbeat ---

    /// A node transitioned status.
    fn heartbeat_transition(&self, _node_id: &str, _from: &str, _to: &str) {}

    /// A node was evicted.
    fn heartbeat_evicted(&self, _node_id: &str) {}

    // --- Rate limiter ---

    /// A request was allowed.
    fn ratelimit_allowed(&self, _key: &str) {}

    /// A request was rejected.
    fn ratelimit_rejected(&self, _key: &str) {}

    /// Keys were evicted.
    fn ratelimit_evicted(&self, _count: usize) {}

    // --- Relay ---

    /// A message was sent.
    fn relay_sent(&self, _from: &str, _seq: u64) {}

    /// A message was received (passed dedup).
    fn relay_received(&self, _from: &str, _seq: u64) {}

    /// A duplicate was dropped.
    fn relay_duplicate_dropped(&self, _from: &str, _seq: u64) {}

    // --- Barrier ---

    /// A participant arrived at a barrier.
    fn barrier_arrived(&self, _barrier_name: &str, _arrived: usize, _expected: usize) {}

    /// A barrier was released.
    fn barrier_released(&self, _barrier_name: &str) {}

    // --- Workflow DAG ---

    /// A workflow run was started.
    fn workflow_run_started(&self, _workflow_id: &str) {}

    /// A workflow run completed successfully.
    fn workflow_run_completed(&self, _workflow_id: &str, _duration_ms: u64) {}

    /// A workflow run failed.
    fn workflow_run_failed(&self, _workflow_id: &str, _duration_ms: u64) {}

    /// A workflow step began executing.
    fn workflow_step_started(&self, _workflow_id: &str, _step_id: &str) {}

    /// A workflow step reached a terminal status.
    fn workflow_step_finished(
        &self,
        _workflow_id: &str,
        _step_id: &str,
        _status: &str,
        _duration_ms: u64,
    ) {
    }
}

/// No-op metrics implementation (default).
pub struct NoopMetrics;

impl MajraMetrics for NoopMetrics {}

// ---------------------------------------------------------------------------
// Namespaced metrics (multi-tenant)
// ---------------------------------------------------------------------------

/// Wraps a [`MajraMetrics`] implementation with a tenant namespace prefix.
///
/// All queue names, topics, and workflow IDs are prefixed with the namespace
/// before being passed to the inner metrics backend. This provides per-tenant
/// metric partitioning without requiring the backend to be tenant-aware.
///
/// # Example
///
/// ```rust
/// use majra::metrics::{NamespacedMetrics, NoopMetrics};
/// use std::sync::Arc;
///
/// let inner = Arc::new(NoopMetrics);
/// let tenant_metrics = NamespacedMetrics::new("tenant-42", inner);
/// // tenant_metrics.queue_enqueued("jobs", 3) → inner.queue_enqueued("tenant-42:jobs", 3)
/// ```
pub struct NamespacedMetrics {
    prefix: String,
    inner: std::sync::Arc<dyn MajraMetrics>,
}

impl NamespacedMetrics {
    /// Create namespaced metrics with the given prefix.
    pub fn new(prefix: impl Into<String>, inner: std::sync::Arc<dyn MajraMetrics>) -> Self {
        Self {
            prefix: prefix.into(),
            inner,
        }
    }

    #[inline]
    fn ns(&self, name: &str) -> String {
        format!("{}:{name}", self.prefix)
    }
}

impl MajraMetrics for NamespacedMetrics {
    fn queue_enqueued(&self, queue_name: &str, priority: u8) {
        self.inner.queue_enqueued(&self.ns(queue_name), priority);
    }
    fn queue_dequeued(&self, queue_name: &str, priority: u8) {
        self.inner.queue_dequeued(&self.ns(queue_name), priority);
    }
    fn queue_state_changed(&self, queue_name: &str, from: &str, to: &str) {
        self.inner
            .queue_state_changed(&self.ns(queue_name), from, to);
    }
    fn queue_depth(&self, queue_name: &str, depth: usize) {
        self.inner.queue_depth(&self.ns(queue_name), depth);
    }
    fn queue_running(&self, queue_name: &str, count: usize) {
        self.inner.queue_running(&self.ns(queue_name), count);
    }
    fn pubsub_published(&self, topic: &str, subscriber_count: usize) {
        self.inner
            .pubsub_published(&self.ns(topic), subscriber_count);
    }
    fn pubsub_dropped(&self, topic: &str) {
        self.inner.pubsub_dropped(&self.ns(topic));
    }
    fn heartbeat_transition(&self, node_id: &str, from: &str, to: &str) {
        self.inner.heartbeat_transition(&self.ns(node_id), from, to);
    }
    fn heartbeat_evicted(&self, node_id: &str) {
        self.inner.heartbeat_evicted(&self.ns(node_id));
    }
    fn ratelimit_allowed(&self, key: &str) {
        self.inner.ratelimit_allowed(&self.ns(key));
    }
    fn ratelimit_rejected(&self, key: &str) {
        self.inner.ratelimit_rejected(&self.ns(key));
    }
    fn ratelimit_evicted(&self, count: usize) {
        self.inner.ratelimit_evicted(count);
    }
    fn relay_sent(&self, from: &str, seq: u64) {
        self.inner.relay_sent(&self.ns(from), seq);
    }
    fn relay_received(&self, from: &str, seq: u64) {
        self.inner.relay_received(&self.ns(from), seq);
    }
    fn relay_duplicate_dropped(&self, from: &str, seq: u64) {
        self.inner.relay_duplicate_dropped(&self.ns(from), seq);
    }
    fn barrier_arrived(&self, barrier_name: &str, arrived: usize, expected: usize) {
        self.inner
            .barrier_arrived(&self.ns(barrier_name), arrived, expected);
    }
    fn barrier_released(&self, barrier_name: &str) {
        self.inner.barrier_released(&self.ns(barrier_name));
    }
    fn workflow_run_started(&self, workflow_id: &str) {
        self.inner.workflow_run_started(&self.ns(workflow_id));
    }
    fn workflow_run_completed(&self, workflow_id: &str, duration_ms: u64) {
        self.inner
            .workflow_run_completed(&self.ns(workflow_id), duration_ms);
    }
    fn workflow_run_failed(&self, workflow_id: &str, duration_ms: u64) {
        self.inner
            .workflow_run_failed(&self.ns(workflow_id), duration_ms);
    }
    fn workflow_step_started(&self, workflow_id: &str, step_id: &str) {
        self.inner
            .workflow_step_started(&self.ns(workflow_id), step_id);
    }
    fn workflow_step_finished(
        &self,
        workflow_id: &str,
        step_id: &str,
        status: &str,
        duration_ms: u64,
    ) {
        self.inner
            .workflow_step_finished(&self.ns(workflow_id), step_id, status, duration_ms);
    }
}

// ---------------------------------------------------------------------------
// Prometheus implementation
// ---------------------------------------------------------------------------

/// Built-in Prometheus metrics exporter.
///
/// Registers counters and gauges with the provided [`prometheus::Registry`]
/// (or the default global registry). Wire this into `ManagedQueue::with_metrics()`
/// or any component that accepts `Arc<dyn MajraMetrics>`.
///
/// # Example
///
/// ```rust,no_run
/// use majra::metrics::PrometheusMetrics;
///
/// let registry = prometheus::Registry::new();
/// let metrics = PrometheusMetrics::new(&registry).expect("register metrics");
/// // Pass Arc::new(metrics) to ManagedQueue::with_metrics(), etc.
/// ```
#[cfg(feature = "prometheus")]
pub struct PrometheusMetrics {
    // Queue
    queue_enqueued: prometheus::IntCounterVec,
    queue_dequeued: prometheus::IntCounterVec,
    queue_state_changed: prometheus::IntCounterVec,
    queue_depth: prometheus::IntGaugeVec,
    queue_running: prometheus::IntGaugeVec,
    // PubSub
    pubsub_published: prometheus::IntCounterVec,
    pubsub_dropped: prometheus::IntCounterVec,
    // Heartbeat
    heartbeat_transitions: prometheus::IntCounterVec,
    heartbeat_evictions: prometheus::IntCounter,
    // Rate limiter
    ratelimit_allowed: prometheus::IntCounter,
    ratelimit_rejected: prometheus::IntCounter,
    ratelimit_evicted: prometheus::IntCounter,
    // Relay
    relay_sent: prometheus::IntCounter,
    relay_received: prometheus::IntCounter,
    relay_duplicates_dropped: prometheus::IntCounter,
    // Barrier
    barrier_arrivals: prometheus::IntCounterVec,
    barrier_releases: prometheus::IntCounter,
    // Workflow
    workflow_runs_started: prometheus::IntCounter,
    workflow_runs_completed: prometheus::IntCounter,
    workflow_runs_failed: prometheus::IntCounter,
    workflow_run_duration_ms: prometheus::HistogramVec,
}

#[cfg(feature = "prometheus")]
impl PrometheusMetrics {
    /// Create and register all majra metrics with the given registry.
    pub fn new(registry: &prometheus::Registry) -> Result<Self, prometheus::Error> {
        let queue_enqueued = prometheus::IntCounterVec::new(
            prometheus::Opts::new("majra_queue_enqueued_total", "Jobs enqueued"),
            &["queue", "priority"],
        )?;
        let queue_dequeued = prometheus::IntCounterVec::new(
            prometheus::Opts::new("majra_queue_dequeued_total", "Jobs dequeued"),
            &["queue", "priority"],
        )?;
        let queue_state_changed = prometheus::IntCounterVec::new(
            prometheus::Opts::new(
                "majra_queue_state_transitions_total",
                "Job state transitions",
            ),
            &["queue", "from", "to"],
        )?;
        let queue_depth = prometheus::IntGaugeVec::new(
            prometheus::Opts::new("majra_queue_depth", "Current queued items"),
            &["queue"],
        )?;
        let queue_running = prometheus::IntGaugeVec::new(
            prometheus::Opts::new("majra_queue_running", "Currently running jobs"),
            &["queue"],
        )?;
        let pubsub_published = prometheus::IntCounterVec::new(
            prometheus::Opts::new("majra_pubsub_published_total", "Messages published"),
            &["topic"],
        )?;
        let pubsub_dropped = prometheus::IntCounterVec::new(
            prometheus::Opts::new(
                "majra_pubsub_dropped_total",
                "Messages dropped (backpressure)",
            ),
            &["topic"],
        )?;
        let heartbeat_transitions = prometheus::IntCounterVec::new(
            prometheus::Opts::new(
                "majra_heartbeat_transitions_total",
                "Node status transitions",
            ),
            &["from", "to"],
        )?;
        let heartbeat_evictions =
            prometheus::IntCounter::new("majra_heartbeat_evictions_total", "Nodes evicted")?;
        let ratelimit_allowed =
            prometheus::IntCounter::new("majra_ratelimit_allowed_total", "Requests allowed")?;
        let ratelimit_rejected =
            prometheus::IntCounter::new("majra_ratelimit_rejected_total", "Requests rejected")?;
        let ratelimit_evicted =
            prometheus::IntCounter::new("majra_ratelimit_evicted_total", "Keys evicted")?;
        let relay_sent =
            prometheus::IntCounter::new("majra_relay_sent_total", "Relay messages sent")?;
        let relay_received = prometheus::IntCounter::new(
            "majra_relay_received_total",
            "Relay messages received (deduped)",
        )?;
        let relay_duplicates_dropped = prometheus::IntCounter::new(
            "majra_relay_duplicates_dropped_total",
            "Relay duplicates dropped",
        )?;
        let barrier_arrivals = prometheus::IntCounterVec::new(
            prometheus::Opts::new(
                "majra_barrier_arrivals_total",
                "Barrier participant arrivals",
            ),
            &["barrier"],
        )?;
        let barrier_releases =
            prometheus::IntCounter::new("majra_barrier_releases_total", "Barriers released")?;
        let workflow_runs_started = prometheus::IntCounter::new(
            "majra_workflow_runs_started_total",
            "Workflow runs started",
        )?;
        let workflow_runs_completed = prometheus::IntCounter::new(
            "majra_workflow_runs_completed_total",
            "Workflow runs completed",
        )?;
        let workflow_runs_failed = prometheus::IntCounter::new(
            "majra_workflow_runs_failed_total",
            "Workflow runs failed",
        )?;
        let workflow_run_duration_ms = prometheus::HistogramVec::new(
            prometheus::HistogramOpts::new(
                "majra_workflow_run_duration_ms",
                "Workflow run duration in milliseconds",
            )
            .buckets(vec![
                10.0, 50.0, 100.0, 500.0, 1000.0, 5000.0, 10000.0, 60000.0, 300000.0,
            ]),
            &["workflow", "status"],
        )?;

        registry.register(Box::new(queue_enqueued.clone()))?;
        registry.register(Box::new(queue_dequeued.clone()))?;
        registry.register(Box::new(queue_state_changed.clone()))?;
        registry.register(Box::new(queue_depth.clone()))?;
        registry.register(Box::new(queue_running.clone()))?;
        registry.register(Box::new(pubsub_published.clone()))?;
        registry.register(Box::new(pubsub_dropped.clone()))?;
        registry.register(Box::new(heartbeat_transitions.clone()))?;
        registry.register(Box::new(heartbeat_evictions.clone()))?;
        registry.register(Box::new(ratelimit_allowed.clone()))?;
        registry.register(Box::new(ratelimit_rejected.clone()))?;
        registry.register(Box::new(ratelimit_evicted.clone()))?;
        registry.register(Box::new(relay_sent.clone()))?;
        registry.register(Box::new(relay_received.clone()))?;
        registry.register(Box::new(relay_duplicates_dropped.clone()))?;
        registry.register(Box::new(barrier_arrivals.clone()))?;
        registry.register(Box::new(barrier_releases.clone()))?;
        registry.register(Box::new(workflow_runs_started.clone()))?;
        registry.register(Box::new(workflow_runs_completed.clone()))?;
        registry.register(Box::new(workflow_runs_failed.clone()))?;
        registry.register(Box::new(workflow_run_duration_ms.clone()))?;

        Ok(Self {
            queue_enqueued,
            queue_dequeued,
            queue_state_changed,
            queue_depth,
            queue_running,
            pubsub_published,
            pubsub_dropped,
            heartbeat_transitions,
            heartbeat_evictions,
            ratelimit_allowed,
            ratelimit_rejected,
            ratelimit_evicted,
            relay_sent,
            relay_received,
            relay_duplicates_dropped,
            barrier_arrivals,
            barrier_releases,
            workflow_runs_started,
            workflow_runs_completed,
            workflow_runs_failed,
            workflow_run_duration_ms,
        })
    }
}

#[cfg(feature = "prometheus")]
impl MajraMetrics for PrometheusMetrics {
    fn queue_enqueued(&self, queue_name: &str, priority: u8) {
        self.queue_enqueued
            .with_label_values(&[queue_name, &priority.to_string()])
            .inc();
    }

    fn queue_dequeued(&self, queue_name: &str, priority: u8) {
        self.queue_dequeued
            .with_label_values(&[queue_name, &priority.to_string()])
            .inc();
    }

    fn queue_state_changed(&self, queue_name: &str, from: &str, to: &str) {
        self.queue_state_changed
            .with_label_values(&[queue_name, from, to])
            .inc();
    }

    fn queue_depth(&self, queue_name: &str, depth: usize) {
        self.queue_depth
            .with_label_values(&[queue_name])
            .set(depth as i64);
    }

    fn queue_running(&self, queue_name: &str, count: usize) {
        self.queue_running
            .with_label_values(&[queue_name])
            .set(count as i64);
    }

    fn pubsub_published(&self, topic: &str, _subscriber_count: usize) {
        self.pubsub_published.with_label_values(&[topic]).inc();
    }

    fn pubsub_dropped(&self, topic: &str) {
        self.pubsub_dropped.with_label_values(&[topic]).inc();
    }

    fn heartbeat_transition(&self, _node_id: &str, from: &str, to: &str) {
        self.heartbeat_transitions
            .with_label_values(&[from, to])
            .inc();
    }

    fn heartbeat_evicted(&self, _node_id: &str) {
        self.heartbeat_evictions.inc();
    }

    fn ratelimit_allowed(&self, _key: &str) {
        self.ratelimit_allowed.inc();
    }

    fn ratelimit_rejected(&self, _key: &str) {
        self.ratelimit_rejected.inc();
    }

    fn ratelimit_evicted(&self, count: usize) {
        self.ratelimit_evicted.inc_by(count as u64);
    }

    fn relay_sent(&self, _from: &str, _seq: u64) {
        self.relay_sent.inc();
    }

    fn relay_received(&self, _from: &str, _seq: u64) {
        self.relay_received.inc();
    }

    fn relay_duplicate_dropped(&self, _from: &str, _seq: u64) {
        self.relay_duplicates_dropped.inc();
    }

    fn barrier_arrived(&self, barrier_name: &str, _arrived: usize, _expected: usize) {
        self.barrier_arrivals
            .with_label_values(&[barrier_name])
            .inc();
    }

    fn barrier_released(&self, _barrier_name: &str) {
        self.barrier_releases.inc();
    }

    fn workflow_run_started(&self, _workflow_id: &str) {
        self.workflow_runs_started.inc();
    }

    fn workflow_run_completed(&self, workflow_id: &str, duration_ms: u64) {
        self.workflow_runs_completed.inc();
        self.workflow_run_duration_ms
            .with_label_values(&[workflow_id, "completed"])
            .observe(duration_ms as f64);
    }

    fn workflow_run_failed(&self, workflow_id: &str, duration_ms: u64) {
        self.workflow_runs_failed.inc();
        self.workflow_run_duration_ms
            .with_label_values(&[workflow_id, "failed"])
            .observe(duration_ms as f64);
    }

    fn workflow_step_started(&self, _workflow_id: &str, _step_id: &str) {}

    fn workflow_step_finished(
        &self,
        _workflow_id: &str,
        _step_id: &str,
        _status: &str,
        _duration_ms: u64,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_metrics_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoopMetrics>();
    }

    #[test]
    fn trait_object_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn MajraMetrics>();
    }

    #[test]
    fn noop_methods_callable() {
        let m = NoopMetrics;
        m.queue_enqueued("test", 2);
        m.queue_dequeued("test", 2);
        m.queue_state_changed("test", "queued", "running");
        m.queue_depth("test", 5);
        m.queue_running("test", 1);
        m.pubsub_published("topic", 3);
        m.pubsub_dropped("topic");
        m.heartbeat_transition("node-1", "online", "suspect");
        m.heartbeat_evicted("node-1");
        m.ratelimit_allowed("ip");
        m.ratelimit_rejected("ip");
        m.ratelimit_evicted(5);
        m.relay_sent("node-1", 1);
        m.relay_received("node-1", 1);
        m.relay_duplicate_dropped("node-1", 1);
        m.barrier_arrived("sync-1", 2, 3);
        m.barrier_released("sync-1");
    }

    struct CountingMetrics {
        enqueued: std::sync::atomic::AtomicU64,
    }

    impl MajraMetrics for CountingMetrics {
        fn queue_enqueued(&self, _queue_name: &str, _priority: u8) {
            self.enqueued
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    #[test]
    fn custom_metrics_impl() {
        let m = CountingMetrics {
            enqueued: std::sync::atomic::AtomicU64::new(0),
        };
        m.queue_enqueued("test", 2);
        m.queue_enqueued("test", 3);
        assert_eq!(m.enqueued.load(std::sync::atomic::Ordering::Relaxed), 2);
    }

    #[cfg(feature = "prometheus")]
    mod prometheus_tests {
        use super::super::*;

        #[test]
        fn prometheus_metrics_register() {
            let registry = prometheus::Registry::new();
            let m = PrometheusMetrics::new(&registry).expect("should register");

            // Verify it's Send + Sync (required for Arc<dyn MajraMetrics>).
            fn assert_send_sync<T: Send + Sync>() {}
            assert_send_sync::<PrometheusMetrics>();

            // Exercise all methods.
            m.queue_enqueued("test", 2);
            m.queue_dequeued("test", 2);
            m.queue_state_changed("test", "queued", "running");
            m.queue_depth("test", 5);
            m.queue_running("test", 1);
            m.pubsub_published("topic", 3);
            m.pubsub_dropped("topic");
            m.heartbeat_transition("node-1", "online", "suspect");
            m.heartbeat_evicted("node-1");
            m.ratelimit_allowed("ip");
            m.ratelimit_rejected("ip");
            m.ratelimit_evicted(5);
            m.relay_sent("node-1", 1);
            m.relay_received("node-1", 1);
            m.relay_duplicate_dropped("node-1", 1);
            m.barrier_arrived("sync-1", 2, 3);
            m.barrier_released("sync-1");
            m.workflow_run_started("wf-1");
            m.workflow_run_completed("wf-1", 1500);
            m.workflow_run_failed("wf-1", 3000);
        }

        #[test]
        fn prometheus_counter_values() {
            let registry = prometheus::Registry::new();
            let m = PrometheusMetrics::new(&registry).unwrap();

            m.queue_enqueued("managed", 2);
            m.queue_enqueued("managed", 2);
            m.queue_enqueued("managed", 4);

            // Check counter value.
            let families = registry.gather();
            let enqueued = families
                .iter()
                .find(|f| f.name() == "majra_queue_enqueued_total")
                .expect("enqueued metric should exist");

            let total: f64 = enqueued
                .get_metric()
                .iter()
                .map(|m| m.get_counter().value())
                .sum();
            assert_eq!(total, 3.0);
        }

        #[test]
        fn prometheus_duplicate_register_fails() {
            let registry = prometheus::Registry::new();
            let _m1 = PrometheusMetrics::new(&registry).unwrap();
            // Second registration should fail.
            assert!(PrometheusMetrics::new(&registry).is_err());
        }
    }
}
