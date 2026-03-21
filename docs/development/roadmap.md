# Majra Roadmap

> **Principle**: Shared concurrency primitives for the AGNOS ecosystem. One implementation, many consumers.

Completed items are in [CHANGELOG.md](../CHANGELOG.md).

---

## v1.0.0 — Stable Release

API freeze, full coverage, proven by real consumers. No new features — just hardening, docs, and the integration proof.

### Stability
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
