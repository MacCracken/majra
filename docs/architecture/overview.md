# Architecture Overview

majra is a modular concurrency primitives library written in Cyrius. Each module
is a `.cyr` file included via `include` directives in dependency order.

## Module Map

```
majra (v2.7.3, ~7,500 lines across 22 modules; state.md's `src/` total of 8,212 lines / 23 files also counts the 680-line src/main.cyr self-test entry)
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
│                   + unsubscribe and per-subscriber lag policy (2.7.0)
├── relay           Sequenced dedup relay with broadcast
├── barrier         N-way barrier (sync + concurrent with futex)
├── heartbeat       FSM health tracker + GPU telemetry + fleet stats
├── ratelimit       Token bucket + sliding window (fixed-point math)
│
│ ── Networking (ipc_encrypted, ws: [lib.backends] only) ─
├── ipc             Unix domain socket framing (4-byte BE length prefix)
├── ipc_encrypted   AES-256-GCM framing with nonce management (sigil AES-GCM)
├── transport       Transport vtable + circuit breaker + connection pool
├── ws              WebSocket (SHA-1 handshake, RFC 6455 framing)
│
│ ── Composition ───────────────────────────────
├── fleet           Distributed job queue with work-stealing
├── dag             DAG workflow engine (Kahn's sort, retry, error policies)
│                   tiers serial by default; opt-in parallel (2.7.0)
│
│ ── Trust ([lib.signed] / [lib.backends]) ─────
├── signed_envelope Ed25519 signatures over canonical envelope encoding (sigil Ed25519)
│
│ ── Operations ([lib.admin] / [lib.backends]) ─
├── admin           HTTP admin/metrics endpoint (/health, /fleet, /ratelimit)
│
│ ── Backends ([lib.backends] profile only) ────
├── redis_backend    RESP2 protocol (SET/GET, ZADD, PUBLISH, HSET, EVAL)
├── postgres_backend PostgreSQL v3 wire protocol (startup, auth, query, CRUD)
│                   ⚠ plaintext transport, cleartext auth only
└── patra_queue      Durable priority queue backed by patra (survives restart)
```

## Distribution profiles

Four bundles are produced by `cyrius distlib`. Consumers pick the smallest profile that covers their needs:

| Bundle                    | Profile           | Modules            | Needs sigil? | Consumer use-case |
|---------------------------|-------------------|--------------------|--------------|-------------------|
| `dist/majra.cyr`          | `[lib]` (default) | 15 core modules    | no  | In-process concurrency primitives; no network surface |
| `dist/majra-signed.cyr`   | `[lib.signed]`    | 15 core + `signed_envelope` | **yes** | Cross-node integrity via Ed25519 signatures |
| `dist/majra-admin.cyr`    | `[lib.admin]`     | 15 core + `admin`  | no  | Operator-facing HTTP observability endpoint |
| `dist/majra-backends.cyr` | `[lib.backends]`  | 15 core + signed + admin + 4 network modules (ipc_encrypted, ws, redis_backend, postgres_backend) + patra_queue (durable, file-backed) | **yes** | Full cross-process distribution with durability |

"Needs sigil?" means majra's own code calls into it. Every `.deps` sidecar names `sigil` regardless — the core one because `sigil` sits in the declared `[deps].stdlib` list it mirrors, the admin one transitively via sandhi → tls — so a consumer provisioning from any sidecar gets it in `lib/`.

## Design Principles

1. **Zero core dependencies** — `dist/majra.cyr` pulls nothing beyond the Cyrius stdlib. Richer profiles (`signed`, `backends`) reach sigil — first-party, and since 2.6.8 a folded cyrius stdlib module rather than a git dep, no external crates.
2. **Thread-safe by default** — concurrent variants use mutex + futex.
3. **Globals for cross-call state** — cc5 is better than cc3, but deeply nested
   call chains can still clobber locals. `postgres_backend` still promotes its
   connect-path values to globals (serialised behind `_pg_connect_mtx`).
   `relay` and `barrier` **no longer do**: their file-scope globals (relay's
   inputs and dedup state, barrier's results) were removed at 2.6.0 and 2.6.9
   respectively, because a global serialised only by a per-object mutex is
   clobbered by a second object using a different lock.
   Prefer a caller-provided out-buffer.
4. **Fixed-point math** — no floating point; token buckets use x1000 scaling.
5. **Eviction where it is asked for** — the keyed collections expose TTL
   eviction (`ratelimit_evict_stale`, `sliding_window_evict_stale`,
   `relay_evict_stale_dedup`), but **the caller must schedule it**; only
   heartbeat evicts autonomously, via `eviction_cycles`. Pubsub releases
   subscribers on explicit `pubsub_unsubscribe`, or bounds them per-subscriber
   through a `PUBSUB_LAG_*` policy.
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
Producer ──► DirectChannel ──────────────────────► chan_recv     (send only ~390 ns/op, 2.7.3)
Producer ──► HashedChannel ──► topic hash lookup ► chan_recv
Producer ──► PubSub ──► exact O(1) + pattern ───► chan_recv     (publish + recv ~1.1 us/op, 2.7.3)
                                                └──► consumer accept loop ──► majra_ws_send_text ──► WebSocket clients

Producer ──► ManagedQueue ──► priority dequeue ──► Consumer
                           └──► job state lifecycle (queued → running → completed)

Node A ──► relay_send() ──────────────────────────► subscribers via channels
Node B ──► relay_receive() ──► dedup filter ──────► subscribers

FleetQueue ──► select_node (least loaded) ──► ManagedQueue on target node
            ──► rebalance() ──► steal from overloaded ──► redistribute
```

The two latencies are 5-trial medians from the 2.7.3 `cyrius bench` run: `direct_channel_send` (send only, no receive) and `pubsub_1sub_publish` (publish plus one `chan_recv`), so they are not a like-for-like comparison (see `benches/bench_all.bcyr`); point-in-time perf snapshots live in `docs/benchmarks/`.

The WebSocket leg is consumer-owned: `ws` ships framing primitives only — there is no pub/sub → WebSocket loop in majra, and `ws_bridge_new` is a configuration holder with accessors (see the `src/ws.cyr` header).

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

Box labels are abbreviated to fit: `redis_pub` / `redis_zpop` / `redis_exist` are `redis_publish` / `redis_zpopmin` / `redis_exists`; the other labels are the exact function names.

## Consumers

The consumer table (who pins which modules and profile) is live state and lives in [`docs/development/state.md` § Consumers](../development/state.md#consumers), refreshed every release. Profiles are indicative — consumers pin whatever they need, and a consumer wanting only the core can always pin `majra` regardless of what modules they reference.
