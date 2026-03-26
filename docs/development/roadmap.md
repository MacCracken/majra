# Majra Roadmap

> **Principle**: Shared concurrency primitives for the AGNOS ecosystem. One implementation, many consumers.

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

---

## Post-1.0

### Consumer integration
- [ ] SecureYeoman: expand NAPI bindings beyond pubsub + ratelimit to queue, heartbeat, dag
- [ ] Ifran: integrate managed queue + fleet with GPU-aware scheduling

---

## Non-goals

- **Inference / model serving** — that's Hoosh's domain
- **Model management** — that's Ifran's domain
- **Application-level business logic** — majra provides primitives, consumers define semantics
- **Message broker replacement** — majra is an in-process library, not a standalone broker (use Redis backend for cross-process)
