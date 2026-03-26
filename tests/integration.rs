//! Integration tests — multi-module scenarios simulating real consumer patterns.

#![allow(unused_imports)]

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

/// Simulates an Ifran-like training job lifecycle:
/// ManagedQueue (job scheduling) + ConcurrentHeartbeatTracker (fleet health) +
/// TypedPubSub (event broadcasting) + AsyncBarrierSet (distributed sync).
#[cfg(all(feature = "queue", feature = "heartbeat", feature = "pubsub"))]
#[tokio::test]
async fn training_job_lifecycle() {
    use majra::heartbeat::{ConcurrentHeartbeatTracker, GpuTelemetry, HeartbeatConfig, Status};
    use majra::queue::{
        JobState, ManagedQueue, ManagedQueueConfig, Priority, ResourcePool, ResourceReq,
    };

    // --- Setup fleet ---
    let tracker = Arc::new(ConcurrentHeartbeatTracker::default());
    tracker.register_with_telemetry(
        "gpu-node-1",
        serde_json::json!({"role": "trainer"}),
        vec![GpuTelemetry {
            utilization_pct: 10.0,
            memory_used_mb: 1000,
            memory_total_mb: 16000,
            temperature_c: Some(45.0),
        }],
    );
    tracker.register("cpu-node-1", serde_json::json!({"role": "indexer"}));

    let stats = tracker.fleet_stats();
    assert_eq!(stats.total_nodes, 2);
    assert_eq!(stats.online, 2);
    assert_eq!(stats.total_gpus, 1);
    assert_eq!(stats.total_vram_mb, 16000);

    // --- Setup job queue ---
    let queue = ManagedQueue::new(ManagedQueueConfig {
        max_concurrency: 2,
        finished_ttl: Duration::from_secs(60),
    });

    let mut events = queue.subscribe_events();

    // Enqueue GPU training job.
    let train_id = queue
        .enqueue(
            Priority::High,
            "train-model".to_string(),
            Some(ResourceReq {
                gpu_count: 1,
                vram_mb: 8000,
            }),
        )
        .await;

    // Enqueue CPU indexing job (no GPU needed).
    let index_id = queue
        .enqueue(Priority::Normal, "index-dataset".to_string(), None)
        .await;

    assert_eq!(queue.queued_count().await, 2);

    // --- Dequeue with GPU resources ---
    let gpu_pool = ResourcePool {
        gpu_count: 1,
        vram_mb: 16000,
    };
    let job = queue.dequeue(&gpu_pool).await.unwrap();
    assert_eq!(job.payload, "train-model");
    assert_eq!(job.state, JobState::Running);
    assert_eq!(queue.running_count(), 1);

    // Dequeue the CPU job too.
    let job2 = queue.dequeue_any().await.unwrap();
    assert_eq!(job2.payload, "index-dataset");

    // Complete both.
    queue.complete(train_id).unwrap();
    queue.fail(index_id).unwrap();

    assert_eq!(queue.running_count(), 0);
    assert_eq!(queue.get(&train_id).unwrap().state, JobState::Completed);
    assert_eq!(queue.get(&index_id).unwrap().state, JobState::Failed);

    // --- Verify events were emitted ---
    let mut event_count = 0;
    while events.try_recv().is_ok() {
        event_count += 1;
    }
    assert!(event_count >= 6); // 2 enqueue + 2 dequeue + 2 state changes + 2 terminal

    // --- Heartbeat the GPU node ---
    let _ = tracker.heartbeat_with_telemetry(
        "gpu-node-1",
        vec![GpuTelemetry {
            utilization_pct: 95.0,
            memory_used_mb: 14000,
            memory_total_mb: 16000,
            temperature_c: Some(78.0),
        }],
    );
    let tel = tracker.get_gpu_telemetry("gpu-node-1").unwrap();
    assert_eq!(tel[0].utilization_pct, 95.0);
}

/// Tests TypedPubSub + fleet heartbeat interaction:
/// publish GPU events, subscribe with filters, verify delivery.
#[cfg(feature = "pubsub")]
#[tokio::test]
async fn typed_pubsub_fleet_events() {
    use majra::pubsub::{TypedPubSub, TypedPubSubConfig};

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct GpuEvent {
        node: String,
        utilization: f32,
    }

    let config = TypedPubSubConfig {
        replay_capacity: 5,
        ..Default::default()
    };
    let hub = TypedPubSub::<GpuEvent>::with_config(config);

    // Only subscribe to high-utilization events.
    let mut rx = hub.subscribe_filtered("gpu/#", |e: &GpuEvent| e.utilization > 80.0);

    hub.publish(
        "gpu/node-1",
        GpuEvent {
            node: "node-1".into(),
            utilization: 50.0,
        },
    );
    hub.publish(
        "gpu/node-2",
        GpuEvent {
            node: "node-2".into(),
            utilization: 95.0,
        },
    );

    // Only the high-utilization event should arrive.
    let msg = rx.recv().await.unwrap();
    assert_eq!(msg.payload.node, "node-2");
    assert!(rx.try_recv().is_err());
}

