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

## Next — Ifran Integration Support

### Fleet & heartbeat enhancements
- [ ] **GPU telemetry in heartbeat metadata** — allow heartbeat payloads to carry structured GPU stats (utilization %, memory, temperature) so Ifran can drop its custom telemetry polling loop
- [ ] **Node eviction policies** — configurable auto-eviction of offline nodes after Nx offline_timeout (Ifran currently does 2x manually)
- [ ] **Fleet stats aggregation** — aggregate GPU counts, total VRAM, online/suspect/offline counts across all tracked nodes

### Queue enhancements
- [ ] **GPU-aware scheduling** — queue items carry resource requirements (GPU count, VRAM); dequeue only when resources are available. Ifran's training scheduler currently uses a simple FIFO and explicitly wants this
- [ ] **Max concurrency enforcement** — built-in max-concurrent-running limit with automatic dequeue when slots free up
- [ ] **Job lifecycle states** — Queued → Running → Completed/Failed/Cancelled state machine with TTL-based eviction of terminal jobs
- [ ] **Persistent queue backing** — optional SQLite persistence for crash recovery (Ifran currently rolls its own)

### Pub/sub enhancements
- [ ] **Typed event channels** — generic `PubSub<T: Serialize>` to replace Ifran's three separate `broadcast::channel<T>` buses (training events, GPU events, download progress)
- [ ] **Backpressure policies** — configurable behavior when subscribers lag (drop oldest, drop newest, block)
- [ ] **Event replay** — optional bounded replay buffer so late subscribers can catch up

### Rate limiting enhancements
- [ ] **Automatic stale-key eviction** — periodic sweep of rate limiter keys with no recent activity (Ifran's per-IP DashMap currently grows unbounded)
- [ ] **Rate limit stats** — expose rejection counts, active key count for observability

### Concurrency primitives
- [ ] **Multiplexed connections** — connection pooling and multiplexing for distributed node communication
- [ ] **Fleet queue** — distributed job queue across multiple nodes with work-stealing

---

## Post-v1

- [ ] **Redis-backed mode** — optional Redis backend for cross-process pub/sub and queues
- [ ] **gRPC transport for relay** — relay messages over gRPC instead of raw TCP for firewall friendliness
- [ ] **Metrics integration** — Prometheus counters/gauges for queue depth, pub/sub throughput, heartbeat states

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
