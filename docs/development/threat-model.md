# Threat Model

## Trust Boundaries

majra is a library compiled into the consumer's binary. It does not listen on ports
autonomously unless the consumer explicitly starts `majra_admin_serve()`. Outbound
network connections (Redis, PostgreSQL, WebSocket, IPC) are initiated by the
consumer's code.

**Crypto trust boundary**: when using the `signed` or `backends` profiles, sigil
(first-party, vendored) is the sole crypto implementation. AES-256-GCM, Ed25519,
HMAC-SHA256, HKDF all live there. sigil's own `docs/audit/` directory documents
its crypto audit surface.

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
| signed_envelope | Ed25519 verify on untrusted input | Forgery | sigil's `ed25519_verify` rejects non-canonical S; canonical encoding is deterministic — tamper causes verify to fail |
| signed_envelope | Key storage | Key leakage | `expected_pk` comparison via `ct_eq` (constant-time); caller owns key lifetime |
| admin | HTTP endpoint | Unauthorized access | **Localhost-only by design**. Binding 0.0.0.0 without fronting auth is a misuse — documented in `src/admin.cyr` header |
| admin | HTTP endpoint | Mutation | Read-only — no PUT/POST/DELETE routes exist |
| patra_queue | SQL injection via payload | Injection | Payloads go into patra INSERT via string concat — **consumer must sanitize payload strings** before enqueue (same contract as `postgres_backend`) |
| patra_queue | Unbounded disk growth | Disk exhaustion | Consumer responsibility — periodically sweep `completed`/`failed` rows |

## Memory Safety

Cyrius provides no memory safety guarantees at the language level. All memory management
is manual via `fl_alloc`/`fl_free` (freelist) and `alloc` (bump allocator).

Mitigations:
- Struct layouts are documented with offsets — all code follows documented layouts
- No pointer arithmetic beyond documented struct boundaries
- Freelist allocator provides size-class isolation (16-4096 byte classes)
- Large allocations (>4096) go directly to mmap/munmap

## Supply Chain

- **Zero external dependencies for the core profile** — `dist/majra.cyr` uses only the Cyrius stdlib (vendored in `lib/`)
- **One first-party dep for the richer profiles** — `sigil` (vendored as `lib/sigil.cyr`, pinned via `[deps.sigil]` in `cyrius.cyml`). sigil is in the same organization, bootstrapped from the same compiler, audited as part of the AGNOS crypto boundary
- **No package manager** — no supply chain attack vector via crate registries
- **Compiler is self-hosting** — Cyrius bootstraps from a 29 KB seed binary
- **Byte-identical verification** — compiler self-compilation produces identical output
- **`cyrius deps` freshness gate** — CI regenerates all four `dist/*.cyr` bundles and fails if `git diff dist/` is non-empty, preventing stale bundles from shipping out-of-sync with the committed `src/`
