---
name: Majra Current State
description: Live volatile state — version, dep versions, test counts, bundle sizes, consumers, in-flight blockers. Refresh every release.
type: state
---

# Current State — majra

> **Last refresh**: 2026-08-22 (post-2.6.10) | **Refresh cadence**: every release (ideally bumped by the release post-hook).
> **What this file is**: volatile state. The companion `CLAUDE.md` holds durable rules; this file holds whatever drifts release-to-release. Per [first-party-documentation § CLAUDE.md](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#claudemd), version numbers, test counts, consumer lists, and in-flight work all live here, not in `CLAUDE.md`.

---

## Version

| File | Value | Source |
|---|---|---|
| `VERSION` | **2.6.10** | single source of truth |
| `cyrius.cyml [package].version` | `${file:VERSION}` | reads `VERSION` |
| Latest git tag | `2.6.10` | release workflow asserts `VERSION == tag` |

## Toolchain

| Pin | Value | Source |
|---|---|---|
| Cyrius | **6.5.35** | `cyrius.cyml [package].cyrius` |
| Cyrius floor for `signed`/`backends` | **≥ 6.4.64** | historical — sigil 3.12.x calls `thread_local_alloc()`, absent before 6.4.64. Since 2.6.8 this is no longer a *pairing* constraint: sigil is a folded stdlib module, so the cyrius pin and the sigil version are one knob |
| cc5_aarch64 cross-build | not wired (unblocked; a verification task) |

> **Cyrius 6.x build workflow**: stdlib provisioning is split from git-dep
> resolution. Run `cyrius lib sync --full` (copies the version-pinned snapshot —
> **108 files under 6.5.35** — into `./lib/`) **before** `cyrius deps`, and build
> with `cyrius build --no-deps` so the build's auto-`deps` doesn't perturb the
> synced lib. **The `--full` flag is load-bearing**: since cyrius 6.4.x a bare
> `cyrius lib sync` copies only the declared `[deps].stdlib` *subset* and omits
> the toolchain modules sigil/sandhi reach into
> (`chrono`/`async`/`sakshi`/`dynlib`/`fdlopen`/`keccak`/`random`/`ct`/`slice`).
> A missing toolchain module compiles to a runtime `ud2` (**SIGILL, not a build
> error**). CI + release both run `cyrius lib sync --full`.
>
> **Since 2.6.8, `cyrius deps` resolves nothing** — majra declares no git deps at
> all. The step is kept in CI because `cyrius deps --verify` is what enforces the
> lockfile.

## Dependencies (resolved)

**majra has zero git dependencies since 2.6.8.** Everything below arrives in the
`cyrius lib sync --full` snapshot and tracks the toolchain pin.

| Dep | Resolved version | Pull path | Used by |
|---|---|---|---|
| `lib/sigil.cyr` | **3.12.9** (stdlib fold) | `[deps].stdlib` → `cyrius lib sync --full` | `src/ipc_encrypted.cyr` (`aes_gcm_*`), `src/signed_envelope.cyr` (`ed25519_*`) — 6 symbols total. **Moved off `[deps.sigil]` at 2.6.8** — see below |
| `lib/patra.cyr` | **1.13.10** (stdlib fold) | `cyrius lib sync --full` | `src/patra_queue.cyr` (durable queue) |
| `lib/sandhi.cyr` | **1.9.10** (stdlib fold) | `cyrius lib sync --full` | `src/admin.cyr` (`HTTP_*` consts, `sandhi_server_*` server API) |
| `lib/sakshi.cyr` | (stdlib fold) | `cyrius lib sync --full` | structured logging, pulled transitively by patra + sigil. majra's own `src/` calls no sakshi symbol. **Moved off `[deps.sakshi]` at the 6.5.18 pin** |
| `lib/ct.cyr` | (stdlib fold) | `cyrius lib sync --full` | `src/signed_envelope.cyr` (`ct_eq_bytes_lens`) |
| `lib/tls.cyr` | (stdlib fold) | `cyrius lib sync --full` | transitive — sandhi references `TLS_BACKEND_LIBSSL` at parse time |

> **Why sigil stopped being a git dep (2.6.8).** `distlib` classifies a declared
> git dep *out of the stdlib leaves*, so `dist/majra-signed.deps` and
> `dist/majra-backends.deps` shipped without naming `sigil` — the two profiles
> that exist because they carry crypto. A consumer provisioning from the sidecar
> got undefined `ed25519_init` / `ed25519_sign` / `ed25519_verify` /
> `ct_eq_bytes_lens`, and since an undefined fn lowers to a trapping `ud2` the
> build still reported `OK`; the failure surfaced as a SIGILL at first use.
> Declaring sigil in `[deps].stdlib` restores it to both sidecars. This is the
> same move sakshi made at 6.5.18, under the same rule — see the ⚠ note in
> `cyrius.cyml`, and [`dependency-watch.md`](dependency-watch.md).

Lockfile (`cyrius.lock`) carries SHA-256 over **108** resolved files — the whole
`lib sync --full` snapshot, and nothing else. **No commit-pin line since 2.6.8**
(zero git deps). Held at 108 across the 6.5.31 → 6.5.35 span; was 99 under
6.4.62–6.4.83, 97 under 6.2.11. CI's `cyrius deps --verify` enforces match.

## Build footprint

| Target | Lines | Bytes |
|---|---|---|
| `dist/majra.cyr` (core) | 4,409 | 146 KB |
| `dist/majra-signed.cyr` | 4,586 | 153 KB |
| `dist/majra-admin.cyr` | 4,600 | 154 KB |
| `dist/majra-backends.cyr` | 7,034 | 241 KB |
| `src/` total | 7,633 lines across 23 files | — |

> **Both hardening passes moved every bundle.** `src/` grew 5,813 → 7,166
> (2.6.9) → 7,633 lines (2.6.10), much of it rationale comments recording what
> each guard defends against — and, after the second pass, why several of
> 2.6.9's guards were wrong.

## Test surface

| Suite | Entry point | Assertions | Notes |
|---|---|---|---|
| Core | `src/main.cyr` (binary self-tests) | 150 | runs as part of `cyrius build` smoke |
| Expanded | `tests/test_core.tcyr` | 252 | broader unit coverage; grew across the 2.6.x relay/ratelimit/queue fix arc |
| Backends | `tests/test_backends.tcyr` | 130 | redis / pg / ws / aes-gcm / signed_envelope / admin |
| Patra-queue | `tests/test_patra_queue.tcyr` | 28 | separate entry — adding to test_backends used to blow the 16384 fixup cap |
| **CI total** | | **560** | |
| Live integration | `tests/test_live.tcyr` | 36 | requires Redis + PostgreSQL. **CI-only** — not runnable on a dev box without `redis:7-alpine` + `postgres:16-alpine` up; 7 Redis + 4 PostgreSQL categories |
| Fuzz harnesses | `fuzz/*.fcyr` | 3 binaries | 500-iter run × 10s timeout per harness in CI |
| Benchmarks | `benches/bench_all.bcyr` | 17 targets | history tracked via `bench-history.csv` (gitignored — **not present on a fresh clone**, so cross-release comparison means rebuilding the prior pin, as 2.6.8 did) |
| Examples | `examples/*.cyr` | 2 binaries | `managed_queue`, `pubsub_tiers`; CI builds + runs both |
| Soak | `tests/soak/soak_*.cyr` (4 files) | queue 5k ops, pubsub 2k topics, relay dedup+evict, heartbeat 100×20 + auto-evict | on-demand; all 4 clean under 6.5.35 at 2.6.9. `soak_queue`'s job_count assertion was rewritten at 2.6.9 — it had been asserting the unbounded-growth leak |

## Distribution bundles (4 profiles)

| Bundle | Manifest section | Includes | Sidecar leaves |
|---|---|---|---|
| `dist/majra.cyr` | `[lib]` | core engine: error, counter, envelope, namespace, metrics, ratelimit, heartbeat, queue, pubsub, relay, barrier, ipc, transport, fleet, dag — 15 modules | 24 |
| `dist/majra-signed.cyr` | `[lib.signed]` | core + `signed_envelope.cyr` | 11 (incl. `sigil`) |
| `dist/majra-admin.cyr` | `[lib.admin]` | core + `admin.cyr` | 11 (incl. `sandhi`) |
| `dist/majra-backends.cyr` | `[lib.backends]` | everything — core + signed_envelope + admin + redis_backend + postgres_backend + ipc_encrypted + ws + patra_queue | 14 (incl. `sigil`) |

`cyrius distlib [<profile>]` regenerates each; CI's distribution-freshness gate
fails on stale diff.

> **Note on `dist/majra.deps`**: the default profile's sidecar mirrors the
> `[deps].stdlib` hint list verbatim (24 entries) rather than a computed leaf
> set, so it over-declares for a core-only consumer — it names `sigil`, `patra`,
> `tls` and others the core engine never calls. Pre-existing `distlib` behavior,
> not a majra bug; the three *named* profiles get computed leaf sets.

## Consumers

| Consumer | Modules used | Profile likely chosen |
|---|---|---|
| daimon | pubsub, relay, ipc | core or signed |
| AgnosAI | pubsub, queue, relay, barrier | core |
| hoosh | queue, heartbeat, fleet | core |
| sutra | heartbeat, fleet, dag | core |
| stiva | dag, heartbeat, ipc | core |
| ifran | (per `docs/guides/migration-ifran.md`) | core |
| secureyeoman | (per `docs/guides/migration-secureyeoman.md`) | signed |

> agnosai drives the relay from a 100-worker `sandhi_server_run_pooled` pool and
> reported most of the 2.6.x relay defects. bote and libro sit in the same
> dependency closure.

## Recent releases

| Tag | Date | Headline |
|---|---|---|
| 2.6.10 | 2026-08-22 | **Second P(-1) pass — repairing 115 findings introduced 50 new ones.** 68 confirmed / 10 refuted; 50 were 2.6.9's own. One critical: the rate limiter fails OPEN after an idle period, because 2.6.9's `consumed_ns` back-calculation overflows i64 and drives the refill clock backward. Plus: the 2.6.9 socket-permission fix was a verified no-op (fchmod after bind touches only the sockfs inode — path stayed 0755 while the header promised 0600), the RESP parser truncated ordinary Redis replies containing a nil or `:0`, encrypted IPC could self-deadlock (recv held the send mutex across a blocking read), the circuit breaker latched OPEN forever, `sha1` stopped being reentrant, an existing `.patra` file lost every job on upgrade, and `chb_get` became a use-after-free. Pre-existing and missed by pass 1: SQL injection in the PostgreSQL workflow API. Several of 2.6.9's own tests were tautologies and are now mutation-verified. Tests 515 → 560. See [`docs/audit/2026-08-22-audit-pass2.md`](../audit/2026-08-22-audit-pass2.md) and [`migration-2.6.9.md`](../guides/migration-2.6.9.md). |
| 2.6.9 | 2026-08-22 | **First P(-1) hardening pass**, and the first entry in `docs/audit/`. 115 findings confirmed / 2 refuted across 23 modules, each produced by one reviewer and re-checked by an independent adversarial verifier. Two criticals: encrypted IPC used ONE nonce space for both directions of a channel (byte-identical `(key, nonce)` each way — keystream reuse plus GHASH subkey leakage), and PostgreSQL NULL columns drove a 4 GiB alloc + memcpy because `_pg_read_be32` zero-extends so the `col_len < 0` guard was dead code. Plus 30 highs: the WebSocket 64-bit length form was never implemented, RESP bulk length wrapped negative into a 16-byte block labelled INT64_MAX, `host` was ignored by both backends, the queue's concurrency cap was advisory, `mq_cancel` didn't prevent delivery, `fleet_rebalance` corrupted running_count, `relay_send` emitted out of order, patra_queue spliced payloads into SQL. **Three breaking signature changes** (`encrypted_ipc_new`, `majra_admin_serve`, `transport_send`/`_recv`) — none avoidable. Tests 410 → 515. `pubsub_publish_nosub` -35%, `fleet_stats_100` -16%; `pq_enqueue` +7.9%. See [`docs/audit/2026-08-22-audit.md`](../audit/2026-08-22-audit.md). |
| 2.6.8 | 2026-08-22 | **The folded-module sweep finishes the job.** sigil moved from a `[deps.sigil]` git dep into `[deps].stdlib` — `distlib` had been classifying it out of the stdlib leaves, so `majra-signed.deps` / `majra-backends.deps` shipped without naming `sigil` and a sidecar-provisioned consumer got a `ud2` SIGILL on first `ed25519_*` call (build reported `OK`). Verified in a clean room both before and after. `cyrius.lock` drops to zero git deps / 108 pure hashes. Cyrius pin 6.5.31 → 6.5.35 (snapshot holds at 108; `patra` 1.13.9 → 1.13.10, `bayan` + `vani` move, neither called). No formatter drift, no lint delta, no benchmark delta (measured head-to-head against a 6.5.31 build over 5 trials, not asserted). Bundle bodies byte-identical. |
| 2.6.7 | 2026-08-20 | `[deps.sigil]` 3.12.7 → 3.12.9 — the last folded-module pin lagging the toolchain, found by sweeping the whole dependency closure. Cyrius pin 6.5.20 → 6.5.31 (eleven minors). `src/ws.cyr` reformatted for 6.5.31's canonical continuation indent. Fixed `version-bump.sh` telling you to regenerate 2 of 4 bundles. |
| 2.6.6 | 2026-08-13 | **A full subscriber ring blocked the relay's SENDER, forever.** Both fan-out paths used `chan_send` (futex-waits for space) where Rust's `Relay::send` never blocks; now `chan_try_send`. 2.6.5 is what made it reachable — honouring the requested capacity meant a depth-2 relay deadlocked on the third send. Found by adversarial review of the 2.6.5 change set, not by the suite. |
| 2.6.5 | 2026-08-13 | The relay's capacity was discarded (every subscriber channel was 256 deep regardless) and its timestamp was unportable. |
| 2.6.4 | 2026-08-13 | **The rate limiter never refused anything.** Bucket key ownership, reclaim on evict, allocation-free sweep. |
| 2.6.3 | 2026-08-12 | The `fl_alloc` stopgap retired — upstream fixed it properly. |
| 2.6.2 | 2026-08-11 | The priority queue: O(n²) drain, and an unguarded negative index. |
| 2.6.1 | 2026-08-10 | `relay_receive` raced the **allocator**, not the relay. |
| 2.6.0 | 2026-08-08 | **`relay_receive` was not reentrant**, plus three smaller relay defects — all four reported by agnosai, which drives the relay from a 100-worker pool. Minor bump: two new public functions, one appended stats field. |
| 2.5.3 | 2026-07-28 | First `src/` logic change in the 2.5 line: two silent data-loss races rooted in `fl_alloc` being unsynchronized, head-of-line blocking in `pubsub_publish`, and `#`/`+` tightened to whole-level matching (a namespace-isolation bypass). |
| 2.5.2 | 2026-07-28 | Cyrius pin 6.4.62 → 6.4.83, sigil 3.11.1 → 3.12.1, sakshi pinned forward via a new `[deps.sakshi]` block. `benches/bench_all.bcyr` entry-point repair. |
| 2.5.1 | 2026-07-13 | Cyrius pin 6.3.15 → 6.4.62, sigil 3.9.8 → 3.11.1. `lib sync --full` became load-bearing; agnosys dropped from the graph. |
| 2.5.0 | 2026-06-30 | agnos-target support for the core pub/sub engine. Cyrius pin 6.2.11 → 6.3.15. |
| 2.4.5 | 2026-06-10 | Cyrius 6.x migration: pin 5.10.44 → 6.1.24, sigil 2.9.0 → 3.7.8. New `lib sync` + `--no-deps` workflow. |

Full history in [`../../CHANGELOG.md`](../../CHANGELOG.md).

## In-flight / blockers

| Item | Status | Where to look |
|---|---|---|
| **pubsub has no unsubscribe** | Open by design — an abandoned subscriber wedges its topic, because fan-out is a blocking backpressure contract | plan: [roadmap § 2.7.0](roadmap.md) |
| **PostgreSQL: cleartext auth, no TLS** | Open — credentials and queries cross the wire in the clear; the connect fails closed on SCRAM rather than downgrading | plan: [roadmap § 2.7 line](roadmap.md) |
| **Per-key ratelimit stats** | Open — `/ratelimit` returns global counters for any key, marked `"scope":"global"` | plan: [roadmap § 2.7.0](roadmap.md) |
| **Parallel DAG tier execution** | Open — steps within a tier run serially | plan: [roadmap § 2.7.0](roadmap.md) |
| ~~**`base64_*` collides with `lib/bayan.cyr`**~~ | **RESOLVED** at 2.6.8 — renamed to `majra_base64_encode` / `majra_base64_decode`. All four profiles emit zero duplicate-fn warnings | CHANGELOG 2.6.8 · [`semver.md`](semver.md) § Documented exceptions |
| **Shared-memory IPC transport** | parked until a consumer hits the syscall-per-message ceiling | plan: [roadmap § 2.7 line](roadmap.md) |
| **agnos `--agnos` full build (non-core)** | blocked upstream on patra's unguarded `SYS_LSEEK`. Core is agnos-clean since 2.5.0; only `backends` + a daemon `--agnos` build are affected | plan: [roadmap § Waiting on upstream](roadmap.md) |
| **aarch64 cross-build** | unblocked since 2.4.5; wiring the CI step is a verification task | plan: [roadmap § 2.7 line](roadmap.md) |
| ~~**sigil pin lags the toolchain fold**~~ | **RESOLVED** at 2.6.8 — sigil is a `[deps].stdlib` module now and tracks the pin. The whole class is closed: majra declares zero git deps | `cyrius.cyml [deps]` |
| ~~**sigil asm-offset drift**~~ | **RESOLVED** at 2.4.5 | [dependency-watch.md](dependency-watch.md) |

## Refresh procedure

When cutting a release:

1. Bump `VERSION` (everything else reads it via `${file:VERSION}`).
2. Update this file's tables — version, build footprint, test counts (if changed), consumers (if changed), recent releases.
3. If dep versions changed, update the Dependencies table.
4. If a blocker resolved, move its row out of "In-flight / blockers".
5. Re-anchor "Last refresh" date in the header.

Lifecycle-paired with [`../doc-health.md`](../doc-health.md) (doc-state ledger) — this file tracks the *code state*, that one tracks the *doc state*.
