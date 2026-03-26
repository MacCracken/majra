# Security Policy

## Scope

majra is a concurrency primitives library providing pub/sub, queues, relay,
heartbeat, rate limiting, barrier synchronisation, DAG workflows, and distributed
backends. It is a pure Rust library with no C FFI and no `unsafe` code.

### Attack Surface

| Area | Risk | Mitigation |
|------|------|------------|
| **Concurrency** | Data races, deadlocks, unsound `Send`/`Sync` | DashMap + tokio::sync primitives; compile-time `Send + Sync` assertions on all public types |
| **Denial of service** | Unbounded memory growth in rate limiter, queue, pub/sub, relay dedup | TTL eviction on all collections (`evict_stale_*`), configurable capacity limits (`max_dedup_entries`, `max_subscriptions`, `max_runs`) |
| **SQLite injection** | Malformed inputs to persistence layer | Parameterised queries exclusively |
| **PostgreSQL injection** | Malformed inputs to persistence layer | Parameterised queries via `tokio-postgres` |
| **Redis command injection** | Untrusted keys/values | Keys are prefixed strings; rate limiter uses atomic Lua script |
| **IPC encryption** | Key compromise, nonce reuse | AES-256-GCM via `ring`, monotonic nonce counter per direction, pre-shared key model |
| **WebSocket bridge** | Connection exhaustion | Configurable `max_connections` limit, connection counting |
| **Serialisation** | Untrusted `serde_json` payloads in envelopes, relay, IPC | Size limits on IPC frames (16 MiB max), no arbitrary code execution |
| **Relay dedup** | Unbounded dedup table growth | TTL eviction + configurable max entries |
| **Pending requests** | Unbounded correlation map growth | TTL eviction via `evict_stale_requests()` |
| **Circuit breaker** | Cascading failure from endpoint outages | Configurable failure threshold + cooldown, half-open probe, manual reset |
| **Sliding window approximation** | ~5% accuracy loss vs exact counting | Documented tradeoff; token bucket available for burst-tolerant use cases |
| **Nonce exhaustion** | AES-GCM nonce reuse after 2^32 messages | Hard error at limit, warning at 2^31, `rekey()` API for key rotation |
| **HashedChannel collisions** | Topic hash collision (u64) routes to wrong subscriber | Probability ~1 in 2^64; use TypedPubSub if collision-free routing required |

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 1.0.x   | Yes       |
| < 1.0   | No        |

## Reporting a Vulnerability

If you discover a security vulnerability in majra, please report it
responsibly:

1. **Email** [security@agnos.dev](mailto:security@agnos.dev) with a description
   of the issue, steps to reproduce, and any relevant context.
2. **Do not** open a public issue for security vulnerabilities.
3. You will receive an acknowledgment within **48 hours**.
4. We follow a **90-day disclosure timeline**. We will work with you to
   coordinate public disclosure after a fix is available.

## Response Timeline

| Severity | Target Fix |
|----------|-----------|
| Critical | 14 days |
| High | 30 days |
| Moderate/Low | Next release |

## Security Design Principles

- No `unsafe` code (compile-time enforced with `Send + Sync` assertions).
- All concurrent types use `DashMap` or `tokio::sync` — no raw atomics or lock-free structures.
- All collections have eviction mechanisms to prevent unbounded growth.
- SQLite and PostgreSQL backends use parameterised queries exclusively.
- Redis operations use atomic Lua scripts where race conditions would be possible.
- IPC encryption uses AES-256-GCM with monotonic nonces (no nonce reuse).
- Fuzz testing (`make fuzz`) targets serialisation and queue state transitions.
- `cargo-deny` and `cargo-audit` run in CI for supply-chain security.
