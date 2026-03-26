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

### QUIC Transport (Tier 1 — Network Evolution)

- [ ] **QUIC relay backend** — `quinn`-based transport alongside TCP. Feature-gated: `quic = ["dep:quinn", "dep:rustls"]`
- [ ] **Multiplexed streams** — ordered streams for pub/sub, RPC. Replaces single TCP connection per peer
- [ ] **Unreliable datagrams** — fire-and-forget channel for real-time data (game state, sensor readings). Consumers: joshua multiplayer, edge sensor fleet
- [ ] **0-RTT reconnect** — edge nodes resume without full handshake on network transitions
- [ ] **Connection migration** — survive IP changes (WiFi → cellular, DHCP renewal)
- [ ] TCP remains default. QUIC is opt-in, non-breaking

See [network-evolution.md](../../../../docs/development/network-evolution.md) in agnosticos for full architecture.

### Other Post-v1

- [ ] **Redis-backed mode** — optional Redis backend for cross-process pub/sub and queues
- [ ] **Prometheus metrics exporter** — built-in `MajraMetrics` implementation with counters/gauges (queue depth, pub/sub throughput, heartbeat states)
- [ ] **Fleet queue** — distributed job queue across multiple nodes with work-stealing
- [ ] **TypedPubSub dead subscriber cleanup** — GC sweep of dropped receivers during publish

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
