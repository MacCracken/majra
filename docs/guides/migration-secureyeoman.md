# Migrating SecureYeoman to majra

> SecureYeoman currently uses majra for **pub/sub** (event dispatcher) and
> **rate limiting** (per-user, per-IP throttling) via NAPI bindings in `sy-napi`.
> This guide covers expanding to the full majra feature set.

## Current integration (via sy-napi)

| SY component | majra feature | Status |
|-------------|--------------|--------|
| EventDispatcher | `pubsub` (TypedPubSub) | In production |
| Rate limiter | `ratelimit` (token bucket) | In production |

## Phase 1: Expand NAPI bindings (low risk)

### Add queue + heartbeat to sy-napi

```toml
# crates/sy-napi/Cargo.toml
majra = { path = "../../../majra", features = ["pubsub", "ratelimit", "queue", "heartbeat", "dag"] }
```

### Replace setInterval heartbeat scheduling

SY's `HeartbeatManager` uses `setInterval` with custom interval tracking. Replace with majra:

```rust
// In sy-napi/src/majra.rs — add heartbeat bindings
use majra::heartbeat::{ConcurrentHeartbeatTracker, HeartbeatConfig, EvictionPolicy};

let tracker = ConcurrentHeartbeatTracker::new(HeartbeatConfig {
    suspect_after: Duration::from_secs(15),
    offline_after: Duration::from_secs(45),
    eviction_policy: Some(EvictionPolicy {
        offline_cycles: 3,
        eviction_tx: Some(tx),
    }),
});
```

### Replace webhook retry polling with ManagedQueue

SY polls `event_deliveries` table with `setInterval`. Replace with majra's priority queue:

```rust
use majra::queue::{ManagedQueue, ManagedQueueConfig, Priority};

let queue = ManagedQueue::new(ManagedQueueConfig {
    max_concurrency: 10,
    finished_ttl: Duration::from_secs(3600),
});

// Enqueue webhook delivery.
queue.enqueue(Priority::Normal, webhook_payload, None).await;

// Dequeue and deliver (no polling needed — use dequeue_wait).
let job = queue.dequeue_wait().await;
```

## Phase 2: DAG workflow engine (medium risk)

SY's `workflow-engine.ts` has a custom topological sort. Replace with majra's DAG engine:

```rust
use majra::dag::{WorkflowEngine, WorkflowEngineConfig, InMemoryWorkflowStorage, StepExecutor};

// Implement StepExecutor for SY's step types.
struct SyStepExecutor { /* agent, tool, webhook, swarm handlers */ }

#[async_trait]
impl StepExecutor for SyStepExecutor {
    async fn execute(&self, step: &WorkflowStep, ctx: &WorkflowContext) -> Result<Value, String> {
        match step.config["type"].as_str() {
            Some("agent") => execute_agent(step, ctx).await,
            Some("webhook") => execute_webhook(step, ctx).await,
            // ... 16+ step types
        }
    }
}

let engine = WorkflowEngine::new(
    Arc::new(PostgresWorkflowStorage::connect(&pg_url).await?),
    Arc::new(SyStepExecutor::new()),
    WorkflowEngineConfig::default(),
);
```

## Phase 3: Multi-tenant isolation

SY is multi-tenant. Use `Namespace` to scope all operations:

```rust
use majra::namespace::Namespace;

let ns = Namespace::new(&tenant_id);

// Scoped pub/sub.
hub.publish(&ns.topic("events/created"), payload);
let mut rx = hub.subscribe(&ns.pattern("events/#"));

// Scoped rate limiting.
limiter.check(&ns.key("api_requests"));

// Scoped metrics.
let metrics = NamespacedMetrics::new(&tenant_id, prometheus_metrics.clone());
```

## Phase 4: Relay for A2A delegation (higher risk)

SY's A2A protocol uses custom encrypted envelopes. majra's relay provides:

```rust
use majra::relay::Relay;

let relay = Relay::new(&node_id);

// Request-response for delegation.
let (seq, rx) = relay.send_request("target-node", "a2a/delegate", json!({
    "task": "summarize",
    "context": "...",
}));

// Await reply with timeout.
let reply = tokio::time::timeout(Duration::from_secs(30), rx).await??;
```

For encrypted IPC (desktop/capture):

```rust
use majra::ipc_encrypted::EncryptedIpcConnection;

let conn = EncryptedIpcConnection::connect(path, &shared_key).await?;
conn.send(&json!({"command": "capture"})).await?;
```

## Phase 5: Distributed backends

When scaling beyond a single process:

```rust
// Distributed rate limiting via Redis.
use majra::redis_backend::RedisRateLimiter;
let rl = RedisRateLimiter::new(redis_client, 100.0, 50, "sy:rl:");

// Distributed heartbeat for edge devices.
use majra::redis_backend::RedisHeartbeatTracker;
let hb = RedisHeartbeatTracker::new(redis_client, "sy:hb:", 30);

// PostgreSQL workflow storage.
use majra::postgres_backend::PostgresWorkflowStorage;
let storage = PostgresWorkflowStorage::connect(&pg_url).await?;
```

## Cargo.toml (full integration)

```toml
[dependencies]
majra = { version = "1", features = [
    "pubsub", "queue", "relay", "heartbeat", "ratelimit",
    "dag", "barrier", "ipc-encrypted", "redis-backend", "postgres",
    "prometheus", "logging"
] }
```
