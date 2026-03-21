# Majra

> مجرا (Arabic/Persian: conduit, channel) — Distributed queue & multiplex engine for Rust

Majra provides shared messaging primitives for the [AGNOS](https://github.com/MacCracken) ecosystem, eliminating duplicate pub/sub, queue, relay, and heartbeat implementations across [AgnosAI](https://github.com/MacCracken/agnosai), [Ifran](https://github.com/MacCracken/synapse), [SecureYeoman](https://github.com/MacCracken/secureyeoman), and [daimon](https://github.com/agnostos/daimon).

**Pure Rust, async-native** — built on tokio, zero-copy where possible.

## Features

| Module | Feature | Description |
|--------|---------|-------------|
| **pubsub** | `pubsub` | Topic-based pub/sub with MQTT-style `*`/`#` wildcard matching |
| **pubsub** | `pubsub` | `TypedPubSub<T>` — generic, type-safe pub/sub with backpressure, replay, and filters |
| **queue** | `queue` | Multi-tier priority queue (5 levels) with DAG dependency scheduling |
| **queue** | `queue` | `ManagedQueue<T>` — resource-aware, lifecycle-tracked, concurrent job queue |
| **relay** | `relay` | Sequenced, deduplicated inter-node message relay |
| **transport** | `relay` | Pluggable transport trait + multiplexed connection pool |
| **ipc** | `ipc` | Length-prefixed framing over Unix domain sockets |
| **heartbeat** | `heartbeat` | TTL-based node health: Online → Suspect → Offline with GPU telemetry and fleet stats |
| **ratelimit** | `ratelimit` | Per-key token bucket rate limiter with stale-key eviction and stats |
| **barrier** | `barrier` | N-way barrier sync with deadlock recovery + async `arrive_and_wait()` |
| **metrics** | always | `MajraMetrics` trait — wire to Prometheus/OpenTelemetry |

Default features: `pubsub`, `queue`, `relay`, `heartbeat`.

Optional: `ipc`, `ratelimit`, `barrier`, `sqlite` (persistent queue backing).

## Quick Start

```toml
[dependencies]
majra = "0.21"
```

### Typed Pub/Sub

```rust
use majra::pubsub::{TypedPubSub, TypedPubSubConfig};

let hub = TypedPubSub::<MyEvent>::new();

// Subscribe with a filter — only high-priority events.
let mut rx = hub.subscribe_filtered("events/#", |e: &MyEvent| e.priority > 5);

hub.publish("events/training", MyEvent { priority: 10, .. });

let msg = rx.recv().await.unwrap();
```

### Managed Queue (GPU-aware scheduling)

```rust
use majra::queue::{ManagedQueue, ManagedQueueConfig, Priority, ResourceReq, ResourcePool};
use std::time::Duration;

let queue = ManagedQueue::new(ManagedQueueConfig {
    max_concurrency: 4,
    finished_ttl: Duration::from_secs(3600),
});

// Enqueue a GPU training job.
let id = queue.enqueue(
    Priority::High,
    "train-llama-70b".to_string(),
    Some(ResourceReq { gpu_count: 4, vram_mb: 80_000 }),
).await;

// Dequeue only what fits available resources.
let pool = ResourcePool { gpu_count: 2, vram_mb: 16_000 };
if let Some(job) = queue.dequeue(&pool).await {
    // ... run the job ...
    queue.complete(job.id).unwrap();
}
```

### Fleet Heartbeat with GPU Telemetry

```rust
use majra::heartbeat::{ConcurrentHeartbeatTracker, GpuTelemetry, HeartbeatConfig};

let tracker = ConcurrentHeartbeatTracker::default();
tracker.register_with_telemetry("gpu-node-1", serde_json::json!({}), vec![
    GpuTelemetry {
        utilization_pct: 85.0,
        memory_used_mb: 6000,
        memory_total_mb: 8000,
        temperature_c: Some(72.0),
    },
]);

let stats = tracker.fleet_stats();
println!("{} nodes, {} GPUs, {} MB VRAM available",
    stats.total_nodes, stats.total_gpus, stats.available_vram_mb);
```

### Async Barrier

```rust
use majra::barrier::AsyncBarrierSet;

let barriers = AsyncBarrierSet::new();
barriers.create("training-sync", participants);

// Each worker awaits the barrier — released when all arrive.
barriers.arrive_and_wait("training-sync", "worker-0").await?;
```

### Priority Queue

```rust
use majra::queue::{PriorityQueue, QueueItem, Priority};

let mut q = PriorityQueue::new();
q.enqueue(QueueItem::new(Priority::Low, "background-task"));
q.enqueue(QueueItem::new(Priority::Critical, "urgent-task"));

assert_eq!(q.dequeue().unwrap().payload, "urgent-task");
```

### Message Relay

```rust
use majra::relay::Relay;

let relay = Relay::new("node-1");
let mut rx = relay.subscribe();

relay.broadcast("announce", serde_json::json!({"joined": true}));
```

### Rate Limiter with Stats

```rust
use majra::ratelimit::RateLimiter;

let limiter = RateLimiter::new(100.0, 50); // 100 req/s, burst 50
limiter.check("client-ip");

let stats = limiter.stats();
// Evict idle keys to prevent unbounded memory growth.
limiter.evict_stale(std::time::Duration::from_secs(300));
```

## Ecosystem

Majra unifies patterns from battle-tested implementations:

| Consumer | What majra replaces |
|----------|-------------------|
| **Ifran** | Job scheduler (PriorityQueue), fleet heartbeat, rate limiting |
| **SecureYeoman** | EventDispatcher, A2A heartbeat, sliding-window rate limiter, DAG workflow, swarm barriers (~3,200 lines) |
| **AgnosAI** | Priority DAG scheduling, pub/sub wildcards, relay dedup, barrier sync |
| **daimon** | Topic routing, fleet relay, IPC framing |

## License

AGPL-3.0-only
