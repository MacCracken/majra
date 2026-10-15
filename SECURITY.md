# Security Policy

## Scope

majra is a concurrency primitives library providing pub/sub, queues, relay,
heartbeat, rate limiting, barrier synchronisation, DAG workflows, and distributed
backends. Written in Cyrius with zero external dependencies.

### Attack Surface

| Area | Risk | Mitigation |
|------|------|------------|
| **Memory safety** | Buffer overflows, use-after-free | Manual memory via freelist (size-class isolation) and bump allocator; struct layouts documented with offsets |
| **Concurrency** | Data races, deadlocks | Mutex + futex primitives from the Cyrius stdlib, one lock per structure; atomics for counters and the shared relay-message refcount (2.10.1) |
| **PostgreSQL transport & auth** | Password and every query and result row cross the wire in cleartext | **No TLS, no SSLRequest, cleartext-password auth only.** Fails closed on SCRAM (auth type 10) rather than downgrading. Use ONLY over loopback or an already-confidential channel — a unix-socket proxy, a TLS-terminating sidecar, a WireGuard/VPN link. SCRAM-SHA-256 + SSLRequest is a roadmap item. |
| **Denial of service** | Unbounded memory growth | Admin requests allocate from a per-handle arena reset after each one (2.10.1). Caller-driven TTL eviction on the keyed collections (`ratelimit_evict_stale`, `sliding_window_evict_stale`, `relay_evict_stale_dedup`), and since 2.12.0 a hard key cap for the two limiters (`ratelimit_evict_to`, `sliding_window_evict_to`) that holds under key-spraying, where TTL eviction cannot — **the consumer must schedule these; nothing sweeps automatically.** Heartbeat evicts autonomously via `eviction_cycles`. Pubsub subscribers are released only by explicit `pubsub_unsubscribe`, or bounded per-subscriber by a `PUBSUB_LAG_*` policy. |
| **PostgreSQL injection** | Malformed inputs to persistence layer | Values are **dollar-quoted** by `_pg_add_literal` (`$m_$…$m_$`, the tag one `_` longer than the longest `_` run in the value), so `standard_conforming_strings` no longer matters (2.8.1; it doubled single quotes before, and before 2.6.10 queries were string-interpolated). Simple query protocol only — no prepared statements. |
| **Redis command injection** | Untrusted keys/values | Commands are RESP arrays built per argument, never string-concatenated |
| **Redis replies** | A malicious or broken server | Depth (8), array (1M) and bulk (64 MiB) caps; strict integer fields (2.10.1); any violation poisons the connection instead of desyncing it |
| **IPC encryption** | Key compromise, nonce reuse, replay | AES-256-GCM under a per-connection key derived from a 32-byte session handshake (2.9.0 — captured frames do not replay into another connection); role-split, salted nonce counters with in-order replay rejection; rekey at a frame or volume limit |
| **WebSocket framing** | Oversized or malformed frames, cross-site upgrades | 1 MiB payload cap, control frames ≤ 125 bytes and unfragmented, client masking required; optional Origin policy (`ws_set_origin_check`, 2.9.0). There is **no** pub/sub bridge loop and no handshake idle timeout — the consumer's accept loop owns connection limits and `SO_RCVTIMEO` |
| **IPC framing** | Oversized frames | 1 MB max frame size check |
| **Relay dedup** | Unbounded dedup table growth | **Unbounded by default.** Opt in via `relay_set_max_dedup` (LRU bound) and/or call `relay_evict_stale_dedup(r, max_idle_ns)` on a schedule — neither is automatic. ⚠ Bounding opens a replay window by construction: evicting a sender forgets its last-seen sequence number, so its already-delivered messages become acceptable again. Size the bound above the real peer count. |
| **Pattern matching** | Deep nesting DoS | Character-by-character scan, no recursion |
| **SHA-1 (WebSocket)** | Collision attacks | Used only for RFC 6455 handshake (not security-critical) |
| **Nonce exhaustion** | AES-GCM nonce reuse | Hard error at 2^32 frames or 1 TiB per key, warning at 2^31 / 256 GiB; `encrypted_ipc_rekey()` rotates, refusing the installed key and drawing a fresh nonce salt, so even a reinstalled key never repeats a (key, nonce) pair |
| **Circuit breaker** | Cascading failure from endpoint outages | Configurable failure threshold + cooldown, half-open probe, manual reset |

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 2.x (current: see `VERSION`) | Yes — security fixes land on the latest 2.x patch |
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
- Concurrent types use the stdlib mutex (futex-backed) and atomics.
- Keyed collections that grow per client have caller-driven eviction; every block the API hands out has a release function (2.11.0).
- Network protocols (RESP, PostgreSQL, WebSocket) implemented from scratch.
- IPC encryption uses AES-256-GCM framing with monotonic nonces.
- Fuzz testing (`cyrius fuzz`) targets queue, pub/sub, and heartbeat.
- Compiler is self-hosting with byte-identical verification.

## Further reading

- [`docs/development/threat-model.md`](docs/development/threat-model.md) — trust
  boundaries and the full per-module attack-surface table.
- [`docs/audit/`](docs/audit/) — the four P(-1) hardening audits: 2026-08-22
  (115 findings) and its second pass (68, 50 of them regressions the first
  pass's repairs introduced), 2026-09-15 (130), and 2026-10-15 (12, after two
  toolchain language bumps).
- [`docs/development/semver.md`](docs/development/semver.md) — what a security
  fix may and may not change in a patch release.
