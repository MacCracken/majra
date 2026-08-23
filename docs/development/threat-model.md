# Threat Model

## Trust Boundaries

majra is a library compiled into the consumer's binary. It does not listen on ports
autonomously unless the consumer explicitly starts `majra_admin_serve()`. Outbound
network connections (Redis, PostgreSQL, WebSocket, IPC) are initiated by the
consumer's code.

**Crypto trust boundary**: when using the `signed` or `backends` profiles, sigil
(first-party) is the sole crypto implementation. AES-256-GCM, Ed25519,
HMAC-SHA256, HKDF all live there. sigil's own `docs/audit/` directory documents
its crypto audit surface. Since **2.6.8** sigil is a *folded cyrius stdlib
module*, provisioned into `lib/sigil.cyr` by `cyrius lib sync --full` and
version-tied to the toolchain pin (3.12.9 under cyrius 6.5.35) rather than to a
`[deps.sigil]` git tag — so the supply-chain surface is the toolchain snapshot,
covered by `cyrius.lock`'s 108 hashes and CI's `cyrius deps --verify`. See
[`dependency-watch.md`](dependency-watch.md) for why the git dep was retired.

> **Consumer-side note.** Bundles published at **2.6.7 and earlier** carry
> `.deps` sidecars that omit `sigil` for the `signed` / `backends` profiles. A
> consumer that provisions strictly from the sidecar builds with undefined
> `ed25519_*` — reported as a warning, lowered to a trapping `ud2`, and
> surfacing as a SIGILL on first use rather than a failed build. Fixed at 2.6.8;
> consumers pinned earlier should add `sigil` to their own include set.

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
| signed_envelope | Key storage | Key leakage | `expected_pk` comparison via `ct_eq_bytes_lens` (stdlib `lib/ct.cyr`, constant-time); caller owns key lifetime |
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

- **Zero external dependencies for the core profile** — `dist/majra.cyr` uses only the Cyrius stdlib (resolved into `lib/` by `cyrius deps` from the version pinned in `cyrius.cyml`; `lib/` itself is gitignored, repopulated on every CI run + every developer build)
- **Two first-party deps for the richer profiles** — `sigil` (the crypto boundary, resolved into `lib/sigil.cyr` via `[deps.sigil]`) and `sakshi` (structured logging, resolved into `lib/sakshi.cyr` via `[deps.sakshi]`; declared since 2.5.2 to pin a resolution that would otherwise be inherited — and silently downgraded — from sigil's own manifest). `cyrius.lock` carries a SHA-256 over every resolved file plus a commit-pin for sakshi, and CI's `cyrius deps --verify` enforces hash match. Both are in the same organization, bootstrapped from the same compiler; sigil is audited as part of the AGNOS crypto boundary. **Version-pinning note**: a transitive dep whose version is inherited from another dep's manifest is not pinned by majra — declaring it top-level is what makes the resolved bytes reviewable here
- **No package manager** — no supply chain attack vector via crate registries
- **Compiler is self-hosting** — Cyrius bootstraps from a 29 KB seed binary
- **Byte-identical verification** — compiler self-compilation produces identical output
- **`cyrius deps` freshness gate** — CI regenerates all four `dist/*.cyr` bundles and fails if `git diff dist/` is non-empty, preventing stale bundles from shipping out-of-sync with the committed `src/`
