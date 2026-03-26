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

### Hardening (done)
- [x] Relay dedup TTL eviction (`evict_stale_dedup`)
- [x] DAG `InMemoryWorkflowStorage` retention (`evict_older_than`, `max_runs`)
- [x] Auto PubSub dead subscriber cleanup (periodic on publish)
- [x] Request-response correlation on Relay (`send_request`/`reply`)
- [x] ConnectionPool stale eviction (`evict_stale`)
- [x] Bounded capacity limits (relay dedup, pubsub subscriptions, DAG runs)

### Consumer-driven features (done)
- [x] Multi-tenant scoping — `Namespace` module for topic/key/node-ID prefixing
- [x] PostgreSQL storage backend — `postgres` feature with `PostgresWorkflowStorage`
- [x] IPC encryption — `ipc-encrypted` feature with AES-256-GCM via `ring`
- [x] WebSocket bridge for pubsub — `ws` feature with `WsBridge`
- [x] Distributed rate limiting — `RedisRateLimiter` with atomic Lua script
- [x] Fleet heartbeat for edge — `RedisHeartbeatTracker` with TTL-based keys

### Remaining for 1.0
- [ ] `ManagedQueue` PostgreSQL persistence backend (workflow storage done, queue storage pending)
- [ ] Per-tenant metrics partitioning (namespace module provides the scoping, metrics integration pending)
- [ ] Integration tests against live PostgreSQL and Redis
- [ ] Migration guide for SecureYeoman consumers

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
- **Message broker replacement** — majra is an in-process library, not a standalone broker (use Redis backend for cross-process)
