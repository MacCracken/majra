# Dependency Watch

Pinned versions and upgrade considerations for majra's dependencies.

## Core Dependencies (always compiled)

| Crate | Pinned | Notes |
|-------|--------|-------|
| **tokio** | 1.x | Runtime. Features: sync, time, net, macros, rt, io-util. Major version bump is breaking. |
| **dashmap** | 6.x | Lock-free concurrent hashmap. Used in every module. v6 dropped some deprecated APIs from v5. |
| **serde** | 1.x | Serialisation framework. Stable API, rarely breaks. |
| **serde_json** | 1.x | JSON codec. Used for envelopes, relay, IPC, redis. |
| **chrono** | 0.4 | Timestamps. Pre-1.0 but stable in practice. `serde` feature required. |
| **uuid** | 1.x | V4 UUIDs for task IDs and correlation IDs. `serde` feature required. |
| **thiserror** | 2.x | Error derive macro. v2 dropped transparent requirement for `#[from]`. |
| **tracing** | 0.1 | Structured logging facade. Near-zero overhead when inactive. |
| **async-trait** | 0.1 | Async trait support. Will become unnecessary when Rust stabilises `async fn in trait`. |

## Optional Dependencies

| Crate | Feature | Pinned | Notes |
|-------|---------|--------|-------|
| **rusqlite** | `sqlite` | 0.32 | SQLite with bundled amalgamation. WAL mode. |
| **prometheus** | `prometheus` | 0.14 | Metrics exporter. Stable API. |
| **redis** | `redis-backend` | 0.27 | Async Redis client. `tokio-comp` + `aio` features. |
| **futures-util** | `redis-backend` | 0.3 | Stream utilities for Redis pub/sub. |
| **tokio-postgres** | `postgres` | 0.7 | Async PostgreSQL client. No TLS by default. |
| **deadpool-postgres** | `postgres` | 0.14 | Connection pooling for tokio-postgres. |
| **quinn** | `quic` | 0.11 | QUIC transport. Brings rustls. |
| **rustls** | `quic` | 0.23 | TLS via ring. `ring` feature for crypto. |
| **rcgen** | `quic` | 0.13 | Self-signed cert generation (testing). |
| **ring** | `ipc-encrypted` | 0.17 | AES-256-GCM encryption. FIPS-grade crypto. |
| **tokio-tungstenite** | `ws` | 0.24 | WebSocket server/client. |
| **tracing-subscriber** | `logging` | 0.3 | Tracing output formatting. env-filter + fmt features. |

## Dev Dependencies

| Crate | Pinned | Notes |
|-------|--------|-------|
| **criterion** | 0.5 | Benchmark framework with HTML reports. Macro API. |
| **proptest** | 1.x | Property-based testing. |
| **tokio** (dev) | 1.x | Full features + test-util for async tests. |

## Upgrade Considerations

- **tokio 2.x**: Not yet released. When it arrives, will require significant migration.
- **async-trait removal**: Once `async fn in trait` stabilises (expected ~1.85+), remove `async-trait` dep and use native syntax. Monitor [tracking issue](https://github.com/rust-lang/rust/issues/91611).
- **dashmap 7.x**: Watch for API changes. v5→v6 was straightforward.
- **ring 0.18+**: Watch for API changes to `LessSafeKey`/`UnboundKey`.
- **redis 0.28+**: Watch for connection API changes (multiplexed async connection).
- **criterion 0.8+**: Uses builder pattern instead of macro API. Both prakash and hisab have migrated; majra still on 0.5.
