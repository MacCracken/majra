# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.1.1] - 2026-04-09

### Changed
- Cyrius toolchain pinned to v3.2.5 (cc3 compiler, minimum version)

## [Unreleased]

## [2.1.0] — 2026-04-09

### Changed
- **Cyrius stdlib synced to v3.2.1** — vendored `lib/` updated from 28 to 35 modules, all existing modules refreshed to upstream
- **Binary size**: 93 KB → 108 KB (expanded stdlib)
- **Build tooling references**: `cc2` / `cyrb` → `cyrius` across README, CONTRIBUTING, dependency-watch docs
- **Test runner**: fixed benchmark invocation (direct build+run instead of `cyrius bench`)

### Added
- **7 new stdlib modules** vendored from Cyrius 3.2.1:
  - `sakshi.cyr` / `sakshi_full.cyr` — structured logging/tracing (v0.8.0, enum-based log levels)
  - `base64.cyr` — base64 encode/decode
  - `chrono.cyr` — timestamp formatting and parsing
  - `csv.cyr` — RFC 4180 CSV parser/writer
  - `hashmap_fast.cyr` — optimised hashmap variant
  - `http.cyr` — minimal HTTP/1.0 client
- **Upstream stdlib improvements** pulled into 9 existing modules:
  - `assert.cyr` — `assert_lt`, `assert_gte`, `assert_lte`, `assert_nonnull`
  - `io.cyr` — file locking: `file_lock`, `file_unlock`, `file_trylock`, `file_lock_shared`
  - `string.cyr` — `atoi()` for string-to-integer parsing
  - `regex.cyr` — bugfix: `str_replace` now uses `str_data`/`str_len` correctly
  - `str.cyr` — bugfix: `str_join` uses `str_builder_add` for Str separators
  - `syscalls.cyr` — inotify wrappers: `sys_inotify_init`, `sys_inotify_add_watch`, `sys_inotify_rm_watch`
  - `hashmap.cyr` — `map_iter` support via fnptr
  - `callback.cyr` — syscalls include for timing
  - `tagged.cyr` — `option_print`/`result_print` support

### Fixed
- Stale `cc2`/`cyrb` references in documentation (README.md, CONTRIBUTING.md, dependency-watch.md)
- Test runner benchmark command (`cyrius bench` → direct build+run of `benches/bench_all.cyr`)

## [2.0.0] — 2026-04-08

**Full port from Rust to Cyrius.** All 19 modules re-implemented from scratch with zero external dependencies.

### Changed
- **Language**: Rust → Cyrius (compiled via `cc2`, statically linked)
- **Build system**: Cargo → `cyrb` / direct `cc2` compilation
- **Dependencies**: 25 Rust crates → 0 (Cyrius stdlib only)
- **Binary output**: library crate → standalone executable (~93 KB)
- **Generics**: `T: Send + Clone + Serialize` → `i64` (pointer to heap struct)
- **Traits**: `MajraMetrics`, `Transport`, `WorkflowStorage` → function pointer vtables
- **Async/await**: tokio → threads + mutexes + futex wait/wake
- **DashMap**: → mutex-protected hashmap
- **Floating point**: `f64` rate tokens → fixed-point i64 (x1000 scaling)
- **UUID**: `uuid` crate → 128-bit random via `getrandom` syscall
- **Timestamps**: `chrono` → `clock_gettime(CLOCK_MONOTONIC)` nanoseconds

### Added
- **Redis backend** (`redis_backend.cyr`) — full RESP2 protocol implementation over TCP: SET/GET/DEL, sorted sets (ZADD/ZPOPMIN/ZCARD), PUBLISH, HSET/HGET, EVAL, KEYS, SETEX, EXPIRE
- **PostgreSQL backend** (`postgres_backend.cyr`) — wire protocol v3: startup, cleartext auth, simple query, row parsing, workflow table DDL/CRUD
- **WebSocket** (`ws.cyr`) — RFC 6455: SHA-1 implementation (RFC 3174), base64 encode/decode, WebSocket handshake (Sec-WebSocket-Accept), frame send/recv with masking, ping/pong
- **Encrypted IPC** (`ipc_encrypted.cyr`) — AES-256-GCM framing with nonce management, base64 wire encoding, key rotation. Crypto stubs ready for AES-NI (x86_64) and aarch64 intrinsics
- **295 test assertions** across 4 suites: core (144), expanded (92), backends (25), live (36)
- **17 benchmarks** covering all major operations
- **2 examples**: managed_queue, pubsub_tiers
- **Test runner**: `tests/test.sh` runs all suites + benchmarks

