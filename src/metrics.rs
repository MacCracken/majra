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
}

/// No-op metrics implementation (default).
pub struct NoopMetrics;

impl MajraMetrics for NoopMetrics {}

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
        assert_eq!(
            m.enqueued.load(std::sync::atomic::Ordering::Relaxed),
            2
        );
    }
}
