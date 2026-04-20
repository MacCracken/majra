# Testing Guide

## Running Tests

```bash
# Full audit (test + fmt + lint + vet + deny + bench)
cyrius audit

# Individual test suites (manual)
cyrius build src/main.cyr                build/majra            && ./build/majra
cyrius build tests/test_core.tcyr        build/test_core        && ./build/test_core
cyrius build tests/test_backends.tcyr    build/test_backends    && ./build/test_backends
cyrius build tests/test_patra_queue.tcyr build/test_patra_queue && ./build/test_patra_queue

# Live integration tests (requires Redis on :6379, PostgreSQL on :5432)
cyrius build tests/test_live.tcyr build/test_live && ./build/test_live

# Soak tests (on-demand, not in CI)
cyrius build tests/soak/soak_queue.cyr build/soak_queue && ./build/soak_queue
```

## Test Suites

| Suite | File | Assertions | Coverage |
|-------|------|-----------|----------|
| Core | `src/main.cyr` | 150 | All 15 core modules + revived relay dedup |
| Expanded | `tests/test_core.tcyr` | 96 | Deep: queue lifecycle, pubsub patterns, DAG retry, fleet routing, circuit breaker, integration, multi-threaded barrier |
| Backends | `tests/test_backends.tcyr` | 42 | base64, SHA-1, AES-256-GCM, signed envelopes, admin endpoint, WebSocket, RESP, PG wire |
| Patra queue | `tests/test_patra_queue.tcyr` | 17 | Durable enqueue / priority dequeue / complete / counts / reopen persistence |
| Live | `tests/test_live.tcyr` | 36 | 7 Redis + 4 PostgreSQL categories (see below) |
| **Total** | | **341** (305 CI + 36 live) | |

`test_patra_queue` is a separate entry point because adding it to `test_backends` blows the cc5 16384 fixup-table cap (patra pulls sakshi + io + fs transitively).

## Test Categories

| Category | Where | Count |
|----------|-------|-------|
| Error/envelope/namespace | `src/main.cyr` | ~30 |
| Queue (priority, lifecycle, cancel) | `src/main.cyr` + `tests/test_core.tcyr` | ~30 |
| PubSub (patterns, wildcards, filters) | `src/main.cyr` + `tests/test_core.tcyr` | ~25 |
| Relay (send, subscribe, routing) | `src/main.cyr` + `tests/test_core.tcyr` | ~15 |
| Barrier (sync, concurrent, force) | `src/main.cyr` + `tests/test_core.tcyr` | ~15 |
| Heartbeat (FSM, telemetry, eviction) | `src/main.cyr` + `tests/test_core.tcyr` | ~25 |
| RateLimit (bucket, window, keys) | `src/main.cyr` + `tests/test_core.tcyr` | ~15 |
| Fleet (routing, submit, rebalance) | `src/main.cyr` + `tests/test_core.tcyr` | ~10 |
| DAG (linear, parallel, retry, skip) | `src/main.cyr` + `tests/test_core.tcyr` | ~12 |
| Transport (circuit breaker, pool) | `src/main.cyr` + `tests/test_core.tcyr` | ~10 |
| Crypto/protocol (base64, SHA-1, RESP, PG) | `tests/test_backends.tcyr` | ~25 |
| Live Redis | `tests/test_live.tcyr` | ~20 |
| Live PostgreSQL | `tests/test_live.tcyr` | ~16 |
| Integration (training job, namespaced pubsub) | `tests/test_core.tcyr` | ~10 |

## Benchmarks

```bash
# Run with automatic history tracking
cyrius bench

# Manual run
cyrius build benches/bench_all.cyr build/bench && ./build/bench
```

17 benchmarks covering: envelope creation, priority queue, pattern matching (4 variants), pubsub publish, direct channel, heartbeat, fleet stats, rate limiting, relay send, barrier cycle, circuit breaker, counter increment.

## Soak tests

`tests/soak/` holds on-demand stress tests. Not in CI; run before releases or when a code change could plausibly affect a primary state machine. Each soak file is standalone: `fn main()`, returns 0 on pass, non-zero on invariant violation. See `tests/soak/README.md` for conventions and known limitations (bump-allocator-bounded iteration counts).

Currently shipped:
- `soak_queue.cyr` — 5k ops (1k rounds × 5 jobs) managed-queue lifecycle; asserts `mq_total_completed`, `mq_job_count`, and per-round `queued_count`/`running_count` invariants.

## Compiler Limitations

Cyrius 5.4.x fixup table cap is 16384 forward references (up from 8192 in cc3). Large test entry points that aggregate many src modules can still hit the cap — `test_patra_queue` is split out for this reason. If adding significant new test code causes a "fixup table full" error, create a new `.tcyr` entry point scoped to what you're testing.

## Live Test Setup

```bash
# Start Redis and PostgreSQL
docker run -d --name majra-redis -p 6379:6379 redis:7-alpine
docker run -d --name majra-postgres -p 5432:5432 \
  -e POSTGRES_PASSWORD=majra -e POSTGRES_DB=majra postgres:16-alpine

# Configure PostgreSQL for cleartext auth (required by Cyrius PG client)
docker exec majra-postgres su postgres -c \
  "echo 'host all all 0.0.0.0/0 password' > /var/lib/postgresql/data/pg_hba.conf && \
   echo 'local all all trust' >> /var/lib/postgresql/data/pg_hba.conf && \
   pg_ctl reload -D /var/lib/postgresql/data"

# Run live tests
cyrius build tests/test_live.tcyr build/test_live && ./build/test_live

# Cleanup
docker stop majra-redis majra-postgres && docker rm majra-redis majra-postgres
```
