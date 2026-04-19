# Architecture Overview

majra is a modular concurrency primitives library written in Cyrius. Each module
is a `.cyr` file included via `include` directives in dependency order.

## Module Map

```
majra (v2.3.0, ~4,800 lines across 19 modules)
│
│ ── Core (always included) ────────────────────
├── error           Error codes (enum) + result helpers
├── counter         Mutex-protected i64 counter
├── envelope        Universal message envelope (UUID, routing, payload)
├── namespace       Multi-tenant scoping (topic, key, node_id prefixing)
├── metrics         22-slot function pointer vtable for observability
│
│ ── Primitives ────────────────────────────────
├── queue           5-tier priority queue + ManagedQueue with lifecycle
├── pubsub          MQTT wildcard matching + DirectChannel + HashedChannel
├── relay           Sequenced dedup relay with broadcast
├── barrier         N-way barrier (sync + concurrent with futex)
├── heartbeat       FSM health tracker + GPU telemetry + fleet stats
├── ratelimit       Token bucket + sliding window (fixed-point math)
│
│ ── Networking ────────────────────────────────
├── ipc             Unix domain socket framing (4-byte BE length prefix)
├── ipc_encrypted   AES-256-GCM framing with nonce management
├── transport       Transport vtable + circuit breaker + connection pool
├── ws              WebSocket (SHA-1 handshake, RFC 6455 framing)
│
│ ── Composition ───────────────────────────────
├── fleet           Distributed job queue with work-stealing
├── dag             DAG workflow engine (Kahn's sort, retry, error policies)
│
│ ── Backends ([lib.backends] profile only) ────
├── redis_backend   RESP2 protocol (SET/GET, ZADD, PUBLISH, HSET, EVAL)
└── postgres_backend PostgreSQL v3 wire protocol (startup, auth, query, CRUD)
```

## Distribution profiles

Two bundles are produced by `cyrius distlib`:

| Bundle                    | Profile           | Modules            | Consumer use-case                                      |
|---------------------------|-------------------|--------------------|--------------------------------------------------------|
| `dist/majra.cyr`          | `[lib]` (default) | 15 core modules    | In-process concurrency primitives; no network surface  |
| `dist/majra-backends.cyr` | `[lib.backends]`  | 15 core + 4 network modules (ipc_encrypted, ws, redis_backend, postgres_backend) | Full cross-process distribution |

## Design Principles

1. **Zero dependencies** — Cyrius stdlib only. No external crates, no LLVM.
2. **Thread-safe by default** — concurrent variants use mutex + futex.
3. **Globals for cross-call state** — cc5 is better than cc3, but deeply nested call chains can still clobber locals. Several modules (`postgres_backend`, `relay`, `barrier`) promote critical values to globals defensively.
4. **Fixed-point math** — no floating point; token buckets use x1000 scaling.
5. **Eviction everywhere** — all collections support TTL/capacity-based eviction.
6. **Multi-tenant ready** — Namespace module provides topic/key/node-ID scoping.
7. **Vtable polymorphism** — traits replaced by function pointer structs.

## Concurrency Model

| Type | Backing | Use case |
|------|---------|----------|
| PriorityQueue | 5 vecs (one per tier) | Single-owner enqueue/dequeue |
| ConcurrentPQ | PQ + mutex + futex | Shared, blocking dequeue_wait |
| ManagedQueue | CPQ + hashmap + mutex | Full lifecycle management |
| ConcurrentHeartbeatTracker | hashmap + mutex | Shared fleet tracking |
| ConcurrentBarrierSet | hashmap + mutex + futex | Blocking arrive_and_wait |
| RateLimiter | hashmap + mutex | Token bucket per key |
| SlidingWindowLimiter | hashmap + mutex | Window counter per key |
| Relay | hashmap + mutex + counter | Dedup + broadcast |
| DirectChannel | MPSC channel | Raw point-to-point |
| HashedChannel | hashmap of channels | Topic-routed |
| PubSub | 2 hashmaps (exact + pattern) | Full pub/sub with wildcards |
| ConnectionPool | hashmap + mutex | Per-endpoint reuse + circuit breaker |
| FleetQueue | hashmap + mutex | Distributed work-stealing |

## Data Flow

```
Producer ──► DirectChannel ──────────────────────► chan_recv     (368 ns/op)
Producer ──► HashedChannel ──► topic hash lookup ► chan_recv
Producer ──► PubSub ──► exact O(1) + pattern ───► chan_recv     (1 us/op)
                                                └──► WsBridge ──► WebSocket clients

Producer ──► ManagedQueue ──► priority dequeue ──► Consumer
                           └──► job state lifecycle (queued → running → completed)

Node A ──► relay_send() ──────────────────────────► subscribers via channels
Node B ──► relay_receive() ──► dedup filter ──────► subscribers

FleetQueue ──► select_node (least loaded) ──► ManagedQueue on target node
            ──► rebalance() ──► steal from overloaded ──► redistribute
```

## Distributed Architecture

```
Process A                          Redis                          Process B
┌─────────────┐                  ┌─────────┐                  ┌─────────────┐
│ redis_pub   │ ◄──── PUBLISH ──►│  Redis  │◄──── PUBLISH ──►│ redis_pub   │
│ redis_zadd  │ ◄──── ZADD ────►│  Server │◄──── ZPOPMIN ──►│ redis_zpop  │
│ redis_hset  │ ◄──── HSET ────►│         │◄──── HGET ────►│ redis_hget  │
│ redis_setex │ ◄──── SETEX ───►│         │◄──── EXISTS ──►│ redis_exist │
└─────────────┘                  └─────────┘                  └─────────────┘

Process A                        PostgreSQL                    Process B
┌──────────────────┐           ┌───────────┐              ┌──────────────────┐
│ pg_query         │ ◄── SQL ──►│  Postgres │◄── SQL ────►│ pg_query         │
│ pg_exec          │           │  Server   │              │ pg_exec          │
└──────────────────┘           └───────────┘              └──────────────────┘
```

## Consumers

| Project | Modules used |
|---------|-------------|
| **daimon** | pubsub, relay, ipc |
| **AgnosAI** | pubsub, queue, relay, barrier |
| **hoosh** | queue, heartbeat, fleet |
| **sutra** | heartbeat, fleet, dag |
| **stiva** | dag, heartbeat, ipc |
