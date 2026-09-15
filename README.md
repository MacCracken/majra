# Majra

> مجرا (Arabic/Persian: conduit, channel) — Distributed queue & multiplex engine

Majra provides shared messaging primitives for the [AGNOS](https://github.com/MacCracken) ecosystem, eliminating duplicate pub/sub, queue, relay, and heartbeat implementations across [AgnosAI](https://github.com/MacCracken/agnosai), [ifran](https://github.com/MacCracken/ifran), [SecureYeoman](https://github.com/MacCracken/secureyeoman), and [daimon](https://github.com/MacCracken/daimon).

**Written in [Cyrius](https://github.com/MacCracken/cyrius)** — compiles to a statically linked binary via `cyrius build`, for x86_64-Linux natively and for aarch64-Linux via `cyrius build --aarch64` (verified under `qemu-aarch64`; CI cross-builds the suites for aarch64 but does not yet run them there — see docs/development/state.md). Optional crypto surface (signed envelopes, encrypted IPC) pulls [sigil](https://github.com/MacCracken/sigil), a folded cyrius stdlib module — since majra 2.6.8 it is declared under `[deps].stdlib` and arrives with the toolchain snapshot rather than as a separate git dep. The core profile has no crypto surface at all, and majra declares **zero git dependencies**.

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
| **ws** | WebSocket framing primitives (RFC 6455) — SHA-1 upgrade handshake, frame read/write, ping/pong/close. **No pub/sub bridge**: drive `majra_ws_recv_frame` / `majra_ws_send_text` from your own accept loop |
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
majra (v2.7.3, ~7,500 lines across 22 modules; 8,212 across 23 files counting the src/main.cyr self-test entry — see docs/development/state.md)
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

# Full test matrix — 637 assertions at 2.7.3.
# Counts move every release; docs/development/state.md carries the current ones.
cyrius build --no-deps tests/test_core.tcyr        build/test_core        && ./build/test_core
cyrius build --no-deps tests/test_backends.tcyr    build/test_backends    && ./build/test_backends
cyrius build --no-deps tests/test_patra_queue.tcyr build/test_patra_queue && ./build/test_patra_queue

# aarch64-Linux: cross-build any entry point the same way and run it under
# qemu-aarch64 (full recipe in docs/guides/testing.md; CI cross-builds the
# suites for aarch64 but the run-under-qemu lane is not wired yet)
cyrius build --aarch64 --no-deps src/main.cyr build/majra_aarch64 && qemu-aarch64 ./build/majra_aarch64

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

`cyrius deps` resolves the tag and copies the chosen bundle into `lib/` under its own name — `lib/majra.cyr`, `lib/majra-signed.cyr`, `lib/majra-admin.cyr` or `lib/majra-backends.cyr`. A plain `cyrius build` prepends it (and the sidecar leaves) to your entry point automatically; an entry point built with `--no-deps` `include`s it by hand.

The bundles are **pure `src/` concatenation** — they carry no `include "lib/…"` lines of their own. A consumer that lets `cyrius build` resolve `[deps.majra]` gets the sidecar leaves and the bundle prepended automatically; a consumer that hand-includes a bundle under `--no-deps` must supply every stdlib module the bundle (and sigil) reaches into itself, in sidecar order. The `.deps` sidecar next to each bundle lists that set in full — majra's own leaves plus what sigil, sandhi and patra pull in for the richer profiles. These sets were verified at 2.7.3 (as at 2.6.8) by building a clean consumer against each shipped bundle — sidecar leaves in order, then the bundle — and all four build with no undefined function and run:

> **Sidecar note (fixed at 2.6.8).** Through 2.6.7 the `majra-signed` and `majra-backends` sidecars omitted `sigil` itself, because declaring it as a git dep made `cyrius distlib` classify it out of the stdlib leaves. A consumer that provisioned strictly from the sidecar got undefined `ed25519_*` — and since an undefined fn lowers to a trapping `ud2`, the build reported `OK` and the process SIGILLed at first use. Both sidecars name `sigil` from 2.6.8 on. If you pinned `2.6.7` or earlier, add `sigil` to your own include set.

| Profile | sibling dep | stdlib modules the consumer must include (leaf counts per profile: docs/development/state.md) |
|---|---|---|
| `majra` (core) | — | the `.deps` sidecar set (mirrors `[deps].stdlib`, incl. `chrono` and `sys`) |
| `majra-admin` | — | the `.deps` sidecar set (adds `sandhi`, `tls`, `sakshi` and what they reach) |
| `majra-signed` | sigil ≥ 3.12.9 (stdlib fold) | the `.deps` sidecar set (adds `sigil` and what it reaches: `sys`, `ct`, `bayan`, `io`, `random`, `thread_local`, `keccak`) |
| `majra-backends` | sigil ≥ 3.12.9 (stdlib fold) | the `.deps` sidecar set (adds `sigil`, `sandhi`, `tls` and `patra`) |

The "adds" parentheticals are measured against what the core engine's own code reaches, not against the core sidecar — that one carries the full `[deps].stdlib` set (the same 27 names, reordered by distlib) and so already names `sigil`, `patra`, `tls`, `sandhi` and `sakshi` the core never calls; `majra-admin` also picks up `sigil` transitively through `tls`.

> **Hand-included consumers (since 2.7.3).** The core bundle itself now reaches into `lib/chrono.cyr` — `time_now_ns`, `time_epoch_ns` and majra's internal sleep primitive delegate to it. A consumer provisioning from the sidecar (`cyrius deps` with `modules = ["dist/majra.cyr"]`) needs nothing: every sidecar has listed `chrono` since 2.7.1. A consumer that hand-includes the bundle under `--no-deps` must `include "lib/chrono.cyr"` after `lib/syscalls.cyr` and before the bundle, or the build is refused (`refusing to emit binary with N reachable undefined function(s)`, after `undefined function 'clock_now_ns'` / `'clock_epoch_ns'` / `'sleep_ms'` warnings — which of the three are reachable depends on what the consumer calls).

> **Toolchain floor for the crypto profiles: cyrius ≥ 6.4.64.** sigil 3.12.x allocates its crypto-bank thread-local slot dynamically via `thread_local_alloc()`, which first appears in the 6.4.64 stdlib snapshot (`TLOCAL_MAX_SLOTS` 16 → 128); an older snapshot fails the build with `refusing to emit binary with N reachable undefined function(s)`. sigil's own source comment says "requires cyrius >= 6.4.65" — 6.4.64 is where the symbol actually lands, so 6.4.65 is the conservative-safe floor. `lib/thread_local.cyr` was already required at sigil 3.11.1 (for `thread_local_{init,get,set}`). Since sigil now rides the stdlib snapshot, a consumer on cyrius ≥ 6.5.x satisfies this floor automatically.

There is no separate sigil profile to pull for a `signed`-only consumer: sigil rides the stdlib snapshot (`lib/sigil.cyr`, named by the signed, admin and backends sidecars), and adding a `[deps.sigil]` git dep to slim it reintroduces the through-2.6.7 sidecar problem above — `distlib` reclassifies the module out of the stdlib leaves and drops it from the `.deps` sidecars.

## Ecosystem

| Consumer | Modules used |
|----------|-------------|
| **daimon** | pubsub, relay, ipc |
| **AgnosAI** | pubsub, queue, relay, barrier |
| **hoosh** | queue, heartbeat, fleet |
| **sutra** | heartbeat, fleet, dag |
| **stiva** | dag, heartbeat, ipc |
| **ifran** | queue, pubsub, heartbeat, fleet |
| **secureyeoman** | pubsub, ratelimit, heartbeat, queue, dag, namespace, relay, ipc_encrypted (per docs/guides/migration-secureyeoman.md) |

## Ported from Rust

Majra was originally a Rust library (v1.0.4, ~13,000 lines). It was ported to Cyrius via `cyrius port`, re-implementing all modules from scratch.

| Metric | Rust v1.0.4 | Cyrius v2.7.x |
|--------|-------------|---------------|
| Source lines | 12,969 | 8,212 (all of `src/`, incl. the 680-line `src/main.cyr` test entry; 7,532 in the 22 modules) |
| Modules | 22 | 22 (QUIC transport not ported — deferred until a consumer needs multiplexed streams or connection migration; see roadmap § QUIC transport) |
| Dependencies | 25 crates | 0 — sigil is a folded stdlib module |
| Toolchain | cargo + rustc + LLVM | cyrius 6.6.4 |

## License

GPL-3.0-only