### Removed
- **QUIC transport** — deferred until sigil crypto port (TLS 1.3 dependency)
- **SQLite persistence** — no SQLite binding in Cyrius
- **Prometheus metrics** — replaced by generic function pointer vtable
- **Logging module** — `println` suffices

### Known Issues
- Cyrius compiler local variable clobbering across function calls — mitigated via globals
- Relay dedup and barrier `arrive_and_wait` affected by hashmap lookup issue in nested call contexts
- No `\r` escape in Cyrius string literals — RESP/HTTP/WebSocket use raw byte 13

## [1.0.4]

### Changed
- **License changed from AGPL-3.0-only to GPL-3.0-only** — updated `Cargo.toml`, `deny.toml`, `README.md`, `CONTRIBUTING.md`, `CLAUDE.md`, and `LICENSE` file
- **Dependencies updated** — 25 packages bumped to latest compatible versions (ICU 2.1→2.2, wasm-bindgen 0.2.115→0.2.117, libc 0.2.183→0.2.184, and others)

## [1.0.3]

### Fixed
- **`ws` feature missing `futures-util` dependency** — `ws` feature used `futures_util::{SinkExt, StreamExt}` but did not gate `dep:futures-util`, causing compilation failure when `ws` was enabled without `redis-backend` (which happened to bring `futures-util` in under `full`)

## [1.0.2]

### Changed
- **`redis` dependency upgraded from 0.27 to 1.x** — aligns with redis crate stable 1.0 release. No API changes required; `get_multiplexed_async_connection()`, `AsyncCommands`, `Script::invoke_async()` remain compatible. Consumers pinned to `redis 0.27` via majra can now use `redis 1.x` directly without version conflicts.

## [1.0.1]

### Added
- `EncryptedIpcConnection::rekey()` — key rotation API with nonce counter reset
- `EncryptedIpcConnection::needs_rekey()` / `messages_sent()` — nonce exhaustion tracking (warns at 2^31, errors at 2^32)
- `SlidingWindowLimiter` — approximate sliding-window rate limiter (~5% accuracy of exact, O(1) memory/time per key)
- `WorkflowEngine::resume()` — durable workflow execution: reload step results from storage, skip completed steps, resume from interruption point
- `ConnectionPool::with_circuit_breaker()` — per-endpoint circuit breaker (configurable failure threshold + cooldown)
- `CircuitBreakerConfig`, `CircuitState` — circuit breaker types (Closed/Open/HalfOpen)
- `ConnectionPool::circuit_state()` / `reset_circuit()` — circuit breaker introspection and manual reset
- `Relay::compact_dedup()` — DashMap shrink-to-fit to reclaim dead capacity after eviction
- `RateLimiter::compact()` / `SlidingWindowLimiter::compact()` — DashMap shrink-to-fit
- `NamespacedMetrics` — per-tenant metrics partitioning via prefix delegation
- Subscriber count warning at 40+ receivers per pattern (broadcast quadratic slowdown)
- Cached Redis Lua script SHA for `RedisRateLimiter` (EVALSHA optimization)
- `DirectChannel<T>` — zero-overhead broadcast channel, 73M msg/s, no topic routing
- `HashedChannel<T>` + `TopicHash` — hashed topic routing with coarse timestamp, 16M msg/s
- `TypedPubSub<T>` dual-pipe refactor — exact-topic subscribers use O(1) DashMap lookup (fast path), wildcard-only patterns iterate (slow path)
- 7 new dual-pipe + DirectChannel + HashedChannel benchmarks
- 4 new `SlidingWindowLimiter` tests

### Changed
- `TypedPubSub` internal storage split into `exact_subscriptions` + `pattern_subscriptions` for O(1) exact-topic publish
- `PostgresWorkflowStorage::connect_with_pool_size()` documents pool sizing formula (`cores * 2 + 1`, 10 MB/connection)
- Architecture overview documents three-tier pub/sub, circuit breaker, DashMap fragmentation mitigation

## [1.0.0] — 2026-03-26

**First stable release.** API freeze. Full feature coverage across pub/sub, queues, relay, IPC, heartbeat, rate limiting, barriers, DAG workflows, fleet scheduling, and distributed backends.

### Added

