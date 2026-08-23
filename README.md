# Majra

> مجرا (Arabic/Persian: conduit, channel) — Distributed queue & multiplex engine

Majra provides shared messaging primitives for the [AGNOS](https://github.com/MacCracken) ecosystem, eliminating duplicate pub/sub, queue, relay, and heartbeat implementations across [AgnosAI](https://github.com/MacCracken/agnosai), [ifran](https://github.com/MacCracken/ifran), [SecureYeoman](https://github.com/MacCracken/secureyeoman), and [daimon](https://github.com/agnostos/daimon).

**Written in [Cyrius](https://github.com/MacCracken/cyrius)** — compiles to a statically linked binary via `cyrius build`. Optional crypto surface (signed envelopes, encrypted IPC) pulls [sigil](https://github.com/MacCracken/sigil), a folded cyrius stdlib module — since majra 2.6.8 it is declared under `[deps].stdlib` and arrives with the toolchain snapshot rather than as a separate git dep. The core profile has no crypto surface at all, and majra declares **zero git dependencies**.

## Modules

| Module | Description |
|--------|-------------|
| **pubsub** | Three-tier pub/sub: DirectChannel, HashedChannel, PubSub with MQTT wildcard matching. Unsubscribe + per-subscriber lag policy (`PUBSUB_LAG_BLOCK` is the default — a stalled subscriber blocks publishes to its own topic) |
| **queue** | Multi-tier priority queue + ManagedQueue with job lifecycle management |
| **relay** | Sequenced, deduplicated relay — unicast (`relay_send`) and broadcast (`relay_broadcast`) |
| **transport** | Transport vtable + connection pool with circuit breaker |
| **ipc** | Length-prefixed framing over Unix domain sockets |
| **ipc_encrypted** | AES-256-GCM encrypted IPC with key rotation and nonce tracking |
| **heartbeat** | TTL-based node health: Online / Suspect / Offline with GPU telemetry and fleet stats |
| **ratelimit** | Token bucket + sliding window rate limiters (fixed-point math) |
| **barrier** | N-way barrier synchronisation with deadlock recovery |
| **dag** | DAG workflow engine — tier-based scheduling, retry, error policies. Tiers run **serially by default**; `workflow_def_set_parallel` opts in (break-even is a ~89µs step, and your executor must be thread-safe) |
| **fleet** | Distributed job queue with work-stealing across nodes |
| **namespace** | Multi-tenant scoping for topics, keys, and node IDs |
| **metrics** | Pluggable metrics vtable with 22 hook points |
| **redis_backend** | Cross-process pub/sub, sorted-set queues, hash-based rate limiter, heartbeat via RESP protocol |
| **postgres_backend** | PostgreSQL workflow + queue storage via wire protocol v3 |
| **ws** | WebSocket framing primitives (RFC 6455) — SHA-1 upgrade handshake, frame read/write, ping/pong/close. **No pub/sub bridge**: drive `ws_recv_frame` / `ws_send_text` from your own accept loop |
| **signed_envelope** | Ed25519 signatures over a canonical envelope encoding (via sigil) |
| **admin** | Read-only HTTP admin/metrics endpoint (`/health`, `/fleet`, `/ratelimit`) |
| **patra_queue** | Durable job queue backed by patra — survives process restart |

## Quick Start

```cyrius
# The bundles carry no `include "lib/…"` lines of their own, so the entry
# point supplies every stdlib module the code reaches into. This is the
# minimum set for pubsub.
include "lib/string.cyr"
include "lib/fmt.cyr"
include "lib/alloc.cyr"
include "lib/freelist.cyr"
include "lib/vec.cyr"
include "lib/str.cyr"
include "lib/hashmap.cyr"
include "lib/syscalls.cyr"
include "lib/tagged.cyr"
include "lib/thread.cyr"

include "src/error.cyr"
include "src/counter.cyr"
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
    if (ns == 0) { return 1; }   # refused: prefix held / : # + or a control byte

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
majra (v2.7.0, ~8,100 lines across 22 modules)
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
# One-time setup (cyrius 6.x): stdlib snapshot, then the lockfile.
# majra declares ZERO git deps — `cyrius deps` resolves nothing, but it is what
# writes and verifies cyrius.lock.
# `--full` is load-bearing since 6.4.x — the bare form copies only the
# declared [deps].stdlib subset and omits the toolchain modules sigil and
# sandhi reach into.
cyrius lib sync --full && cyrius deps

# Compile (core engine) — --no-deps keeps the lib-synced ./lib/ intact
cyrius build --no-deps src/main.cyr build/majra

# Run core tests
./build/majra

# Full test matrix — 629 assertions at 2.7.0.
# Counts move every release; docs/development/state.md carries the current ones.
cyrius build --no-deps tests/test_core.tcyr        build/test_core        && ./build/test_core
cyrius build --no-deps tests/test_backends.tcyr    build/test_backends    && ./build/test_backends
cyrius build --no-deps tests/test_patra_queue.tcyr build/test_patra_queue && ./build/test_patra_queue

# Benchmarks
cyrius build --no-deps benches/bench_all.bcyr build/bench_all && ./build/bench_all

# Soak tests (on-demand, not in CI)
cyrius build --no-deps tests/soak/soak_queue.cyr build/soak_queue && ./build/soak_queue

# Project sweep: fmt, lint, docs, tests, bench
# (the syscall/network policy check is separate: cyrius deny src/main.cyr)
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
modules = ["dist/majra.cyr"]             # core engine only — lean, no crypto
# or pick exactly ONE richer profile instead:
# modules = ["dist/majra-signed.cyr"]    # core + Ed25519-signed envelopes (pulls sigil)
# modules = ["dist/majra-admin.cyr"]     # core + HTTP admin/metrics endpoint
# modules = ["dist/majra-backends.cyr"]  # everything: signed + admin + redis/pg/ws/encrypted IPC/patra_queue
```

`cyrius deps` resolves the tag and copies the chosen bundle into `lib/` under its own name — `lib/majra.cyr`, `lib/majra-signed.cyr`, `lib/majra-admin.cyr` or `lib/majra-backends.cyr` — which you then `include` from your entry point.

The bundles are **pure `src/` concatenation** — they carry no `include "lib/…"` lines of their own, so the consumer's entry point must supply every stdlib module the bundle (and sigil) reaches into. The `.deps` sidecar next to each bundle lists what *majra's own code* needs; the crypto profiles need more than that, because sigil reaches into the stdlib too. These sets are verified by building a clean consumer against each shipped bundle:

> **Sidecar note (fixed at 2.6.8).** Through 2.6.7 the `majra-signed` and `majra-backends` sidecars omitted `sigil` itself, because declaring it as a git dep made `cyrius distlib` classify it out of the stdlib leaves. A consumer that provisioned strictly from the sidecar got undefined `ed25519_*` — and since an undefined fn lowers to a trapping `ud2`, the build reported `OK` and the process SIGILLed at first use. Both sidecars name `sigil` from 2.6.8 on. If you pinned `2.6.7` or earlier, add `sigil` to your own include set.

| Profile | sibling dep | stdlib modules the consumer must include |
|---|---|---|
| `majra` (core) | — | the `.deps` sidecar set |
| `majra-admin` | — | sidecar + `net`, `io`, `chrono`, `async`, `dynlib`, `fdlopen`, `sakshi`, `random`, then `tls` **before** `sandhi` (sandhi reads `TLS_BACKEND_LIBSSL` at parse time) |
| `majra-signed` | sigil ≥ 3.12.9 (stdlib fold) | sidecar (now incl. `sigil`) + `thread_local`, `io`, `fs`, `chrono`, `bayan`, `ct`, `keccak`, `random` |
| `majra-backends` | sigil ≥ 3.12.9 (stdlib fold) | the signed set + `net`, `async`, `sakshi`, `dynlib`, `fdlopen`, `tls`, `sandhi`, `patra` |

> **Toolchain floor for the crypto profiles: cyrius ≥ 6.4.64.** sigil 3.12.x allocates its crypto-bank thread-local slot dynamically via `thread_local_alloc()`, which first appears in the 6.4.64 stdlib snapshot (`TLOCAL_MAX_SLOTS` 16 → 128); an older snapshot fails the build with `refusing to emit binary with N reachable undefined function(s)`. sigil's own source comment says "requires cyrius >= 6.4.65" — 6.4.64 is where the symbol actually lands, so 6.4.65 is the conservative-safe floor. `lib/thread_local.cyr` was already required at sigil 3.11.1 (for `thread_local_{init,get,set}`). Since sigil now rides the stdlib snapshot, a consumer on cyrius ≥ 6.5.x satisfies this floor automatically.

A `signed`-only consumer can pull sigil's per-primitive `dist/sigil-ed25519.cyr` profile (~2k lines) instead of the full crypto bundle.

## Ecosystem

| Consumer | Modules used |
|----------|-------------|
| **daimon** | pubsub, relay, ipc |
| **AgnosAI** | pubsub, queue, relay, barrier |
| **hoosh** | queue, heartbeat, fleet |
| **sutra** | heartbeat, fleet, dag |
| **stiva** | dag, heartbeat, ipc |
| **ifran** | queue, pubsub, heartbeat, fleet |
| **secureyeoman** | signed_envelope, ipc |

## Ported from Rust

Majra was originally a Rust library (v1.0.4, ~13,000 lines). It was ported to Cyrius via `cyrius port`, re-implementing all modules from scratch.

| Metric | Rust v1.0.4 | Cyrius v2.7.x |
|--------|-------------|---------------|
| Source lines | 12,969 | 8,093 |
| Modules | 22 | 22 (QUIC deferred on sigil X25519) |
| Dependencies | 25 crates | 0 — sigil is a folded stdlib module |
| Toolchain | cargo + rustc + LLVM | cyrius 6.5.35 |

## License

GPL-3.0-only
