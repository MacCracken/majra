# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
- 208 tests (unit + integration + doc-tests), 25+ benchmarks

### Changed
- `matches_pattern()` rewritten to iterative zero-allocation with inline depth tracking
- `ManagedQueue::dequeue()` releases tiers lock before DashMap mutation
- `ManagedQueue::cancel()` drops DashMap guard before awaiting tiers lock
- `RateLimiter` internals swapped from `Mutex<HashMap>` to `DashMap`
- `Relay` dedup map swapped to `DashMap`, stats to `AtomicU64`
- `ConnectionPool::acquire()` drops lock before async connect

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

[Unreleased]: https://github.com/MacCracken/majra/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/MacCracken/majra/compare/v0.22.3...v1.0.0
[0.22.3]: https://github.com/MacCracken/majra/compare/v0.21.3...v0.22.3
[0.21.3]: https://github.com/MacCracken/majra/compare/v0.21.0...v0.21.3
[0.21.0]: https://github.com/MacCracken/majra/releases/tag/v0.21.0
