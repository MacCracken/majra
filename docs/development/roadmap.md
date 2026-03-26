# Majra Roadmap

> **Principle**: Shared concurrency primitives for the AGNOS ecosystem. One implementation, many consumers.

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

---

## v1.0.0 — Stable Release

API freeze, full coverage, proven by real consumers.

### Stability
- [x] Error handling audit — all error paths meaningful, no silent failures
- [x] API naming/ergonomics review

### Testing & proof
- [ ] At least 2 real consumers using each module (Ifran + SecureYeoman)
- [x] Live I/O benchmarks (IPC over real Unix sockets, SQLite on real disk)
- [x] Load/soak testing (sustained throughput, latency percentiles, memory growth)

### Documentation
- [x] Consumer migration guides ([Ifran](../guides/migration-ifran.md), [SecureYeoman](../guides/migration-secureyeoman.md))
- [x] SemVer guarantee: [no breaking changes in 1.x](semver.md)

---

## Post-v1

### QUIC Transport (Tier 1 — Network Evolution)

- [x] **QUIC relay backend** — `QuicTransport`, `QuicTransportFactory`, `QuicListener` behind `quic` feature flag
- [x] **Multiplexed streams** — each send/recv opens a new bi-directional QUIC stream (no HOL blocking)
- [x] **Unreliable datagrams** — `send_datagram()` / `recv_datagram()` for fire-and-forget data
- [x] **0-RTT reconnect** — quinn caches session tickets automatically for fast resume
- [x] **Connection migration** — QUIC handles IP changes transparently at the protocol level
- [x] TCP remains default. QUIC is opt-in via `quic` feature flag, non-breaking

See [network-evolution.md](../../../../docs/development/network-evolution.md) in agnosticos for full architecture.

### Other Post-v1

- [x] **Redis-backed mode** — `RedisPubSub` and `RedisQueue` behind `redis-backend` feature flag
- [x] **Prometheus metrics exporter** — `PrometheusMetrics` behind `prometheus` feature flag with counters, gauges, and histograms
- [x] **Fleet queue** — `FleetQueue<T>` with least-loaded routing, resource filtering, and work-stealing rebalance
- [x] **TypedPubSub dead subscriber cleanup** — `cleanup_dead_subscribers()` on both `PubSub` and `TypedPubSub`

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
