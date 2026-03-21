# Threat Model

## Trust Boundaries

majra is a library — it does not listen on ports, read files (except optional
SQLite), or make network calls. The trust boundary is the consumer's code.

## Attack Surface

| Module | Surface | Risk | Mitigation |
|--------|---------|------|------------|
| pubsub | Pattern matching with untrusted topics | ReDoS via deep nesting | `MAX_MATCH_DEPTH` = 32 |
| queue | Unbounded enqueue | Memory exhaustion | Consumer responsibility (max queue size not enforced) |
| ratelimit | Per-key bucket allocation | Unbounded key growth | `evict_stale()` for periodic cleanup |
| relay | Sequence dedup map | Unbounded sender tracking | `DashMap` per-sender (bounded by real nodes) |
| heartbeat | Node registration | Unbounded node tracking | `EvictionPolicy` auto-removes stale nodes |
| ipc | Frame parsing | Oversized frames | `MAX_FRAME_SIZE` = 16 MiB |
| sqlite | SQL queries | Injection | Parameterised queries only |
| envelope | JSON deserialization | Malformed payloads | serde_json rejects invalid input |

## Unsafe Code

None. All concurrency is via `DashMap`, `tokio::sync`, and `std::sync::atomic`.
Compile-time `Send + Sync` assertions verify all public types.

## Supply Chain

- `cargo-deny` enforces license allowlist and advisory checks.
- `cargo-vet` tracks third-party audit state.
- Dependencies are minimal: tokio, dashmap, serde, chrono, uuid, thiserror, tracing, async-trait.
- `rusqlite` (optional) uses `bundled` feature to avoid system OpenSSL dependency.
