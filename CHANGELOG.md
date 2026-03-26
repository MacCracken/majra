# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.22.3] — 2026-03-22

### Changed
- Version bump for stiva 0.22.3 ecosystem release

## [Unreleased]

### Added

#### DAG workflow engine (`dag` feature)
- `WorkflowEngine<S, E>` — tier-based DAG executor with parallel step scheduling, retry with exponential backoff, and 4 error policies (Fail/Continue/Skip/Fallback)
- `TriggerMode` — `All` (AND) and `Any` (OR) join semantics for dependency resolution
- `WorkflowStorage` trait — db-agnostic async storage for definitions, runs, and step runs
- `StepExecutor` trait — consumer-defined step execution logic
- `InMemoryWorkflowStorage` — DashMap-backed default storage (no external deps)
- `SqliteWorkflowStorage` — SQLite-backed storage (behind `sqlite` feature)
- `topological_sort_tiers()` — modified Kahn's algorithm returning parallelizable tiers with trigger-mode-aware in-degree
- `WorkflowDefinition`, `WorkflowRun`, `StepRun` — full execution tracking types
- `WorkflowContext` — step output accumulation for downstream reference
- Validation: cycle detection (via `DagScheduler`), referential integrity for deps and fallbacks
- Cooperative cancellation via `AtomicBool` per run
- 22 unit tests (tier sort, validation, storage CRUD, engine execution, retry, error policies, context propagation)
- 4 benchmarks (tier sort linear/wide, engine execute linear/diamond)

#### Scaffold hardening (P-1)
- `scripts/bench-history.sh` — benchmark runner that parses criterion output and appends results to `bench-history.csv`
- `#[inline]` on ~20 hot-path accessors across all modules (Counter, matches_pattern, len/is_empty, is_terminal, satisfies, node_id, classify_status)
- `#[must_use]` on `BarrierResult`, `matches_pattern()`, `DagScheduler::ready()`, `RelayStats`, `RateLimitStats`, `FleetStats`
- Tracing instrumentation for IPC module (`debug!` on bind/accept, `trace!` on frame send/recv)
- Tracing instrumentation for transport module (`debug!` on new connections and close, `trace!` on reuse)
- Benchmark suite for envelope and IPC modules (`envelope_new`, `envelope_serialize_roundtrip`, `ipc_roundtrip_small_payload`, `ipc_roundtrip_large_payload`)
- 8 new unit tests covering Default impls, deprecated compat, broadcast, subscribe, and error paths (barrier, relay, pubsub)

#### Hardening & memory safety (P-1, SecureYeoman audit)
- `Relay::evict_stale_dedup(max_idle)` — TTL-based eviction of the dedup table to prevent unbounded memory growth
- `Relay::send_request()` / `Relay::reply()` — request-response correlation via UUID correlation IDs and oneshot channels for RPC patterns
- `Relay::set_max_dedup_entries()` — configurable cap on dedup table size with automatic LRU eviction
- `RelayMessage::correlation_id` and `is_reply` fields for request-response pairing
- `RelayStats::dedup_evicted` and `dedup_table_size` fields for dedup observability
- `InMemoryWorkflowStorage::evict_older_than(max_age)` — retention policy for terminal workflow runs and their step runs
- `InMemoryWorkflowStorage::with_max_runs(max)` — bounded capacity with auto-eviction of oldest terminal run on insert
- `InMemoryWorkflowStorage::run_count()` and `step_run_count()` for observability
- `PubSub` and `TypedPubSub` automatic dead-subscriber cleanup on publish (configurable interval via `set_cleanup_interval` / `TypedPubSubConfig::cleanup_interval`)
- `PubSub::try_subscribe()` and `TypedPubSub::try_subscribe()` — capacity-checked subscription with `max_subscriptions` limit
- `ConnectionPool::evict_stale(max_idle)` — TTL-based eviction of idle transport connections with disconnect cleanup
- `PooledTransport` internal struct tracks last-use time for each pooled connection
- 13 new tests covering dedup eviction, request-response, bounded capacity, auto-cleanup, pool stale eviction

### Changed
- SQLite backend `.lock().unwrap()` replaced with `map_err` error propagation (4 call sites) — no more panics on poisoned mutex
- Test coverage improved from 88% to 90%+ (133 tests total: 125 unit + 5 integration + 3 doc-tests)

### Added