#### DAG workflow engine (`dag` feature)
- `WorkflowEngine<S, E>` — tier-based DAG executor with parallel step scheduling, retry with exponential backoff, and 4 error policies (Fail/Continue/Skip/Fallback)
- `TriggerMode` — `All` (AND) and `Any` (OR) join semantics for dependency resolution
- `WorkflowStorage` trait — db-agnostic async storage for definitions, runs, and step runs
- `StepExecutor` trait — consumer-defined step execution logic
- `InMemoryWorkflowStorage` — DashMap-backed default storage with retention policy (`evict_older_than`, `with_max_runs`)
- `SqliteWorkflowStorage` — SQLite-backed storage (behind `sqlite` feature)
- `topological_sort_tiers()` — modified Kahn's algorithm returning parallelizable tiers with trigger-mode-aware in-degree
- `WorkflowDefinition`, `WorkflowRun`, `StepRun` — full execution tracking types
- `WorkflowContext` — step output accumulation for downstream reference
- Validation: cycle detection, referential integrity for deps and fallbacks
- Cooperative cancellation via `AtomicBool` per run

#### Multi-tenant scoping (`namespace` module)
- `Namespace` — prefix-based tenant isolation for topics, keys, and node IDs
- `topic()`, `key()`, `node_id()`, `pattern()`, `wildcard()` — scoped identifier builders
- `strip_topic()`, `strip_key()` — reverse mapping to extract bare identifiers

#### PostgreSQL storage backend (`postgres` feature)
- `PostgresWorkflowStorage` — `WorkflowStorage` impl backed by `deadpool-postgres` connection pool
- `PostgresQueueBackend` — PostgreSQL persistence for `ManagedQueue` (mirrors `SqliteBackend` API)
- `ManagedQueue::with_postgres()` constructor
- Automatic table creation with `majra_` prefix
- `connect()`, `connect_with_pool_size()`, and `from_pool()` constructors

#### IPC encryption (`ipc-encrypted` feature)
- `EncryptedIpcConnection` — AES-256-GCM wrapper around `IpcConnection` using `ring`
- Pre-shared 256-bit key, monotonic nonce counter per direction
- `send()` / `recv()` encrypt/decrypt JSON payloads transparently

#### WebSocket bridge for pubsub (`ws` feature)
- `WsBridge` — bridges `PubSub` topics to WebSocket clients via `tokio-tungstenite`
- Clients subscribe via `{"subscribe": "pattern"}` JSON handshake
- `WsBridgeConfig` — configurable `max_connections` (default 1024)

#### Distributed rate limiting (`redis-backend` feature)
- `RedisRateLimiter` — distributed token-bucket rate limiter via atomic Redis Lua script
- Auto-expiring keys, compatible API style with in-process `RateLimiter`

#### Distributed heartbeat tracker (`redis-backend` feature)
- `RedisHeartbeatTracker` — cross-instance health coordination via Redis key TTLs
- `register()`, `heartbeat()`, `is_online()`, `get_metadata()`, `list_online()`, `deregister()`

#### Typed pub/sub (`TypedPubSub<T>`)
- `TypedPubSub<T>` — generic, type-safe pub/sub hub with backpressure, replay, and filters
- `BackpressurePolicy` — `DropOldest` (default) or `DropNewest`
- Automatic dead-subscriber cleanup on publish (configurable interval)
- `try_subscribe()` — capacity-checked subscription with `max_subscriptions` limit

#### Rate limiter enhancements
- `evict_stale(max_idle)` — periodic sweep of idle keys
- `RateLimitStats` — `total_allowed`, `total_rejected`, `active_keys`, `total_evicted`

#### Relay enhancements
- `send_request()` / `reply()` — request-response correlation via UUID and oneshot channels
- `evict_stale_dedup(max_idle)` — TTL-based dedup table eviction
- `evict_stale_requests(timeout)` — TTL-based pending request cleanup
- `set_max_dedup_entries()` — configurable dedup table cap with LRU eviction
- `RelayMessage::correlation_id` and `is_reply` fields

#### Observability & logging
- `metrics` module — `MajraMetrics` trait with no-op default and Prometheus implementation
- `NamespacedMetrics` — per-tenant metrics partitioning via prefix delegation
- `logging` feature — structured tracing via `MAJRA_LOG` env var
- Structured `#[instrument]` spans on ManagedQueue operations

#### Distributed primitives
- `AsyncBarrierSet` — async barrier with `arrive_and_wait()` and `AtomicBool` release flag
- `transport` module — `Transport` trait, `TransportFactory`, `ConnectionPool` with stale eviction
- `ConnectionPool::evict_stale(max_idle)` — TTL-based idle connection cleanup

#### Code quality
- `#[non_exhaustive]` on all public enums
- `#[must_use]` on all pure return types
- `#[inline]` on all hot-path accessors
- `///` doc comments on every public item
- `Counter` and `evict_from_dashmap` utilities

