---
name: Majra Current State
description: Live volatile state — version, dep versions, test counts, bundle sizes, consumers, in-flight blockers. Refresh every release.
type: state
---

# Current State — majra

> **Last refresh**: 2026-06-11 (post-2.4.6) | **Refresh cadence**: every release (ideally bumped by the release post-hook).
> **What this file is**: volatile state. The companion `CLAUDE.md` holds durable rules; this file holds whatever drifts release-to-release. Per [first-party-documentation § CLAUDE.md](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#claudemd), version numbers, test counts, consumer lists, and in-flight work all live here, not in `CLAUDE.md`.

---

## Version

| File | Value | Source |
|---|---|---|
| `VERSION` | **2.4.6** | single source of truth |
| `cyrius.cyml [package].version` | `${file:VERSION}` | reads `VERSION` |
| Latest git tag | `2.4.6` | release workflow asserts `VERSION == tag` |

## Toolchain

| Pin | Value | Source |
|---|---|---|
| Cyrius | **6.1.35** | `cyrius.cyml [package].cyrius` |
| cc5_aarch64 cross-build | not wired (no longer blocked — agnosys 1.3.2 dropped the SYS_OPEN bug; now a verification task) |

> **Cyrius 6.x build workflow**: stdlib provisioning split from git-dep
> resolution. Run `cyrius lib sync` (copies the version-pinned snapshot —
> 88 files under 6.1.35 — into `./lib/`) **before** `cyrius deps` (overlays the sigil
> git dep), and build with `cyrius build --no-deps` so the build's
> auto-`deps` doesn't perturb the synced lib. A bare `cyrius deps` leaves
> a partial `./lib/`; cyrius 6.1.x compiles unresolved calls to a
> runtime `ud2`, so a missing toolchain module (`slice`, `tls`, `ct`)
> surfaces as a SIGILL, not a build error.

## Dependencies (resolved)

| Dep | Resolved version | Pull path | Used by |
|---|---|---|---|
| `lib/sigil.cyr` | **3.7.10** (git tag) | `[deps.sigil]` in `cyrius.cyml` → `cyrius deps` | `src/ipc_encrypted.cyr` (AES-256-GCM), `src/signed_envelope.cyr` (Ed25519) |
| `lib/sandhi.cyr` | (cyrius stdlib snapshot) | `cyrius lib sync` | `src/admin.cyr` (`HTTP_*` consts, `sandhi_server_*` server API) |
| `lib/ct.cyr` | (cyrius stdlib snapshot) | `cyrius lib sync` | `src/signed_envelope.cyr` (`ct_eq_bytes_lens`); sigil 3.x (retired its bundled `ct_eq` at 3.0.2) |
| `lib/tls.cyr` | (cyrius stdlib snapshot) | `cyrius lib sync` | transitive — sandhi references `TLS_BACKEND_LIBSSL` at parse time |
| `lib/patra.cyr` | (cyrius stdlib snapshot) | `cyrius lib sync` | `src/patra_queue.cyr` (durable queue) |
| `lib/sakshi.cyr` | (cyrius stdlib snapshot) | `cyrius lib sync` | structured logging (pulled by patra; also explicit in backend/patra entry points) |
| `lib/agnosys.cyr` | **1.3.2** (transitive via sigil 3.7.10) | resolved by `cyrius deps` | not directly called from `src/` — pulled by the sigil dist concat (now uses slice subscripts → needs `lib/slice.cyr`) |

Lockfile (`cyrius.lock`) now carries SHA-256 over **88** resolved files (the lib-sync snapshot + the sigil/sakshi/agnosys git deps) — was 94 under 6.1.24, dropped to 88 because cyrius 6.1.35 retired `lib/bigint.cyr` from the stdlib snapshot (majra never called it). CI's `cyrius deps --verify` enforces match.

The sigil pin is now **latest (3.7.10)**; the cyrius-5.10.x asm-offset SIGILL that pinned it at 2.9.0 is gone under cyrius 6.x. See [`dependency-watch.md`](dependency-watch.md) for the full bisect history.

## Build footprint

| Target | Lines | Bytes (approx) |
|---|---|---|
| `dist/majra.cyr` (core) | 3,126 | 85 KB |
| `dist/majra-signed.cyr` | 3,272 | 90 KB |
| `dist/majra-admin.cyr` | 3,259 | 90 KB |
| `dist/majra-backends.cyr` | 4,727 | 137 KB |
| `src/` total | 5,315 lines across 23 files | — |

## Test surface

| Suite | Entry point | Assertions | Notes |
|---|---|---|---|
| Core | `src/main.cyr` (binary self-tests) | 150 | runs as part of `cyrius build` smoke |
| Expanded | `tests/test_core.tcyr` | 96 | broader unit coverage |
| Backends | `tests/test_backends.tcyr` | 42 | redis / pg / ws / aes-gcm / signed_envelope / admin |
| Patra-queue | `tests/test_patra_queue.tcyr` | 17 | separate entry — adding to test_backends used to blow the 16384 fixup cap |
| **CI total** | | **305** | |
| Live integration | `tests/test_live.tcyr` | 32 | requires Redis + PostgreSQL running |
| Fuzz harnesses | `fuzz/*.fcyr` | 3 binaries | 500-iter run × 10s timeout per harness in CI |
| Benchmarks | `benches/bench_all.bcyr` | 17 targets | history tracked via `bench-history.csv` (gitignored) |
| Soak | `tests/soak/soak_*.cyr` (4 files) | queue 5k ops, pubsub 2k topics, relay dedup+evict, heartbeat 100×20 + auto-evict | on-demand; all 4 ran clean under 6.1.35 at the 2.4.6 bump |

## Distribution bundles (4 profiles)

| Bundle | Manifest section | Includes |
|---|---|---|
| `dist/majra.cyr` | `[lib]` | core engine: error, counter, envelope, namespace, metrics, ratelimit, heartbeat, queue, pubsub, relay, barrier, ipc, transport, fleet, dag — 15 modules |
| `dist/majra-signed.cyr` | `[lib.signed]` | core + `signed_envelope.cyr` |
| `dist/majra-admin.cyr` | `[lib.admin]` | core + `admin.cyr` |
| `dist/majra-backends.cyr` | `[lib.backends]` | everything — core + signed_envelope + admin + redis_backend + postgres_backend + ipc_encrypted + ws + patra_queue |

`cyrius distlib [<profile>]` regenerates each; CI's distribution-freshness gate fails on stale diff.

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

## Recent releases

| Tag | Date | Headline |
|---|---|---|
| 2.4.6 | 2026-06-11 | Cyrius pin 6.1.24 → 6.1.35, sigil 3.7.8 → 3.7.10 (latest). No source-logic change; the four bundle bodies stay byte-identical (only version banner moves). `bigint` retired from the cyrius 6.1.35 stdlib snapshot (94 → 88 files); majra never called it, so the stale `test_backends` include + `[deps] stdlib` hint were removed. agnosys holds 1.3.2. 305/305 + fuzz + soak clean. |
| 2.4.5 | 2026-06-10 | Cyrius 6.x migration: pin 5.10.44 → 6.1.24, sigil 2.9.0 → 3.7.8 (asm-NI blocker cleared under cyrius 6.x), agnosys → 1.3.2 (aarch64 SYS_OPEN resolved). New `lib sync` + `--no-deps` build workflow. `admin.cyr` ported to `sandhi_server_*`; `signed_envelope.cyr` to `ct_eq_bytes_lens`. 305/305 + fuzz + soak clean. |
| 2.4.4 | 2026-05-11 | Cyrius toolchain pin 5.10.34 → 5.10.44. No source change; bundle bodies byte-identical (only version banner moved). Sigil held at 2.9.0 — [upstream P1](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md) still open at sigil 3.1.1. |
| 2.4.3 | 2026-05-10 | `patra_queue` retires the patra-1.1.1 client-side workarounds; server-side `WHERE`/`ORDER BY`/`LIMIT`/`COUNT(*)`/`MAX()` via patra 1.9.3. `tests/test_patra_queue.tcyr` ported to `sys_unlink()`. |
| 2.4.2 | 2026-05-10 | Cyrius toolchain pin 5.4.17 → 5.10.34. sandhi-from-stdlib for the HTTP server surface. `lib/` gitignored. CI installer fetches stdlib via the source archive. `src/ipc.cyr` ported to `sys_unlink()`. |
| 2.4.1 | 2026-04-20 | Docs + soak-test cleanup cycle. soak_pubsub / soak_relay / soak_heartbeat added. |
| 2.4.0 | 2026-04-20 | Engineering-backlog minor: soak infrastructure, signed envelopes (`[lib.signed]`), HTTP admin endpoint (`[lib.admin]`), patra-backed persistent queue. |

Full history in [`../../CHANGELOG.md`](../../CHANGELOG.md).

## In-flight / blockers

| Item | Status | Where to look |
|---|---|---|
| ~~**sigil asm-offset drift**~~ | **RESOLVED** at 2.4.5 — cyrius 6.x's `param_load` pseudo eliminated the `[rbp-N]` NI-asm SIGILL; sigil now rides latest (3.7.8) | [dependency-watch.md](dependency-watch.md) |
| ~~**aarch64 cross-build** (SYS_OPEN)~~ | **UNBLOCKED** at 2.4.5 — agnosys rolled to 1.3.2 (no SYS_OPEN). Wiring the cross-build step is now a verification task, not blocked-on-upstream | [`roadmap.md`](roadmap.md) "Engineering backlog" |
| ~~**6.1.24 pin vs published tag**~~ | **RESOLVED** at 2.4.6 — pin now tracks the published cyrius 6.1.35 (git tag + release tarball live; toolchain installed locally and CI-resolvable) | `cyrius.cyml [package].cyrius` |
| **Shared-memory IPC transport** | engineering backlog, parked until a consumer hits the syscall-per-message ceiling | [`roadmap.md`](roadmap.md) "Engineering backlog" |

## Refresh procedure

When cutting a release:

1. Bump `VERSION` (everything else reads it via `${file:VERSION}`).
2. Update this file's tables — version, build footprint, test counts (if changed), consumers (if changed), recent releases.
3. If dep versions changed, update the Dependencies table.
4. If a blocker resolved, move its row out of "In-flight / blockers".
5. Re-anchor "Last refresh" date in the header.

Lifecycle-paired with [`../doc-health.md`](../doc-health.md) (doc-state ledger) — this file tracks the *code state*, that one tracks the *doc state*.
