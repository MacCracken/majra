---
name: Majra Current State
description: Live volatile state — version, dep versions, test counts, bundle sizes, consumers, in-flight blockers. Refresh every release.
type: state
---

# Current State — majra

> **Last refresh**: 2026-05-10 (post-2.4.3) | **Refresh cadence**: every release (ideally bumped by the release post-hook).
> **What this file is**: volatile state. The companion `CLAUDE.md` holds durable rules; this file holds whatever drifts release-to-release. Per [first-party-documentation § CLAUDE.md](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#claudemd), version numbers, test counts, consumer lists, and in-flight work all live here, not in `CLAUDE.md`.

---

## Version

| File | Value | Source |
|---|---|---|
| `VERSION` | **2.4.3** | single source of truth |
| `cyrius.cyml [package].version` | `${file:VERSION}` | reads `VERSION` |
| Latest git tag | `2.4.3` | release workflow asserts `VERSION == tag` |

## Toolchain

| Pin | Value | Source |
|---|---|---|
| Cyrius | **5.10.34** | `cyrius.cyml [package].cyrius` |
| cc5_aarch64 cross-build | not wired (transitive agnosys SYS_OPEN — see Blockers) |

## Dependencies (resolved)

| Dep | Resolved version | Pull path | Used by |
|---|---|---|---|
| `lib/sigil.cyr` | **2.9.0** (git tag) | `[deps.sigil]` in `cyrius.cyml` → `cyrius deps` | `src/ipc_encrypted.cyr` (AES-256-GCM), `src/signed_envelope.cyr` (Ed25519) |
| `lib/sandhi.cyr` | **1.3.3** (cyrius stdlib snapshot) | `[deps].stdlib` includes `"sandhi"` | `src/admin.cyr` (`HTTP_*`, `http_send_status`, `http_server_run`) |
| `lib/tls.cyr` | (cyrius stdlib snapshot) | `[deps].stdlib` includes `"tls"` | transitive — sandhi references `TLS_EARLY_DATA_ACCEPTED` at parse time |
| `lib/patra.cyr` | **1.9.3** (cyrius stdlib snapshot) | `[deps].stdlib` includes `"patra"` | `src/patra_queue.cyr` (durable queue) |
| `lib/sakshi.cyr` | (cyrius stdlib snapshot, transitive via patra) | implicit | structured logging |
| `lib/agnosys.cyr` | 1.0.4 (transitive via sigil 2.9.0) | resolved by `cyrius deps` | not directly called from `src/` — pulled by the sigil dist concat |

Lockfile (`cyrius.lock`) carries SHA-256 over the three git-resolved deps (sigil/sakshi/agnosys). CI's `cyrius deps --verify` enforces match.

Rationale for the sigil pin lives in [`dependency-watch.md`](dependency-watch.md). The TL;DR: 2.9.1+ SIGILL on cyrius 5.10.x due to inline-asm offset drift; tracked upstream as [sigil P1 in the v3.1 arc](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md).

## Build footprint

| Target | Lines | Bytes (approx) |
|---|---|---|
| `dist/majra.cyr` (core) | 3,127 | 85 KB |
| `dist/majra-signed.cyr` | 3,273 | 90 KB |
| `dist/majra-admin.cyr` | 3,259 | 90 KB |
| `dist/majra-backends.cyr` | 4,727 | 137 KB |
| `src/` total | 5,314 lines across 23 files | — |

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
| Soak | `tests/soak/soak_queue.cyr` | 5k-round managed-queue lifecycle | on-demand; soak_pubsub/relay/heartbeat tbd |

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
| 2.4.3 | 2026-05-10 | `patra_queue` retires the patra-1.1.1 client-side workarounds; server-side `WHERE`/`ORDER BY`/`LIMIT`/`COUNT(*)`/`MAX()` via patra 1.9.3. `tests/test_patra_queue.tcyr` ported to `sys_unlink()`. |
| 2.4.2 | 2026-05-10 | Cyrius toolchain pin 5.4.17 → 5.10.34. sandhi-from-stdlib for the HTTP server surface. `lib/` gitignored. CI installer fetches stdlib via the source archive. `src/ipc.cyr` ported to `sys_unlink()`. |
| 2.4.1 | 2026-04-20 | Docs + soak-test cleanup cycle. soak_pubsub / soak_relay / soak_heartbeat added. |
| 2.4.0 | 2026-04-20 | Engineering-backlog minor: soak infrastructure, signed envelopes (`[lib.signed]`), HTTP admin endpoint (`[lib.admin]`), patra-backed persistent queue. |

Full history in [`../../CHANGELOG.md`](../../CHANGELOG.md).

## In-flight / blockers

| Item | Status | Where to look |
|---|---|---|
| **sigil asm-offset drift** (the only filed blocker) | open upstream as P1 in sigil v3.1 work arc | [sigil ticket](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md) |
| **aarch64 cross-build** | not wired; resolves automatically when sigil unblocks (agnosys 1.2.4 mainline already has the SYS_OPEN portability fix, but we're stuck on agnosys 1.0.4 transitively via sigil 2.9.0) | [`roadmap.md`](roadmap.md) "Waiting on upstream" |
| **Shared-memory IPC transport** | engineering backlog, parked until a consumer hits the syscall-per-message ceiling | [`roadmap.md`](roadmap.md) "Engineering backlog" |

## Refresh procedure

When cutting a release:

1. Bump `VERSION` (everything else reads it via `${file:VERSION}`).
2. Update this file's tables — version, build footprint, test counts (if changed), consumers (if changed), recent releases.
3. If dep versions changed, update the Dependencies table.
4. If a blocker resolved, move its row out of "In-flight / blockers".
5. Re-anchor "Last refresh" date in the header.

Lifecycle-paired with [`../doc-health.md`](../doc-health.md) (doc-state ledger) — this file tracks the *code state*, that one tracks the *doc state*.