#### Typed pub/sub (`TypedPubSub<T>`)
- `TypedPubSub<T>` — generic, type-safe pub/sub hub replacing untyped `serde_json::Value` payloads
- `TypedMessage<T>` — typed message with `to_untyped()` / `from_untyped()` conversion
- `BackpressurePolicy` — configurable per-hub: `DropOldest` (default) or `DropNewest`
- `TypedPubSubConfig` — channel capacity, backpressure policy, replay buffer size
- Event replay buffer — optional bounded replay so late subscribers catch up on missed messages
- Subscription filters — `subscribe_filtered()` with predicate-based delivery filtering
- `messages_dropped()` counter for backpressure observability

#### Rate limiter enhancements
- `evict_stale(max_idle)` — periodic sweep of keys with no recent activity, returns eviction count
- `RateLimitStats` — `total_allowed`, `total_rejected`, `active_keys`, `total_evicted` counters
- `stats()` method for runtime observability

#### Observability & logging
- `metrics` module — `MajraMetrics` trait with no-op default (`NoopMetrics`), covering queue, pubsub, heartbeat, rate limiter, relay, and barrier operations
- `ManagedQueue::with_metrics()` — accept custom `Arc<dyn MajraMetrics>` for enqueue/dequeue/state-change reporting
- `logging` feature — structured tracing initialisation via `MAJRA_LOG` env var with per-module filtering
- `info!`-level tracing on ManagedQueue lifecycle events (enqueue, dequeue, state transitions)
- `debug!`-level tracing on heartbeat transitions, evictions, relay sends, rate limiter eviction
- `trace!`-level tracing on per-message pubsub delivery and relay dedup
- Structured `#[instrument]` spans on ManagedQueue enqueue, dequeue, and finish_job

#### Distributed primitives
- `AsyncBarrierSet` — barrier with `arrive_and_wait()` returning a `Future` that resolves on release, with `AtomicBool` release flag to prevent missed wakeups
- `transport` module — `Transport` trait for pluggable relay connections (Unix socket, TCP, future gRPC)
- `TransportFactory` trait — endpoint-based connection creation
- `ConnectionPool` — multiplexed connection pool with per-endpoint reuse and automatic cleanup of disconnected transports

#### Code quality
- `#[non_exhaustive]` on all public enums (8 types)
- `#[must_use]` on `PriorityQueue`, `ConcurrentPriorityQueue`, `RateLimiter`
- `///` doc comments on every public item — `cargo doc -W missing_docs` passes
- `Counter` and `evict_from_dashmap` utilities in `util` module to eliminate boilerplate
- `barrier_status()` and `classify_status()` helpers to deduplicate release-check and status-classification logic
- `persistence_err()` helper to replace 14 repeated `.map_err()` calls in SQLite module
- `ConcurrentBarrierSet` consolidated as type alias for `AsyncBarrierSet`
- `IpcClient` consolidated as type alias for `IpcConnection`

#### Repository infrastructure
- GitHub Actions CI (10-job pipeline: lint, test 3 platforms, MSRV, coverage, benchmarks, security audit, cargo-deny, semver check, docs)
- GitHub Actions release workflow (CI gate → version check → crates.io publish → GitHub Release)
- LICENSE (AGPL-3.0-only), CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md
- Makefile with `make check` (fmt + clippy + test + audit)
- `deny.toml` (license allowlist, advisory checks, source restrictions)
- `codecov.yml` (80% project target, 75% patch)
- `rust-toolchain.toml` (stable + rustfmt + clippy)
- Fuzz targets (queue, pubsub, heartbeat) with libfuzzer
- `supply-chain/` (cargo-vet config + audits)
- `scripts/version-bump.sh` (atomic VERSION + Cargo.toml + Cargo.lock update)
- Architecture overview, threat model, testing guide documentation
- Integration tests (5 multi-module scenarios)
- Benchmarks for all modules (25 benchmarks total)

### Changed
- `matches_pattern()` rewritten from recursive Vec-allocating to iterative zero-allocation with inline depth tracking
- `ManagedQueue::dequeue()` restructured to release tiers lock before DashMap mutation, eliminating nested lock contention
- `ManagedQueue::cancel()` drops DashMap guard before awaiting tiers lock, preventing cross-await lock holding
- `ManagedQueue::dequeue()` only increments `running_count` and emits events when a job is actually found
- `IPC write_frame` uses `u32::try_from` instead of `as u32` to prevent silent truncation on >4 GiB payloads
- `BarrierRecord::forced` now correctly tracks whether `force()` was called via `was_forced` field
- `RateLimiter::Bucket` merged redundant `last_refill`/`last_check` into single `last_access` field
- `ConnectionPool::acquire()` drops lock before async connect to avoid blocking all pool access during I/O

