# Majra Roadmap

> **Principle**: Shared concurrency primitives for the AGNOS ecosystem. One implementation, many consumers.

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

---

## v1.0.0 — Stable Release

API freeze, full coverage, proven by real consumers.

### Testing & proof
- [ ] At least 2 real consumers using each module (Ifran + SecureYeoman)
- [ ] SecureYeoman: expand beyond pubsub + ratelimit to queue, heartbeat, dag
- [ ] 90%+ test coverage across all features

### Hardening (completed this session)
- [x] Relay dedup TTL eviction (`evict_stale_dedup`)
- [x] DAG `InMemoryWorkflowStorage` retention (`evict_older_than`, `max_runs`)
- [x] Auto PubSub dead subscriber cleanup (periodic on publish)
- [x] Request-response correlation on Relay (`send_request`/`reply`)
- [x] ConnectionPool stale eviction (`evict_stale`)
- [x] Bounded capacity limits (relay dedup, pubsub subscriptions, DAG runs)

---

## Post-1.0 — Consumer-Driven Features

Features identified from SecureYeoman integration analysis. These require
coordination with consumer teams and may ship incrementally.

### Multi-tenant scoping
- [ ] Namespace-keyed queue, pubsub, relay isolation
- [ ] Per-tenant metrics partitioning
- **Why**: SecureYeoman is multi-tenant; current primitives are process-global

### PostgreSQL storage backend
- [ ] `postgres` feature: `WorkflowStorage` impl for PostgreSQL
- [ ] `postgres` feature: `ManagedQueue` persistence backend
- **Why**: SecureYeoman standardised on PostgreSQL; SQLite is insufficient for production

### IPC encryption layer
- [ ] Optional AES-256-GCM or TLS envelope encryption for IPC frames
- **Why**: SecureYeoman's desktop/capture IPC requires encrypted channels

### WebSocket bridge for pubsub
- [ ] `ws` feature: bridge in-process pubsub topics to WebSocket fan-out
- **Why**: SecureYeoman has a separate WebSocket broadcast layer for dashboards; consolidating into majra pubsub removes duplication

### Distributed rate limiting
- [ ] Extend `redis-backend` to support distributed token-bucket rate limiting
- **Why**: SecureYeoman falls back to Redis sliding-window when scaling; majra should own this

### Fleet heartbeat for edge devices
- [ ] Cross-instance heartbeat via `fleet` + `redis-backend`
- **Why**: SecureYeoman Edge needs multi-instance health coordination

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
- **Message broker replacement** — majra is an in-process library, not a standalone broker (use Redis backend for cross-process)
