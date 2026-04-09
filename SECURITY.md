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
| **Denial of service** | Unbounded memory growth | TTL eviction on all collections, configurable capacity limits |
| **PostgreSQL injection** | Malformed inputs to persistence layer | String-interpolated queries — consumers must sanitize inputs |
| **Redis command injection** | Untrusted keys/values | Keys built via structured builder, not raw string concatenation |
| **IPC encryption** | Key compromise, nonce reuse | AES-256-GCM framing with monotonic nonce counter, warning at 2^31, hard error at 2^32 |
| **WebSocket bridge** | Connection exhaustion | Configurable `max_connections` limit |
| **IPC framing** | Oversized frames | 1 MB max frame size check |
| **Relay dedup** | Unbounded dedup table growth | TTL eviction + configurable max entries via `relay_set_max_dedup` |
| **Pattern matching** | Deep nesting DoS | Character-by-character scan, no recursion |
| **SHA-1 (WebSocket)** | Collision attacks | Used only for RFC 6455 handshake (not security-critical) |
| **Nonce exhaustion** | AES-GCM nonce reuse after 2^32 messages | Hard error at limit, warning at 2^31, `encrypted_ipc_rekey()` for rotation |
| **Circuit breaker** | Cascading failure from endpoint outages | Configurable failure threshold + cooldown, half-open probe, manual reset |

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 2.0.x   | Yes       |
| 1.x     | No (Rust, archived) |

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
