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

## Audit history

| Date | Scope | Result |
|---|---|---|
| 2026-08-22 | First P(-1) pass — all 23 `src/` modules, six review lenses, adversarial verification | 115 confirmed / 2 refuted; 2 critical, 30 high (shipped as 2.6.9). [`../audit/2026-08-22-audit.md`](../audit/2026-08-22-audit.md) |
| 2026-08-22 | Second P(-1) pass — adversarial re-review of the ~7,000 lines 2.6.9 rewrote | 68 confirmed / 10 refuted; 1 critical, 15 high. **50 of the 68 are 2.6.9's own regressions** (shipped as 2.6.10). [`../audit/2026-08-22-audit-pass2.md`](../audit/2026-08-22-audit-pass2.md) |

> **IPC access control changed at 2.6.9.** `ipc_bind` now chmods the socket to
> 0600. It previously inherited the ambient umask — world-connectable on a
> typical 022 process — and majra performs no peer-credential check, so
> connecting to the endpoint *is* authenticating to it. A deployment needing
> group access must widen the mode itself, deliberately.
>
> **Encrypted IPC gained direction separation and replay protection at 2.6.9.**
> Before that, both directions of a channel derived nonces from independent
> counters starting at 0 under a shared key, so every message pair at the same
> counter reused `(key, nonce)` — see the audit. `encrypted_ipc_new` now
> requires a role. Replay defence is a strictly-increasing peer counter checked
> only *after* the tag authenticates.

> **Ed25519 verification strictness is sigil's property, not majra's.**
> `signed_envelope_verify` delegates to `ed25519_verify`. RFC 8032 5.1.7's
> `S < L` check is what makes Ed25519 non-malleable, and implementations
> genuinely diverge on it (CVE-2026-33895 is a forgery from its absence) and on
> whether small-order public keys and R values are rejected. majra does not
> reimplement any of it and must not be read as guaranteeing it — that
> guarantee belongs to sigil's own audit surface.

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
| postgres_backend | Built-in workflow API (`pg_save`/`get`/`delete_workflow_def`) | Injection | Values quoted and escaped by `_pg_add_literal` since **2.6.10** (single quotes doubled). Simple query protocol only — no prepared statements, so escaping is the sole defence; relies on `standard_conforming_strings=on`, PostgreSQL's default since 9.1 |
| postgres_backend | Raw `pg_query` / `pg_exec` | Injection | Caller-composed SQL — **the caller must escape.** `_pg_add_literal` is available for that |
| postgres_backend | Wire transport + auth | Credential and data disclosure | **Plaintext protocol, cleartext password.** No SSLRequest, no TLS; the only auth implemented is `AuthenticationCleartextPassword` (type 3), so the password crosses the wire unencrypted alongside every query and result row. SCRAM (type 10) is **failed closed**, never downgraded. Deploy only over loopback or an already-confidential channel |
| ws | HTTP upgrade | Malformed headers | Fixed header parsing with length limits (4 KB) |
| ws | SHA-1 | Collision attacks | SHA-1 used only for WebSocket handshake (RFC 6455 requirement, not security-critical) |
| signed_envelope | Ed25519 verify on untrusted input | Forgery | sigil's `ed25519_verify` rejects non-canonical S; canonical encoding is deterministic — tamper causes verify to fail |
| signed_envelope | Key storage | Key leakage | `expected_pk` comparison via `ct_eq_bytes_lens` (stdlib `lib/ct.cyr`, constant-time); caller owns key lifetime |
| admin | HTTP endpoint | Unauthorized access | **No auth of any kind, and no default bind.** `majra_admin_serve` takes a caller-supplied dotted-quad string and returns `-1` if it will not parse, so it fails rather than binding somewhere unintended — but nothing enforces loopback. Pass `"127.0.0.1"` unless fronted by a proxy that authenticates. ⚠ Before **2.6.9** `addr` was forwarded raw to `sockaddr_in`, which wants a packed integer, so the documented `"127.0.0.1"` call bound to the low 32 bits of a `char*` |
| admin | HTTP endpoint | Mutation | Read-only — no PUT/POST/DELETE routes exist |
| pubsub | Slow or stalled subscriber | Publisher stall / fan-out DoS | **`PUBSUB_LAG_BLOCK` is the default** — a subscriber that stops draining parks `pubsub_publish` for its topic. Since **2.7.0** a subscription can opt into `PUBSUB_LAG_DROP_NEWEST` / `_DROP_OLDEST` / `_UNSUBSCRIBE`, and `pubsub_dropped_count` reports what was lost. `pubsub_unsubscribe` breaks a wedge |
| pubsub | Slow or stalled subscriber | Publisher stall / fan-out DoS | **`PUBSUB_LAG_BLOCK` is the default** — a subscriber that stops draining parks `pubsub_publish` for its topic. Since **2.7.0** a subscription can opt into `PUBSUB_LAG_DROP_NEWEST` / `_DROP_OLDEST` / `_UNSUBSCRIBE`, and `pubsub_dropped_count` reports what was lost. `pubsub_unsubscribe` breaks a wedge |
| patra_queue | SQL injection via payload | Injection (closed) | Prepared statement with a bound parameter since **2.6.9** — `patra_prepare("INSERT INTO jobs VALUES (?, ?, ?, 0, ?)")` + `patra_bind_text`. The payload never enters the SQL text; no consumer sanitization required |
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

- **Zero external dependencies, in every profile** — `dist/majra.cyr` and the three richer bundles all draw solely on the Cyrius stdlib snapshot (resolved into `lib/` by `cyrius deps` from the version pinned in `cyrius.cyml`; `lib/` itself is gitignored, repopulated on every CI run + every developer build)
- **No git dependencies at all** — `sigil` (the crypto boundary, resolved into `lib/sigil.cyr` via `[deps.sigil]`) and `sakshi` (structured logging, resolved into `lib/sakshi.cyr` via `[deps.sakshi]`; declared since 2.5.2 to pin a resolution that would otherwise be inherited — and silently downgraded — from sigil's own manifest). `cyrius.lock` carries a SHA-256 over every resolved file plus a commit-pin for sakshi, and CI's `cyrius deps --verify` enforces hash match. Both are in the same organization, bootstrapped from the same compiler; sigil is audited as part of the AGNOS crypto boundary. **Version-pinning note**: a transitive dep whose version is inherited from another dep's manifest is not pinned by majra — declaring it top-level is what makes the resolved bytes reviewable here
- **No package manager** — no supply chain attack vector via crate registries
- **Compiler is self-hosting** — Cyrius bootstraps from a 29 KB seed binary
- **Byte-identical verification** — compiler self-compilation produces identical output
- **`cyrius deps` freshness gate** — CI regenerates all four `dist/*.cyr` bundles and fails if `git diff dist/` is non-empty, preventing stale bundles from shipping out-of-sync with the committed `src/`
