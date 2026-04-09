# Testing Guide

## Running Tests

```bash
# Full test suite
cyrius test

# Full audit (test + fmt + lint + vet + deny + bench)
cyrius audit

# Individual test suites (manual)
cyrius build src/main.cyr build/majra && ./build/majra
cyrius build tests/test_core.tcyr build/test_core && ./build/test_core
cyrius build tests/test_backends.tcyr build/test_backends && ./build/test_backends

# Live integration tests (requires Redis on :6379, PostgreSQL on :5432)
cyrius build tests/test_live.tcyr build/test_live && ./build/test_live
```

## Test Suites

| Suite | File | Assertions | Coverage |
|-------|------|-----------|----------|
| Core | `src/main.cyr` | 144 | All 15 core modules |
| Expanded | `tests/test_core.tcyr` | 92 | Deep: queue lifecycle, pubsub patterns, DAG retry, fleet routing, circuit breaker, integration tests |
| Backends | `tests/test_backends.tcyr` | 25 | base64, SHA-1, encrypted IPC nonce, WebSocket RFC 6455, RESP, PG wire |
| Live | `tests/test_live.tcyr` | 36 | 7 Redis (ping, SET/GET, SETEX, sorted sets, hash, PUBLISH, KEYS) + 4 PostgreSQL (connect, query, CRUD, workflow storage) |
| **Total** | | **295** | |

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

## Compiler Limitations

Due to the Cyrius compiler's fixup table limit (8192 forward references), tests are split across multiple compilation units. If adding significant new test code causes a "fixup table full" error, create a new test file.

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
