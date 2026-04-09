# Threat Model

## Trust Boundaries

majra is a library compiled into the consumer's binary. It does not listen on ports
autonomously. Network connections (Redis, PostgreSQL, WebSocket, IPC) are initiated
by the consumer's code.

## Attack Surface

| Module | Surface | Risk | Mitigation |
|--------|---------|------|------------|
| pubsub | Pattern matching with untrusted topics | Deep nesting DoS | Character-by-character scan, no recursion |
| queue | Unbounded enqueue | Memory exhaustion | Consumer responsibility (apply max queue size) |
| ratelimit | Per-key bucket allocation | Unbounded key growth | `ratelimit_evict_stale()` for periodic cleanup |
| relay | Sequence dedup map | Unbounded sender tracking | `relay_set_max_dedup()` + `relay_evict_stale_dedup()` |
| heartbeat | Node registration | Unbounded node tracking | Eviction policy auto-removes stale nodes |
| ipc | Frame parsing | Oversized frames | 1 MB max frame size check |
| ipc_encrypted | Nonce exhaustion | Key reuse | Counter tracking + `needs_rekey()` warning at 2^31 |
| redis_backend | RESP protocol | Injection | Commands built via structured builder, not string concat |
| postgres_backend | SQL queries | Injection | String-interpolated queries — **consumer must sanitize inputs** |
| ws | HTTP upgrade | Malformed headers | Fixed header parsing with length limits (4 KB) |
| ws | SHA-1 | Collision attacks | SHA-1 used only for WebSocket handshake (RFC 6455 requirement, not security-critical) |

## Memory Safety

Cyrius provides no memory safety guarantees at the language level. All memory management
is manual via `fl_alloc`/`fl_free` (freelist) and `alloc` (bump allocator).

Mitigations:
- Struct layouts are documented with offsets — all code follows documented layouts
- No pointer arithmetic beyond documented struct boundaries
- Freelist allocator provides size-class isolation (16-4096 byte classes)
- Large allocations (>4096) go directly to mmap/munmap

## Supply Chain

- **Zero external dependencies** — Cyrius stdlib is vendored in `lib/`
- **No package manager** — no supply chain attack vector via crate registries
- **Compiler is self-hosting** — Cyrius bootstraps from a 29 KB seed binary
- **Byte-identical verification** — compiler self-compilation produces identical output
