# Architecture Overview

majra is a modular concurrency primitives library. Each module is feature-gated
and can be used independently.

## Module Map

```
majra
├── envelope        (always)    Universal message envelope
├── error           (always)    Shared error types
├── metrics         (always)    MajraMetrics trait for observability
│
├── pubsub          (default)   Topic-based pub/sub + TypedPubSub<T>
├── queue           (default)   PriorityQueue + ManagedQueue + DAG scheduler
├── relay           (default)   Sequenced, deduplicated message relay
├── transport       (relay)     Transport trait + ConnectionPool
├── heartbeat       (default)   Node health FSM + GPU telemetry + fleet stats
│
├── ipc             (opt)       Unix socket framing
├── ratelimit       (opt)       Token bucket rate limiter
├── barrier         (opt)       N-way barrier + AsyncBarrierSet
└── [sqlite]        (opt)       Persistent queue backing
```

## Design Principles

1. **Feature-gated** — consumers include only what they need.
2. **Thread-safe by default** — concurrent variants (`Concurrent*`, `ManagedQueue`,
   `AsyncBarrierSet`) use `DashMap` or `tokio::sync` primitives.
3. **Async-native** — built on tokio; `Send + Sync` on all public types.
4. **Zero unsafe** — no unsafe code in the library.
5. **Lean** — minimal dependencies (tokio, dashmap, serde, chrono, uuid, thiserror, tracing).

## Concurrency Model

| Type | Backing | Use case |
|------|---------|----------|
| `PriorityQueue<T>` | `VecDeque` tiers | Single-owner, `&mut self` |
| `ConcurrentPriorityQueue<T>` | `tokio::Mutex` | Shared, async dequeue |
| `ManagedQueue<T>` | `tokio::Mutex` + `DashMap` | Full lifecycle management |
| `HeartbeatTracker` | `HashMap` | Single-owner |
| `ConcurrentHeartbeatTracker` | `DashMap` | Shared fleet tracking |
| `BarrierSet` | `HashMap` | Single-owner |
| `ConcurrentBarrierSet` | `DashMap` | Shared sync |
| `AsyncBarrierSet` | `DashMap` + `Notify` | Async wait-for-release |
| `RateLimiter` | `DashMap` | Shared, lock-free per key |
| `Relay` | `DashMap` + `AtomicU64` | Shared, lock-free dedup |
| `PubSub` / `TypedPubSub<T>` | `DashMap` | Shared, fan-out |

## Consumers

| Project | Modules used |
|---------|-------------|
| **Ifran** | queue (PriorityQueue), heartbeat, ratelimit |
| **SecureYeoman** | All (planned migration of ~3,200 lines) |
| **AgnosAI** | pubsub, queue, relay, barrier |
| **daimon** | pubsub, relay, ipc |
