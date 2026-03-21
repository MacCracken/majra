# Majra Roadmap

> **Principle**: Shared concurrency primitives for the AGNOS ecosystem. One implementation, many consumers.

Completed items are in [CHANGELOG.md](../CHANGELOG.md).

---

## v0.21.0 — Foundation ✅

All items complete. See CHANGELOG.md for details.

- [x] `pubsub` — Topic-based pub/sub with MQTT-style wildcard matching
- [x] `queue` — Multi-tier priority queue with DAG scheduling
- [x] `relay` — Sequenced, deduplicated inter-node relay
- [x] `ipc` — Length-prefixed framing over Unix domain sockets
- [x] `heartbeat` — TTL-based node health (Online → Suspect → Offline)
- [x] `ratelimit` — Per-key token bucket rate limiter
- [x] `barrier` — N-way barrier sync with deadlock recovery

---

## v0.22 — Consumer-Ready Core

Thread safety, queue power-ups, and heartbeat fleet support. After this release every module is safe for concurrent use and Ifran can migrate off its custom scheduler, telemetry loop, and eviction logic.

### Thread safety (all modules)
- [ ] `PriorityQueue` — concurrent wrapper (`Arc`-based or `DashMap`-backed tiers) so consumers stop wrapping in their own mutex
- [ ] `HeartbeatTracker` — `DashMap` or `RwLock<HashMap>` backing
- [ ] `BarrierSet` — `DashMap` or `RwLock<HashMap>` backing
- [ ] `RateLimiter` — swap `std::sync::Mutex<HashMap>` for `DashMap`
- [ ] `Relay.seen` — swap `std::sync::Mutex<HashMap>` for `DashMap`
- [ ] Compile-time `Send + Sync` assertions on all public types
- [ ] Concurrent-access benchmarks (multi-thread enqueue/dequeue, heartbeats, rate checks)

### Queue enhancements
- [ ] **Resource-aware dequeue** — optional `ResourceReq { gpu_count: u32, vram_mb: u64 }` on `QueueItem`; dequeue only when resources are available (Ifran's training scheduler needs this)
- [ ] **Max concurrency enforcement** — configurable max-running limit with automatic dequeue when slots free
- [ ] **Job lifecycle states** — `Queued → Running → Completed / Failed / Cancelled` state machine with TTL-based eviction of terminal jobs
- [ ] **Persistent queue backing** — optional `sqlite` feature, WAL mode, crash recovery (replaces Ifran's hand-rolled persistence)
- [ ] **Queue events** — emit on enqueue/dequeue/state-change (wire into pubsub)

### Heartbeat & fleet enhancements
- [ ] **Structured GPU telemetry** — first-class `GpuTelemetry { utilization_pct, memory_used_mb, memory_total_mb, temperature_c }` alongside generic JSON metadata (Ifran drops its custom polling loop)
- [ ] **Node eviction policies** — configurable auto-eviction after Nx `offline_timeout` with callback (Ifran currently does 2x manually)
- [ ] **Fleet stats aggregation** — `FleetStats { total_nodes, online, suspect, offline, total_gpus, total_vram_mb, available_vram_mb }`
- [ ] **Metadata update on heartbeat** — allow updating metadata on beat, not just on register

---

## v0.23 — Event System & Observability

Typed pub/sub, smart rate limiting, and metrics hooks. After this release SecureYeoman can replace its EventDispatcher and sliding-window rate limiter; Ifran unifies its three broadcast channels.

### Pub/sub enhancements
- [ ] **Typed event channels** — `PubSub<T: Serialize + DeserializeOwned + Clone + Send + Sync>` generic variant (replaces Ifran's three separate `broadcast::channel<T>` buses)
- [ ] **Backpressure policies** — `BackpressurePolicy { DropOldest, DropNewest, Block }` configurable per subscription
- [ ] **Event replay buffer** — optional bounded replay so late subscribers catch up
- [ ] **Subscription filters** — predicate-based filtering beyond topic matching

### Rate limiting enhancements
- [ ] **Stale-key eviction** — periodic sweep or LRU eviction for keys with no recent activity (fixes Ifran's unbounded per-IP DashMap)
- [ ] **Rate limit stats** — `RateLimitStats { total_allowed, total_rejected, active_keys, evicted_keys }`

### Observability
- [ ] **`MajraMetrics` trait** — default no-op impl; consumers wire to Prometheus/OpenTelemetry
- [ ] **Structured tracing spans** — spans on all major operations across all modules

### Distributed primitives
- [ ] **Async barrier notification** — `arrive()` returns a `Future` that resolves when barrier releases
- [ ] **Multiplexed connections** — connection pool for relay, reuse TCP connections across topics
- [ ] **Relay transport trait** — pluggable transport (Unix socket, TCP, future gRPC)

---

## v1.0.0 — Stable Release

API freeze, full coverage, proven by real consumers. No new features — just hardening, docs, and the integration proof.

### Stability
- [ ] All public API types are `Send + Sync`
- [ ] `#[must_use]` and `#[non_exhaustive]` on all appropriate types
- [ ] No `unsafe` code (or audited and documented)
- [ ] Error handling audit — all error paths meaningful, no silent failures
- [ ] API naming/ergonomics review

### Testing & proof
- [ ] ≥85% test coverage
- [ ] Multi-module integration tests (simulated Ifran training loop: queue + heartbeat + pubsub)
- [ ] At least 2 real consumers using each module (Ifran + SecureYeoman)
- [ ] Benchmark suite for all modules
- [ ] Fuzz targets for serialization paths

### Documentation
- [ ] docs.rs documentation with examples for every public item
- [ ] Consumer migration guides (Ifran, SecureYeoman)
- [ ] MSRV policy documented
- [ ] SemVer guarantee: no breaking changes in 1.x

---

## Post-v1

- [ ] **Redis-backed mode** — optional Redis backend for cross-process pub/sub and queues
- [ ] **gRPC transport for relay** — relay messages over gRPC instead of raw TCP for firewall friendliness
- [ ] **Prometheus metrics integration** — built-in counters/gauges (queue depth, pub/sub throughput, heartbeat states)
- [ ] **Fleet queue** — distributed job queue across multiple nodes with work-stealing

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
