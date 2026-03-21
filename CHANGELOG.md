# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

#### Observability
- `metrics` module — `MajraMetrics` trait with no-op default (`NoopMetrics`), covering queue, pubsub, heartbeat, rate limiter, relay, and barrier operations
- Structured `#[instrument]` tracing spans on `ManagedQueue` key methods (enqueue, dequeue, state transitions)

#### Distributed primitives
- `AsyncBarrierSet` — barrier with `arrive_and_wait()` returning a `Future` that resolves on release
- `transport` module — `Transport` trait for pluggable relay connections (Unix socket, TCP, future gRPC)
- `TransportFactory` trait — endpoint-based connection creation
- `ConnectionPool` — multiplexed connection pool with per-endpoint reuse and automatic cleanup of disconnected transports

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
