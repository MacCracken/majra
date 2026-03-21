# Majra Roadmap

> **Principle**: Shared concurrency primitives for the AGNOS ecosystem. One implementation, many consumers.

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

---

## v1.0.0 — Stable Release

API freeze, full coverage, proven by real consumers.

### Stability
- [ ] Error handling audit — all error paths meaningful, no silent failures
- [ ] API naming/ergonomics review

### Testing & proof
- [ ] At least 2 real consumers using each module (Ifran + SecureYeoman)
- [ ] Live I/O benchmarks (IPC over real Unix sockets, SQLite on real disk)
- [ ] Load/soak testing (sustained throughput, latency percentiles, memory growth)

### Documentation
- [ ] Consumer migration guides (Ifran, SecureYeoman)
- [ ] SemVer guarantee: no breaking changes in 1.x

---

## Post-v1

- [ ] **Redis-backed mode** — optional Redis backend for cross-process pub/sub and queues
- [ ] **gRPC transport for relay** — relay messages over gRPC instead of raw TCP for firewall friendliness
- [ ] **Prometheus metrics exporter** — built-in `MajraMetrics` implementation with counters/gauges (queue depth, pub/sub throughput, heartbeat states)
- [ ] **Fleet queue** — distributed job queue across multiple nodes with work-stealing
- [ ] **TypedPubSub dead subscriber cleanup** — GC sweep of dropped receivers during publish

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
