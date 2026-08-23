# Security Policy

## Scope

majra is a concurrency primitives library providing pub/sub, queues, relay,
heartbeat, rate limiting, barrier synchronisation, DAG workflows, and distributed
backends. Written in Cyrius with zero external dependencies.

### Attack Surface

| Area | Risk | Mitigation |
|------|------|------------|
| **Memory safety** | Buffer overflows, use-after-free | Manual memory via freelist (size-class isolation) and bump allocator; struct layouts documented with offsets |
| **Concurrency** | Data races, deadlocks | Mutex + futex primitives from Cyrius stdlib; single-lock-per-structure model |
| **PostgreSQL transport & auth** | Password and every query and result row cross the wire in cleartext | **No TLS, no SSLRequest, cleartext-password auth only.** Fails closed on SCRAM (auth type 10) rather than downgrading. Use ONLY over loopback or an already-confidential channel — a unix-socket proxy, a TLS-terminating sidecar, a WireGuard/VPN link. SCRAM-SHA-256 + SSLRequest is a roadmap item. |
| **Denial of service** | Unbounded memory growth | Caller-driven TTL eviction on the keyed collections (`ratelimit_evict_stale`, `sliding_window_evict_stale`, `relay_evict_stale_dedup`) — **the consumer must schedule these; nothing sweeps automatically.** Heartbeat evicts autonomously via `eviction_cycles`. Pubsub subscribers are released only by explicit `pubsub_unsubscribe`, or bounded per-subscriber by a `PUBSUB_LAG_*` policy. |
| **PostgreSQL injection** | Malformed inputs to persistence layer | Values are quoted and escaped by `_pg_add_literal` (single quotes doubled; `standard_conforming_strings` assumed on, no `E''` emitted). Simple query protocol only — no prepared statements. Fixed at 2.6.10; before that, queries were string-interpolated. |
| **Redis command injection** | Untrusted keys/values | Keys built via structured builder, not raw string concatenation |
| **IPC encryption** | Key compromise, nonce reuse | AES-256-GCM framing with monotonic nonce counter, warning at 2^31, hard error at 2^32 |
| **WebSocket bridge** | Connection exhaustion | Configurable `max_connections` limit |
| **IPC framing** | Oversized frames | 1 MB max frame size check |
| **Relay dedup** | Unbounded dedup table growth | **Unbounded by default.** Opt in via `relay_set_max_dedup` (LRU bound) and/or call `relay_evict_stale_dedup(r, max_idle_ns)` on a schedule — neither is automatic. ⚠ Bounding opens a replay window by construction: evicting a sender forgets its last-seen sequence number, so its already-delivered messages become acceptable again. Size the bound above the real peer count. |
| **Pattern matching** | Deep nesting DoS | Character-by-character scan, no recursion |
| **SHA-1 (WebSocket)** | Collision attacks | Used only for RFC 6455 handshake (not security-critical) |
| **Nonce exhaustion** | AES-GCM nonce reuse after 2^32 messages | Hard error at limit, warning at 2^31, `encrypted_ipc_rekey()` for rotation |
| **Circuit breaker** | Cascading failure from endpoint outages | Configurable failure threshold + cooldown, half-open probe, manual reset |

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 2.x (current: 2.7.0) | Yes — security fixes land on the latest 2.x patch |
| 1.x     | No (Rust implementation, archived at 2.0.0) |

Report against the latest 2.x release. Fixes are not backported to earlier 2.x
minors.

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

- Zero external dependencies — Cyrius stdlib only, no supply chain attack vector.
- All concurrent types use mutex + futex from `lib/thread.cyr`.
- All collections have eviction mechanisms to prevent unbounded growth.
- Network protocols (RESP, PostgreSQL, WebSocket) implemented from scratch.
- IPC encryption uses AES-256-GCM framing with monotonic nonces.
- Fuzz testing (`cyrius fuzz`) targets queue, pub/sub, and heartbeat.
- Compiler is self-hosting with byte-identical verification.

## Further reading

- [`docs/development/threat-model.md`](docs/development/threat-model.md) — trust
  boundaries and the full per-module attack-surface table.
- [`docs/audit/`](docs/audit/) — findings from the P(-1) hardening audits. The
  first pass (2026-08-22) confirmed 115; the second confirmed 68, of which 50
  were regressions the first pass's own repairs introduced.
- [`docs/development/semver.md`](docs/development/semver.md) — what a security
  fix may and may not change in a patch release.
