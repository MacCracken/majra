# Migrating Ifran to majra

> Ifran manages model lifecycle (download, conversion, quantisation, indexing).
> This guide covers replacing Ifran's internal queue, pub/sub, and heartbeat
> implementations with majra primitives.

## Modules to adopt

| Ifran component | majra replacement | Feature flag |
|----------------|-------------------|-------------|
| Job queue (training, indexing) | `ManagedQueue<T>` | `queue` |
| Model event bus | `TypedPubSub<ModelEvent>` | `pubsub` |
| Node health tracking | `ConcurrentHeartbeatTracker` | `heartbeat` |
| GPU resource filtering | `ResourceReq` / `ResourcePool` | `queue` |
| Job persistence | `SqliteBackend` | `sqlite` |

## Step 1: Replace the job queue

Ifran's internal training/indexing queue becomes a `ManagedQueue<IfranJob>`:

```rust
use majra::queue::{ManagedQueue, ManagedQueueConfig, Priority, ResourceReq, ResourcePool};

#[derive(Clone, Serialize, Deserialize)]
struct IfranJob {
    model_id: String,
    operation: IfranOp, // Train, Index, Convert, Quantise
    params: serde_json::Value,
}

let config = ManagedQueueConfig {
    max_concurrency: 4,
    finished_ttl: Duration::from_secs(3600),
};
let queue = ManagedQueue::<IfranJob>::new(config);

// Enqueue a GPU training job.
let id = queue.enqueue(
    Priority::High,
    IfranJob { model_id: "llama-70b".into(), operation: IfranOp::Train, params: json!({}) },
    Some(ResourceReq { gpu_count: 2, vram_mb: 32000 }),
).await;

// Dequeue respecting available resources.
let pool = ResourcePool { gpu_count: 4, vram_mb: 80000 };
if let Some(job) = queue.dequeue(&pool).await {
    // Run the job...
    queue.complete(job.id)?;
}
```

### Key differences from Ifran's internal queue

- Priority tiers (5 levels) replace flat FIFO
- Resource-aware dequeue replaces manual GPU slot tracking
- Lifecycle events (`subscribe_events()`) replace callback hooks
- `get()` replaces any internal job lookup

## Step 2: Replace the model event bus

Replace Ifran's event emitter with `TypedPubSub`:

```rust
use majra::pubsub::{TypedPubSub, TypedPubSubConfig, BackpressurePolicy};

#[derive(Clone, Serialize, Deserialize)]
struct ModelEvent {
    model_id: String,
    event: String, // "download_complete", "training_started", etc.
    progress: Option<f32>,
}

let config = TypedPubSubConfig {
    channel_capacity: 1024,
    backpressure: BackpressurePolicy::DropOldest,
    replay_capacity: 50, // Late subscribers catch up on recent events.
};
let events = TypedPubSub::<ModelEvent>::with_config(config);

// Subscribe to all events for a specific model.
let mut rx = events.subscribe("models/llama-70b/#");

// Publish.
events.publish("models/llama-70b/training/started", ModelEvent {
    model_id: "llama-70b".into(),
    event: "training_started".into(),
    progress: Some(0.0),
});
```

## Step 3: Replace node health tracking

```rust
use majra::heartbeat::{ConcurrentHeartbeatTracker, HeartbeatConfig, GpuTelemetry};

let tracker = ConcurrentHeartbeatTracker::new(HeartbeatConfig {
    suspect_after: Duration::from_secs(30),
    offline_after: Duration::from_secs(90),
    eviction_policy: None,
});

// Register a GPU node.
tracker.register_with_telemetry("gpu-node-0", json!({"role": "trainer"}), vec![
    GpuTelemetry {
        utilization_pct: 0.0,
        memory_used_mb: 0,
        memory_total_mb: 80000,
        temperature_c: Some(35.0),
    },
]);

// Periodic sweep — call from a background task.
let transitions = tracker.update_statuses();
let stats = tracker.fleet_stats();
```

## Step 4: Add SQLite persistence (optional)

```rust
use majra::queue::persistence::SqliteBackend;

let backend = Arc::new(SqliteBackend::open(Path::new("ifran-queue.db"))?);
let queue = ManagedQueue::<IfranJob>::with_sqlite(config, backend);
```

## Cargo.toml

```toml
[dependencies]
majra = { version = "1", features = ["queue", "pubsub", "heartbeat", "sqlite"] }
```
