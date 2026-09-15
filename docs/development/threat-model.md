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
version-tied to the toolchain pin (3.12.18 under cyrius 6.6.4) rather than to a
`[deps.sigil]` git tag — so the supply-chain surface is the toolchain snapshot,
covered by `cyrius.lock`'s 110 hashes and CI's `cyrius deps --verify`. See
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

> **IPC access control changed at 2.6.9 and became effective at 2.6.10.**
> `ipc_bind` now chmods the socket to 0600 *before* `bind(2)` — 2.6.9 issued
> the fchmod after bind, which touches only the sockfs inode and leaves the
> path at the umask mode (0755 under 022), so its header promised owner-only
> access the code did not deliver (see CHANGELOG.md § [2.6.10]). Before either,
> the socket inherited the ambient umask — world-connectable on a typical 022
> process — and majra performs no peer-credential check, so connecting to the
> endpoint *is* authenticating to it. A deployment needing group access must
> widen the mode itself, deliberately. ⚠ On aarch64-Linux
> that chmod never ran from 2.6.9 through **2.7.2** — it was issued as the raw
> x86_64 number 91, which is `capset(2)` there, so every `ipc_bind` failed
> closed (`-EFAULT`) rather than leaving a wider socket behind — and **2.7.3**
> routes it through the per-target `sys_fchmod`.
>
> **Encrypted IPC gained direction separation and replay protection at 2.6.9.**
> Before that, both directions of a channel derived nonces from independent
> counters starting at 0 under a shared key, so every message pair at the same
> counter reused `(key, nonce)` — see the audit. `encrypted_ipc_new` now
> requires a role. Replay defence is a strictly-increasing peer counter: a stale
> or reflected nonce is rejected before decryption, and the window is advanced
> only *after* the tag authenticates, so a forged nonce cannot burn counters
> and lock out the real peer.

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
| ipc_encrypted | Nonce exhaustion | Key reuse | Counter tracking + `encrypted_ipc_needs_rekey()` warning at 2^31 |
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

- **Zero external dependencies, in every profile** — `dist/majra.cyr` and the three richer bundles all draw solely on the Cyrius stdlib snapshot (provisioned into `lib/` by `cyrius lib sync --full` from the version pinned in `cyrius.cyml` and hashed into `cyrius.lock` by `cyrius deps`; `lib/` itself is gitignored, repopulated on every CI run + every developer build)
- **No git dependencies at all** — `sigil` (the crypto boundary) and `sakshi` (structured logging, reached only transitively through patra and sandhi) are both *folded stdlib modules*: declared in `[deps].stdlib`, provisioned into `lib/sigil.cyr` / `lib/sakshi.cyr` by `cyrius lib sync --full`, and version-tied to the toolchain pin — sigil since 2.6.8, sakshi since 2.6.1 (its `[deps.sakshi]` block, added at 2.5.2 to counter a silent downgrade from sigil's own manifest, was retired once sigil dropped that dep). `cyrius.lock` is one SHA-256 per synced file plus the `cyrius<TAB><pin>` trailer — no commit-pin, because there is no git dep to pin — and CI's `cyrius deps --verify` enforces hash match. Both are in the same organization, bootstrapped from the same compiler; sigil is audited as part of the AGNOS crypto boundary. **Version-pinning note**: the toolchain pin is what pins the resolved bytes. Do not re-add a `[deps.<name>]` block for a folded module to "pin" it — `distlib` reclassifies it out of the stdlib leaves and silently drops it from the `.deps` sidecars (see [`dependency-watch.md`](dependency-watch.md))
- **No package manager** — no supply chain attack vector via crate registries
- **Compiler is self-hosting** — Cyrius bootstraps from a 29 KB seed binary
- **Byte-identical verification** — compiler self-compilation produces identical output
- **Distribution freshness gate** — CI runs all four `cyrius distlib` profiles and fails if any of the four `dist/*.cyr` bundles differs from the committed copy (the `.deps` sidecars are regenerated in the same step but are not diffed), preventing stale bundles from shipping out-of-sync with the committed `src/` (`cyrius deps` writes the lockfile and regenerates nothing)
