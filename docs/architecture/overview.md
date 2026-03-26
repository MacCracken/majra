# Architecture Overview

majra is a modular concurrency primitives library. Each module is feature-gated
and can be used independently.

## Module Map

```
majra (v1.0.0, ~12,000 lines across 20 modules)
│
│ ── Always available ──────────────────────────────
├── envelope        Universal message envelope (Envelope, Target)
├── error           Shared error types (MajraError, IpcError)
├── metrics         MajraMetrics trait + PrometheusMetrics
├── namespace       Multi-tenant scoping (Namespace)
│
│ ── Default features ──────────────────────────────
├── pubsub          Topic-based pub/sub + TypedPubSub<T>
├── queue           PriorityQueue + ManagedQueue + DAG scheduler
├── relay           Sequenced, deduplicated message relay + request-response
├── transport       Transport trait + ConnectionPool (with stale eviction)
├── heartbeat       Node health FSM + GPU telemetry + fleet stats
│
│ ── Optional features ─────────────────────────────
├── ipc             Unix socket / Windows named pipe framing
├── ipc_encrypted   AES-256-GCM encrypted IPC (ring)
├── ratelimit       Token bucket rate limiter
├── barrier         N-way barrier + AsyncBarrierSet
├── dag             DAG workflow engine (WorkflowEngine, WorkflowStorage)
├── fleet           Distributed job queue with work-stealing
├── ws              WebSocket bridge for pub/sub fan-out
│
│ ── Backend features ──────────────────────────────
├── redis_backend   RedisPubSub, RedisQueue, RedisRateLimiter, RedisHeartbeatTracker
├── postgres_backend PostgresWorkflowStorage (deadpool connection pool)
├── [sqlite]        SQLite persistence for ManagedQueue + WorkflowStorage
└── [quic]          QUIC transport (quinn + rustls)
```

## Feature Dependencies

```
pubsub ─────────────────────── ws (implies pubsub)
queue ──────────────────────── dag (implies queue)
                            ── fleet (implies queue + heartbeat)
relay ──────────────────────── quic (implies relay)
                            ── transport (always with relay)
ipc ────────────────────────── ipc-encrypted (implies ipc)
```

## Design Principles

1. **Feature-gated** — consumers include only what they need.
2. **Thread-safe by default** — concurrent variants use `DashMap` or `tokio::sync`.
3. **Async-native** — built on tokio; `Send + Sync` on all public types.
4. **Zero unsafe** — no unsafe code in the library.
5. **Lean** — minimal core deps (tokio, dashmap, serde, chrono, uuid, thiserror, tracing).
6. **Eviction everywhere** — all collections have TTL/capacity-based eviction to prevent unbounded growth.
7. **Multi-tenant ready** — `Namespace` module provides topic/key/node-ID scoping.
8. **Fragmentation-aware** — `compact()` methods on Relay and RateLimiter reclaim DashMap dead capacity. For long-running processes with high key churn, use `tikv-jemallocator` to avoid glibc heap fragmentation.

## Concurrency Model

| Type | Backing | Use case |
|------|---------|----------|
| `PriorityQueue<T>` | `VecDeque` tiers | Single-owner, `&mut self` |
| `ConcurrentPriorityQueue<T>` | `tokio::Mutex` | Shared, async dequeue |
| `ManagedQueue<T>` | `tokio::Mutex` + `DashMap` | Full lifecycle management |
| `ConcurrentHeartbeatTracker` | `DashMap` | Shared fleet tracking |
| `AsyncBarrierSet` | `DashMap` + `Notify` | Async wait-for-release |
| `RateLimiter` | `DashMap` | Shared, lock-free per key |
| `Relay` | `DashMap` + `AtomicU64` | Shared, lock-free dedup + request-response |
| `PubSub` / `TypedPubSub<T>` | `DashMap` | Shared, fan-out with auto-cleanup |
| `ConnectionPool` | `tokio::Mutex<HashMap>` | Per-endpoint connection reuse |
| `FleetQueue<T>` | `DashMap<NodeId, FleetNode>` | Distributed work-stealing |
| `InMemoryWorkflowStorage` | `DashMap` | DAG run/step storage with retention |

## Data Flow

```text
Producer ──► PubSub/TypedPubSub ──► pattern match ──► broadcast::Receiver
                                                   └──► WsBridge ──► WebSocket clients

Producer ──► ManagedQueue ──► resource filter ──► Consumer
                           └──► QueueEvent broadcast
                           └──► SqliteBackend / PostgreSQL persistence

Node A ──► Relay::send_request() ──► correlation_id ──► Node B
Node B ──► Relay::reply() ──────────────────────────► oneshot::Receiver on A

FleetQueue::submit() ──► select_node (least loaded) ──► ManagedQueue on target node
FleetQueue::rebalance() ──► steal from overloaded ──► redistribute to idle
```

## Distributed Architecture

```text
Process A                          Redis                          Process B
┌─────────────┐                  ┌─────────┐                  ┌─────────────┐
│ RedisPubSub │ ◄──── pub/sub ──►│  Redis  │◄──── pub/sub ──►│ RedisPubSub │
│ RedisQueue  │ ◄──── sorted ───►│  Server │◄──── sorted ───►│ RedisQueue  │
│ RedisRL     │ ◄──── lua ──────►│         │◄──── lua ──────►│ RedisRL     │
│ RedisHB     │ ◄──── TTL keys ─►│         │◄──── TTL keys ─►│ RedisHB     │
└─────────────┘                  └─────────┘                  └─────────────┘

Process A                        PostgreSQL                    Process B
┌──────────────────┐           ┌───────────┐              ┌──────────────────┐
│ PostgresWorkflow │ ◄── SQL ──►│  Postgres │◄── SQL ────►│ PostgresWorkflow │
│ Storage          │           │  Server   │              │ Storage          │
└──────────────────┘           └───────────┘              └──────────────────┘
```

## Consumers

| Project | Modules used |
|---------|-------------|
| **Ifran** | queue, heartbeat, ratelimit |
| **SecureYeoman** | pubsub, ratelimit (via NAPI); expanding to queue, heartbeat, dag |
| **AgnosAI** | pubsub, queue, relay, barrier |
| **daimon** | pubsub, relay, ipc |
| **stiva** | dag, heartbeat, queue |

## Dependencies

### Core (always compiled)
tokio, dashmap, serde, serde_json, chrono, uuid, thiserror, tracing, async-trait

### Optional (feature-gated)
rusqlite (sqlite), prometheus, redis + futures-util (redis-backend), tokio-postgres + deadpool-postgres (postgres), quinn + rustls + rcgen (quic), ring (ipc-encrypted), tokio-tungstenite (ws), tracing-subscriber (logging)