/// Tests async barrier with concurrent participants.
#[cfg(feature = "barrier")]
#[tokio::test]
async fn async_barrier_multi_worker() {
    use majra::barrier::AsyncBarrierSet;

    let barriers = Arc::new(AsyncBarrierSet::new());
    let workers: HashSet<String> = (0..4).map(|i| format!("worker-{i}")).collect();
    barriers.create("epoch-sync", workers.clone());

    let mut handles = Vec::new();
    for name in &workers {
        let b = barriers.clone();
        let n = name.clone();
        handles.push(tokio::spawn(async move {
            // Simulate work.
            tokio::time::sleep(Duration::from_millis(5)).await;
            b.arrive_and_wait("epoch-sync", &n).await.unwrap();
        }));
    }

    // All workers should complete without deadlock.
    for h in handles {
        tokio::time::sleep(Duration::from_millis(1)).await;
        h.await.unwrap();
    }
}

/// Tests rate limiter stats and eviction.
#[cfg(feature = "ratelimit")]
#[test]
fn rate_limiter_lifecycle() {
    use majra::ratelimit::RateLimiter;

    let limiter = RateLimiter::new(10.0, 5);

    // Exhaust burst for two keys.
    for _ in 0..5 {
        let _ = limiter.check("client-a");
        let _ = limiter.check("client-b");
    }
    // Next checks should be rejected.
    assert!(!limiter.check("client-a"));
    assert!(!limiter.check("client-b"));

    let stats = limiter.stats();
    assert_eq!(stats.total_allowed, 10);
    assert_eq!(stats.total_rejected, 2);
    assert_eq!(stats.active_keys, 2);

    // Evict after idle.
    std::thread::sleep(Duration::from_millis(15));
    let evicted = limiter.evict_stale(Duration::from_millis(10));
    assert_eq!(evicted, 2);
    assert_eq!(limiter.key_count(), 0);
}

