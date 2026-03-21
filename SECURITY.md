# Security Policy

## Scope

majra is a concurrency primitives library providing pub/sub, queues, relay,
heartbeat, rate limiting, and barrier synchronisation. It is a pure Rust library
with no C FFI, no network I/O in core modules, and no file I/O outside the
optional SQLite persistence feature.

The primary security-relevant surface areas are:

- **Concurrency safety** — data races, deadlocks, or unsound `Send`/`Sync`
  implementations in thread-safe types.
- **Denial of service** — unbounded memory growth in rate limiter keys, queue
  items, or pub/sub subscriptions.
- **SQLite injection** — parameterised queries in the `sqlite` feature prevent
  SQL injection, but malformed inputs could trigger unexpected behaviour.
- **Serialisation boundaries** — `serde_json` deserialisation of untrusted
  payloads in envelopes and relay messages.

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.21.x  | Yes       |
| < 0.21  | No        |

## Reporting a Vulnerability

If you discover a security vulnerability in majra, please report it
responsibly:

1. **Email** [security@agnos.dev](mailto:security@agnos.dev) with a description
   of the issue, steps to reproduce, and any relevant context.
2. **Do not** open a public issue for security vulnerabilities.
3. You will receive an acknowledgment within **48 hours**.
4. We follow a **90-day disclosure timeline**. We will work with you to
   coordinate public disclosure after a fix is available.

## Security Design

- No `unsafe` code in the library (compile-time enforced with `Send + Sync`
  assertions on all public types).
- All concurrent types use `DashMap` or `tokio::sync` primitives — no raw
  atomics or lock-free data structures.
- Rate limiter supports stale-key eviction to prevent unbounded growth.
- SQLite backend uses parameterised queries exclusively.
- Fuzz testing (`make fuzz`) targets serialisation and queue state transitions.
