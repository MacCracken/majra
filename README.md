# Majra

> مجرا (Arabic/Persian: conduit, channel) — Distributed queue & multiplex engine for Rust

Majra provides shared messaging primitives for the [AGNOS](https://github.com/MacCracken) ecosystem, eliminating duplicate pub/sub, queue, relay, and heartbeat implementations across [AgnosAI](https://github.com/MacCracken/agnosai), [Ifran](https://github.com/MacCracken/synapse), [SecureYeoman](https://github.com/MacCracken/secureyeoman), and [daimon](https://github.com/agnostos/daimon).

**Pure Rust, async-native** — built on tokio, zero-copy where possible.

## Features

| Module | Feature | Description |
|--------|---------|-------------|
| **pubsub** | `pubsub` | Topic-based pub/sub with MQTT-style `*`/`#` wildcard matching |
| **pubsub** | `pubsub` | `TypedPubSub<T>` — generic, type-safe pub/sub with backpressure, replay, filters, auto-cleanup |
| **queue** | `queue` | Multi-tier priority queue (5 levels) with DAG dependency scheduling |
| **queue** | `queue` | `ManagedQueue<T>` — resource-aware, lifecycle-tracked, concurrent job queue |
| **relay** | `relay` | Sequenced, deduplicated inter-node message relay with request-response correlation |
| **transport** | `relay` | Pluggable transport trait + multiplexed connection pool with stale eviction |
| **ipc** | `ipc` | Length-prefixed framing over Unix domain sockets |
| **ipc-encrypted** | `ipc-encrypted` | AES-256-GCM encrypted IPC channels via `ring` |
| **heartbeat** | `heartbeat` | TTL-based node health: Online / Suspect / Offline with GPU telemetry and fleet stats |
| **ratelimit** | `ratelimit` | Per-key token bucket rate limiter with stale-key eviction and stats |
| **barrier** | `barrier` | N-way barrier sync with deadlock recovery + async `arrive_and_wait()` |
| **dag** | `dag` | DAG-based workflow engine with tier scheduling, retry, error policies, pluggable storage |
| **fleet** | `fleet` | Distributed job queue with work-stealing across nodes |
| **namespace** | always | Multi-tenant scoping for topics, keys, and node IDs |
| **metrics** | always | `MajraMetrics` trait — wire to Prometheus/OpenTelemetry |
| **redis** | `redis-backend` | Cross-process pub/sub, queues, distributed rate limiter, distributed heartbeat |
| **postgres** | `postgres` | PostgreSQL-backed workflow storage with connection pooling |
| **ws** | `ws` | WebSocket bridge — fan out pub/sub topics to WebSocket clients |
| **quic** | `quic` | QUIC transport with multiplexed streams and datagrams |

Default features: `pubsub`, `queue`, `relay`, `heartbeat`.

## Quick Start

```toml
[dependencies]
majra = "1.0"
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

### Relay with Request-Response

```rust
use majra::relay::Relay;

let relay = Relay::new("node-1");
let mut sub = relay.subscribe();

// Fire-and-forget broadcast.
relay.broadcast("announce", serde_json::json!({"joined": true}));

// Request-response (RPC pattern).
let (seq, rx) = relay.send_request("node-2", "rpc/ping", serde_json::json!({}));
// Await reply with timeout...
```

### Multi-Tenant Isolation

```rust
use majra::namespace::Namespace;
use majra::pubsub::PubSub;

let hub = PubSub::new();
let ns_a = Namespace::new("tenant-a");
let ns_b = Namespace::new("tenant-b");

let mut rx_a = hub.subscribe(&ns_a.pattern("events/#"));
hub.publish(&ns_a.topic("events/created"), serde_json::json!({"a": 1}));
// Only tenant-a receives the message.
```

### Distributed Rate Limiting (Redis)

```rust
use majra::redis_backend::RedisRateLimiter;

let client = redis::Client::open("redis://127.0.0.1/").unwrap();
let limiter = RedisRateLimiter::new(client, 100.0, 50, "myapp:rl:");

if limiter.check("user:123").await? {
    // Request allowed
}
```

### Fleet Heartbeat with GPU Telemetry

```rust
use majra::heartbeat::{ConcurrentHeartbeatTracker, GpuTelemetry};

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
```

## Architecture

```text
majra
├── pubsub          ── TypedPubSub<T>, PubSub, wildcard matching
├── queue           ── PriorityQueue, ManagedQueue, DagScheduler
├── relay           ── Relay (dedup, request-response), Transport, ConnectionPool
├── heartbeat       ── ConcurrentHeartbeatTracker, GpuTelemetry, FleetStats
├── ratelimit       ── RateLimiter (token bucket)
├── barrier         ── AsyncBarrierSet
├── dag             ── WorkflowEngine, WorkflowStorage (InMemory, SQLite, PostgreSQL)
├── fleet           ── FleetQueue (work-stealing)
├── ipc             ── IpcServer, IpcConnection
├── ipc_encrypted   ── EncryptedIpcConnection (AES-256-GCM)
├── namespace       ── Namespace (multi-tenant scoping)
├── ws              ── WsBridge (WebSocket fan-out)
├── redis_backend   ── RedisPubSub, RedisQueue, RedisRateLimiter, RedisHeartbeatTracker
├── postgres_backend── PostgresWorkflowStorage
├── metrics         ── MajraMetrics, PrometheusMetrics
├── envelope        ── Envelope, Target
└── error           ── MajraError, IpcError
```

## Ecosystem

| Consumer | What majra replaces |
|----------|-------------------|
| **Ifran** | Job scheduler, fleet heartbeat, rate limiting |
| **SecureYeoman** | EventDispatcher, A2A heartbeat, rate limiter, DAG workflow, swarm barriers |
| **AgnosAI** | Priority DAG scheduling, pub/sub wildcards, relay dedup, barrier sync |
| **daimon** | Topic routing, fleet relay, IPC framing |
| **stiva** | DagScheduler for compose ordering, HeartbeatTracker for container health |

## Building

```bash
cargo build --all-features
cargo test --all-features
make check              # fmt + clippy + test + audit
make bench              # criterion benchmarks with history
```

## License

AGPL-3.0-only