/// Tests relay dedup under concurrent senders.
#[cfg(feature = "relay")]
#[test]
fn relay_concurrent_dedup() {
    use majra::relay::{Relay, RelayMessage};

    let relay = Arc::new(Relay::new("receiver"));
    let mut handles = Vec::new();

    for sender_idx in 0..4 {
        let r = relay.clone();
        handles.push(std::thread::spawn(move || {
            let from = format!("sender-{sender_idx}");
            for seq in 1..=10u64 {
                let msg = RelayMessage {
                    seq,
                    from: from.clone(),
                    to: "receiver".into(),
                    topic: "data".into(),
                    payload: serde_json::Value::Null,
                    timestamp: chrono::Utc::now(),
                    correlation_id: None,
                    is_reply: false,
                };
                r.receive(msg);
            }
            // Replay same sequences — should all be deduped.
            for seq in 1..=10u64 {
                let msg = RelayMessage {
                    seq,
                    from: from.clone(),
                    to: "receiver".into(),
                    topic: "data".into(),
                    payload: serde_json::Value::Null,
                    timestamp: chrono::Utc::now(),
                    correlation_id: None,
                    is_reply: false,
                };
                r.receive(msg);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let stats = relay.stats();
    assert_eq!(stats.messages_received, 40); // 4 senders * 10 unique
    assert_eq!(stats.duplicates_dropped, 40); // 4 senders * 10 replayed
}

// ---------------------------------------------------------------------------
// Live Redis integration tests (require running Redis)
// ---------------------------------------------------------------------------

/// Redis pub/sub, queue, rate limiter, and heartbeat tracker end-to-end.
///
/// Run with: `cargo test --features redis-backend -- --ignored redis_live`
#[cfg(feature = "redis-backend")]
#[tokio::test]
#[ignore]
async fn redis_live_full_lifecycle() {
    use majra::redis_backend::{RedisHeartbeatTracker, RedisPubSub, RedisQueue, RedisRateLimiter};

    let client =
        redis::Client::open("redis://127.0.0.1/").expect("Redis must be running for this test");

    // --- Pub/Sub ---
    let hub = RedisPubSub::new(client.clone(), "majra:test:live:");
    let mut rx = hub.subscribe::<serde_json::Value>("events/*", 16).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await; // let subscriber connect
    let delivered = hub
        .publish("events/created", &serde_json::json!({"id": 1}))
        .await
        .unwrap();
    if delivered > 0 {
        let (topic, payload) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(topic.contains("events/created"));
        assert_eq!(payload["id"], 1);
    }

    // --- Queue ---
    let q = RedisQueue::new(client.clone(), "majra:test:live:queue");
    q.clear().await.unwrap();
    q.enqueue(5u8, &serde_json::json!({"task": "train"}))
        .await
        .unwrap();
    q.enqueue(1u8, &serde_json::json!({"task": "index"}))
        .await
        .unwrap();
    assert_eq!(q.len().await.unwrap(), 2);
    let job: serde_json::Value = q.dequeue().await.unwrap().unwrap();
    assert_eq!(job["task"], "train"); // highest priority first
    q.clear().await.unwrap();

    // --- Rate Limiter ---
    let rl = RedisRateLimiter::new(client.clone(), 2.0, 2, "majra:test:live:rl:");
    assert!(rl.check("user:1").await.unwrap());
    assert!(rl.check("user:1").await.unwrap());
    assert!(!rl.check("user:1").await.unwrap()); // burst exhausted

    // --- Heartbeat ---
    let hb = RedisHeartbeatTracker::new(client.clone(), "majra:test:live:hb:", 10);
    hb.register("node-1", &serde_json::json!({"gpu": true}))
        .await
        .unwrap();
    assert!(hb.is_online("node-1").await.unwrap());
    let meta = hb.get_metadata("node-1").await.unwrap().unwrap();
    assert_eq!(meta["gpu"], true);
    let online = hb.list_online().await.unwrap();
    assert!(online.iter().any(|(id, _)| id == "node-1"));
    hb.deregister("node-1").await.unwrap();
    assert!(!hb.is_online("node-1").await.unwrap());
}

// ---------------------------------------------------------------------------
// Live PostgreSQL integration tests (require running PostgreSQL)
// ---------------------------------------------------------------------------

/// PostgreSQL workflow storage and queue backend end-to-end.
///
/// Run with: `cargo test --features postgres,dag,queue -- --ignored postgres_live`
///
/// Expects `MAJRA_TEST_PG` env var with connection string, e.g.:
/// `MAJRA_TEST_PG="postgresql://user:pass@localhost/majra_test"`
#[cfg(all(feature = "postgres", feature = "dag"))]
#[tokio::test]
#[ignore]
async fn postgres_live_workflow_storage() {
    use majra::dag::{WorkflowDefinition, WorkflowRunStatus, WorkflowStep, WorkflowStorage};
    use majra::postgres_backend::PostgresWorkflowStorage;

    let conn_str = std::env::var("MAJRA_TEST_PG")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost/majra_test".to_string());

    let storage = Arc::new(
        PostgresWorkflowStorage::connect(&conn_str)
            .await
            .expect("PostgreSQL must be running with the test database"),
    );

    // Create a simple workflow definition.
    let def = WorkflowDefinition {
        id: "test-wf-live".into(),
        name: "Live Test Workflow".into(),
        description: Some("Integration test".into()),
        steps: vec![WorkflowStep {
            id: "step-1".into(),
            name: "Step One".into(),
            depends_on: vec![],
            trigger_mode: majra::dag::TriggerMode::All,
            config: serde_json::json!({"action": "noop"}),
            error_policy: majra::dag::ErrorPolicy::default(),
            retry_policy: majra::dag::RetryPolicy::default(),
        }],
        enabled: true,
        version: 1,
        created_by: "test".into(),
        created_at: chrono::Utc::now().timestamp_millis(),
        updated_at: chrono::Utc::now().timestamp_millis(),
    };

    // CRUD definition.
    storage.create_definition(&def).await.unwrap();
    let loaded = storage
        .get_definition("test-wf-live")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.name, "Live Test Workflow");
    assert_eq!(loaded.steps.len(), 1);

    let defs = storage.list_definitions(10, 0).await.unwrap();
    assert!(defs.iter().any(|d| d.id == "test-wf-live"));

    // Clean up.
    assert!(storage.delete_definition("test-wf-live").await.unwrap());
    assert!(
        storage
            .get_definition("test-wf-live")
            .await
            .unwrap()
            .is_none()
    );
}