#### Repository infrastructure
- GitHub Actions CI (10-job pipeline) and release workflow
- LICENSE, CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md
- Makefile, `deny.toml`, `codecov.yml`, `rust-toolchain.toml`
- Fuzz targets (queue, pubsub, heartbeat)
- `supply-chain/` (cargo-vet), `scripts/version-bump.sh`
- `benchmarks.md` — 3-point trend tracking
- `docs/development/dependency-watch.md` — pinned versions and upgrade paths
- Live Redis integration test (`redis_live_full_lifecycle`) covering pub/sub, queue, rate limiter, heartbeat
- Live PostgreSQL integration test (`postgres_live_workflow_storage`) covering workflow CRUD
- 220 tests (unit + integration + doc-tests), 25+ benchmarks

### Changed
- `matches_pattern()` rewritten to iterative zero-allocation with inline depth tracking
- `ManagedQueue::dequeue()` releases tiers lock before DashMap mutation
- `ManagedQueue::cancel()` drops DashMap guard before awaiting tiers lock
- `RateLimiter` internals swapped from `Mutex<HashMap>` to `DashMap`
- `Relay` dedup map swapped to `DashMap`, stats to `AtomicU64`
- `ConnectionPool::acquire()` drops lock before async connect
- `PostgresWorkflowStorage::connect_with_pool_size()` — configurable pool size (was hardcoded to 16)
- Replay buffer fast-path for exact topic subscriptions (O(1) vs O(n) pattern scan)

### Fixed
- `AsyncBarrierSet::arrive_and_wait()` missed-wakeup race
- `TypedPubSub::publish()` delivered counter accuracy under `DropNewest`
- SQLite `persist()` no longer panics on serialisation failure
- IPC `write_frame` uses `u32::try_from` to prevent silent truncation

## [0.22.3] — 2026-03-22

### Changed
- Version bump for stiva 0.22.3 ecosystem release

## [0.21.3] - 2026-03-21

### Added

#### Thread safety
- `ConcurrentPriorityQueue<T>` — async-aware wrapper with `Notify`-based blocking dequeue
- `ConcurrentHeartbeatTracker` — `DashMap`-backed tracker with all `&self` methods
- `ConcurrentBarrierSet` — `DashMap`-backed barrier manager
- Compile-time `Send + Sync` assertions on all public types

#### Managed queue (`ManagedQueue<T>`)
- `ResourceReq` / `ResourcePool` — GPU-aware dequeue filtering
- `ManagedQueueConfig` — max concurrency enforcement
- `JobState` enum — `Queued → Running → Completed / Failed / Cancelled`
- `ManagedItem<T>` — lifecycle-tracked queue item
- `QueueEvent` — broadcast events on state transitions
- TTL-based eviction via `evict_expired()`
- `sqlite` feature — `SqliteBackend` persistence with WAL mode

#### Fleet & heartbeat
- `GpuTelemetry`, `FleetStats`, `EvictionPolicy`
- `register_with_telemetry()`, `heartbeat_with_telemetry()`, `fleet_stats()`

#### Error types
- `MajraError::InvalidStateTransition`, `ResourceUnavailable`, `Persistence`

### Changed
- `RateLimiter` and `Relay` internals to `DashMap` + `AtomicU64`

## [0.21.0] - 2026-03-21

### Added
- `envelope` — Universal message envelope with Target routing
- `pubsub` — Topic-based pub/sub with MQTT-style wildcard matching
- `queue` — Multi-tier priority queue with DAG dependency scheduling
- `relay` — Sequenced, deduplicated inter-node message relay
- `ipc` — Length-prefixed framing over Unix domain sockets
- `heartbeat` — TTL-based health tracking with Online → Suspect → Offline FSM
- `ratelimit` — Per-key token bucket rate limiter
- `barrier` — N-way barrier synchronisation with deadlock recovery
- `error` — Shared error types (MajraError, IpcError)
- Feature-gated modules: default = pubsub + queue + relay + heartbeat

[Unreleased]: https://github.com/MacCracken/majra/compare/v1.0.4...HEAD
[1.0.4]: https://github.com/MacCracken/majra/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/MacCracken/majra/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/MacCracken/majra/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/MacCracken/majra/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/MacCracken/majra/compare/v0.22.3...v1.0.0
[0.22.3]: https://github.com/MacCracken/majra/compare/v0.21.3...v0.22.3
[0.21.3]: https://github.com/MacCracken/majra/compare/v0.21.0...v0.21.3
[0.21.0]: https://github.com/MacCracken/majra/releases/tag/v0.21.0
