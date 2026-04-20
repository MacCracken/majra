# Majra

> مجرا (Arabic/Persian: conduit, channel) — Distributed queue & multiplex engine

Majra provides shared messaging primitives for the [AGNOS](https://github.com/MacCracken) ecosystem, eliminating duplicate pub/sub, queue, relay, and heartbeat implementations across [AgnosAI](https://github.com/MacCracken/agnosai), [Ifran](https://github.com/MacCracken/synapse), [SecureYeoman](https://github.com/MacCracken/secureyeoman), and [daimon](https://github.com/agnostos/daimon).

**Written in [Cyrius](https://github.com/MacCracken/cyrius)** — compiles to a statically linked binary via `cyrius build`. Optional crypto surface (signed envelopes, encrypted IPC) pulls [sigil](https://github.com/MacCracken/sigil) ≥ 2.9.0; the core profile has no external deps.

## Modules

| Module | Description |
|--------|-------------|
| **pubsub** | Three-tier pub/sub: DirectChannel, HashedChannel, PubSub with MQTT wildcard matching |
| **queue** | Multi-tier priority queue + ManagedQueue with job lifecycle management |
| **relay** | Sequenced, deduplicated relay with broadcast and request-response |
| **transport** | Transport vtable + connection pool with circuit breaker |
| **ipc** | Length-prefixed framing over Unix domain sockets |
| **ipc_encrypted** | AES-256-GCM encrypted IPC with key rotation and nonce tracking |
| **heartbeat** | TTL-based node health: Online / Suspect / Offline with GPU telemetry and fleet stats |
| **ratelimit** | Token bucket + sliding window rate limiters (fixed-point math) |
| **barrier** | N-way barrier synchronisation with deadlock recovery |
| **dag** | DAG workflow engine with tier-based scheduling, retry, error policies |
| **fleet** | Distributed job queue with work-stealing across nodes |
| **namespace** | Multi-tenant scoping for topics, keys, and node IDs |
| **metrics** | Pluggable metrics vtable with 22 hook points |
| **redis_backend** | Cross-process pub/sub, sorted-set queues, hash-based rate limiter, heartbeat via RESP protocol |
| **postgres_backend** | PostgreSQL workflow + queue storage via wire protocol v3 |
| **ws** | WebSocket bridge — SHA-1 handshake, frame parsing, pub/sub fan-out |
| **signed_envelope** | Ed25519 signatures over a canonical envelope encoding (via sigil) |
| **admin** | Read-only HTTP admin/metrics endpoint (`/health`, `/fleet`, `/ratelimit`) |
| **patra_queue** | Durable job queue backed by patra — survives process restart |

## Quick Start

```cyrius
include "lib/alloc.cyr"
include "lib/freelist.cyr"
include "src/pubsub.cyr"

fn main() {
    alloc_init();
    fl_init();

    # Create a pub/sub hub
    var ps = pubsub_new();

    # Subscribe to a topic
    var ch = pubsub_subscribe(ps, "events/created");

    # Publish a message
    pubsub_publish(ps, "events/created", 42);

    # Receive
    var msg = chan_recv(ch);
    return 0;
}
```

### Managed Queue with Priority

```cyrius
var mq = mq_new("training-jobs", 4);

# Enqueue with priority
mq_enqueue(mq, PRIORITY_CRITICAL, job_data_1);
mq_enqueue(mq, PRIORITY_NORMAL, job_data_2);

# Dequeue (highest priority first)
var job = mq_dequeue(mq);
# ... process job ...
mq_complete(mq, job);
```

### Multi-Tenant Isolation

```cyrius
var ns = namespace_new("tenant-42");

# Scoped topics
var topic = namespace_topic(ns, "events/created");
pubsub_publish(ps, str_data(topic), payload);

# Scoped rate limiting
ratelimit_check(rl, str_data(namespace_key(ns, "api")));
```

### Redis Backend

```cyrius
var rc = redis_connect_default();
redis_set_prefix(rc, "majra:");

redis_set(rc, "key", "value");
var v = redis_get(rc, "key");

# Sorted-set queue
redis_zadd(rc, "queue:jobs", "job-data", -priority);
var popped = redis_zpopmin(rc, "queue:jobs");
```

### PostgreSQL Workflow Storage

```cyrius
var conn = pg_connect("127.0.0.1", 5432, "postgres", "majra", "password");
pg_init_workflow_tables(conn);
pg_save_workflow_def(conn, "wf-1", "my workflow", "[]");
```

## Architecture

```
majra (v2.4.x, ~5,500 lines across 22 modules)
│
│ ── Core ──────────────────────────────────────
├── error           Error codes + result helpers
├── counter         Mutex-protected atomic counter
├── envelope        Universal message envelope (UUID, routing, payload)
├── namespace       Multi-tenant scoping (topic, key, node_id prefixing)
├── metrics         22-slot function pointer vtable for observability
│
│ ── Primitives ────────────────────────────────
├── queue           5-tier priority queue + managed lifecycle
├── pubsub          MQTT wildcard matching + DirectChannel + HashedChannel
├── relay           Sequenced dedup relay with broadcast
├── barrier         N-way barrier (sync + concurrent with futex)
├── heartbeat       FSM health tracker + GPU telemetry + fleet stats
├── ratelimit       Token bucket + sliding window (fixed-point)
│
│ ── Networking ────────────────────────────────
├── ipc             Unix domain socket framing (4-byte BE length prefix)
├── ipc_encrypted   AES-256-GCM framing with nonce management (via sigil)
├── transport       Transport vtable + circuit breaker + connection pool
├── ws              WebSocket (SHA-1 handshake, RFC 6455 framing)
│
│ ── Composition ───────────────────────────────
├── fleet           Distributed job queue with work-stealing
├── dag             DAG workflow engine (Kahn's sort, retry, error policies)
│
│ ── Backends ──────────────────────────────────
├── redis_backend    RESP2 protocol (SET/GET, ZADD/ZPOPMIN, PUBLISH, HSET, EVAL)
├── postgres_backend PostgreSQL v3 wire protocol (startup, auth, query, CRUD)
└── patra_queue      Durable priority queue backed by patra (survives restart)
│
│ ── Trust ─────────────────────────────────────
├── signed_envelope Ed25519 signatures over canonical envelope encoding (via sigil)
│
│ ── Operations ────────────────────────────────
└── admin           HTTP admin/metrics endpoint (/health, /fleet, /ratelimit)
```

## Building

```bash
# Compile (core engine)
cyrius build src/main.cyr build/majra

# Run core tests
./build/majra

# Full test matrix (150 + 96 + 42 + 17 = 305 assertions)
cyrius build tests/test_core.tcyr        build/test_core        && ./build/test_core
cyrius build tests/test_backends.tcyr    build/test_backends    && ./build/test_backends
cyrius build tests/test_patra_queue.tcyr build/test_patra_queue && ./build/test_patra_queue

# Benchmarks
cyrius build benches/bench_all.bcyr build/bench_all && ./build/bench_all

# Soak tests (on-demand, not in CI)
cyrius build tests/soak/soak_queue.cyr build/soak_queue && ./build/soak_queue

# Full audit: self-host, test, fmt, lint, vet, deny, bench
cyrius audit

# Regenerate all four distribution bundles (commit alongside src/ changes)
cyrius distlib          # → dist/majra.cyr           (core engine, 15 modules)
cyrius distlib signed   # → dist/majra-signed.cyr    (+ signed_envelope)
cyrius distlib admin    # → dist/majra-admin.cyr     (+ admin endpoint)
cyrius distlib backends # → dist/majra-backends.cyr  (everything: signed + admin + redis/pg/ws/encrypted IPC + patra_queue)
```

## Using majra as a dependency

Downstream Cyrius projects wire majra into their `cyrius.cyml`:

```toml
[deps.majra]
git = "https://github.com/MacCracken/majra.git"
tag = "<majra version>"
modules = ["dist/majra.cyr"]           # core engine only — lean, no crypto
# or pick a richer profile:
modules = ["dist/majra-signed.cyr"]    # core + Ed25519-signed envelopes (pulls sigil)
modules = ["dist/majra-admin.cyr"]     # core + HTTP admin/metrics endpoint
modules = ["dist/majra-backends.cyr"]  # everything: all profiles + redis/pg/ws/encrypted IPC/patra_queue
```

`cyrius deps` resolves the tag, copies the chosen bundle into `lib/majra_majra.cyr`, and you `include` it from your entry point. `majra-signed` and `majra-backends` require sigil ≥ 2.9.0 as a sibling dep.

## Ecosystem

| Consumer | Modules used |
|----------|-------------|
| **daimon** | pubsub, relay, ipc |
| **AgnosAI** | pubsub, queue, relay, barrier |
| **hoosh** | queue, heartbeat, fleet |
| **sutra** | heartbeat, fleet, dag |
| **stiva** | dag, heartbeat, ipc |

## Ported from Rust

Majra was originally a Rust library (v1.0.4, ~13,000 lines). It was ported to Cyrius via `cyrius port`, re-implementing all modules from scratch.

| Metric | Rust v1.0.4 | Cyrius v2.4.x |
|--------|-------------|---------------|
| Source lines | 12,969 | ~5,500 |
| Modules | 22 | 22 (QUIC deferred on sigil X25519) |
| Dependencies | 25 crates | 0 core, 1 optional (sigil) |
| Toolchain | cargo + rustc + LLVM | cyrius 5.4.17 |

## License

GPL-3.0-only