### Fixed
- `AsyncBarrierSet::arrive_and_wait()` missed-wakeup race — added `AtomicBool` released flag checked in wait loop
- `TypedPubSub::publish()` `delivered` counter no longer counts dropped messages under `DropNewest` policy
- SQLite `persist()` no longer panics on `resource_req` serialisation failure (replaced `.unwrap()` with `.map_err()`)
- Removed dead `From<rusqlite::Error>` impl that duplicated `persistence_err()` helper
- SQLite `persist()` no longer stores meaningless `Instant::elapsed()` duration as `enqueued_at`
- Integration tests properly feature-gated with `#[cfg(feature = "...")]` for `--no-default-features` compatibility

## [0.21.3] - 2026-03-21

### Added

#### Thread safety
- `ConcurrentPriorityQueue<T>` — async-aware wrapper with `Notify`-based blocking dequeue (`dequeue_wait`)
- `ConcurrentHeartbeatTracker` — `DashMap`-backed tracker with all `&self` methods
- `ConcurrentBarrierSet` — `DashMap`-backed barrier manager with all `&self` methods
- Compile-time `Send + Sync` assertions on all public types
- Concurrent-access benchmarks (queue, managed queue, heartbeat, rate limiter)

#### Managed queue (`ManagedQueue<T>`)
- `ResourceReq` / `ResourcePool` — optional GPU-aware dequeue filtering (gpu_count, vram_mb)
- `ManagedQueueConfig` — max concurrency enforcement with automatic slot release
- `JobState` enum — `Queued → Running → Completed / Failed / Cancelled` state machine
- `ManagedItem<T>` — lifecycle-tracked queue item with timestamps and resource requirements
- `QueueEvent` — broadcast events on enqueue, dequeue, and state transitions
- TTL-based eviction of terminal jobs via `evict_expired()`
- `sqlite` feature — optional `SqliteBackend` persistence with WAL mode and crash recovery

#### Fleet & heartbeat
- `GpuTelemetry` — structured GPU stats (utilization_pct, memory_used_mb, memory_total_mb, temperature_c)
- `FleetStats` — aggregate node counts, GPU counts, and VRAM totals across fleet
- `EvictionPolicy` — configurable auto-eviction after N offline cycles with mpsc notification channel
- `register_with_telemetry()` — register nodes with initial GPU data
- `heartbeat_with_telemetry()` — update GPU telemetry on heartbeat
- `heartbeat_with_metadata()` — update metadata on heartbeat (both tracker variants)
- `fleet_stats()` — one-call fleet-wide aggregation
- `get_gpu_telemetry()` — retrieve a node's structured GPU data

#### Error types
- `MajraError::InvalidStateTransition` — for illegal job state transitions
- `MajraError::ResourceUnavailable` — for resource constraint violations
- `MajraError::Persistence` — for SQLite errors (behind `sqlite` feature)

### Changed
- `RateLimiter` internals swapped from `std::sync::Mutex<HashMap>` to `DashMap` — same public API, better concurrent performance
- `Relay` dedup map swapped from `std::sync::Mutex<HashMap>` to `DashMap`, stats counters from `Mutex<RelayStats>` to `AtomicU64` — same public API
- `HeartbeatConfig` gains optional `eviction_policy` field (defaults to `None`, non-breaking)

## [0.21.0] - 2026-03-21

### Added
- `envelope` — Universal message envelope with Target routing (Node/Topic/Broadcast)
- `pubsub` — Topic-based pub/sub with MQTT-style `*`/`#` wildcard matching
- `queue` — Multi-tier priority queue (5 levels) with DAG dependency scheduling (Kahn's algorithm)
- `relay` — Sequenced, deduplicated inter-node message relay with atomic counters
- `ipc` — Length-prefixed framing (4-byte u32 + JSON) over Unix domain sockets
- `heartbeat` — TTL-based node health tracking with Online → Suspect → Offline FSM
- `ratelimit` — Per-key token bucket rate limiter with lazy refill
- `barrier` — N-way barrier synchronisation with force/deadlock recovery
- `error` — Shared error types (MajraError, IpcError)
- Feature-gated modules: default = pubsub + queue + relay + heartbeat

[Unreleased]: https://github.com/MacCracken/majra/compare/v0.21.3...HEAD
[0.21.3]: https://github.com/MacCracken/majra/compare/v0.21.0...v0.21.3
[0.21.0]: https://github.com/MacCracken/majra/releases/tag/v0.21.0
