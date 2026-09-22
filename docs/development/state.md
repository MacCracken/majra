---
name: Majra Current State
description: Live volatile state — version, dep versions, test counts, bundle sizes, consumers, in-flight blockers. Refresh every release.
type: state
---

# Current State — majra

> **Last refresh**: 2026-09-21 (post-2.9.1) | **Refresh cadence**: every release (ideally bumped by the release post-hook).
> **What this file is**: volatile state. The companion `CLAUDE.md` holds durable rules; this file holds whatever drifts release-to-release. Per [first-party-documentation § CLAUDE.md](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#claudemd), version numbers, test counts, consumer lists, and in-flight work all live here, not in `CLAUDE.md`.

---

## Version

| File | Value | Source |
|---|---|---|
| `VERSION` | **2.9.1** | single source of truth |
| `cyrius.cyml [package].version` | `${file:VERSION}` | reads `VERSION` |
| Latest git tag | `2.9.0` (2.9.1 pending tag) | release workflow asserts `VERSION == tag` |

## Toolchain

| Pin | Value | Source |
|---|---|---|
| Cyrius | **6.6.6** | `cyrius.cyml [package].cyrius` |
| Cyrius floor for `signed`/`backends` | **≥ 6.4.64** | historical — sigil 3.12.x calls `thread_local_alloc()`, absent before 6.4.64. Since 2.6.8 this is no longer a *pairing* constraint: sigil is a folded stdlib module, so the cyrius pin and the sigil version are one knob |
| aarch64 cross-build (`cycc_aarch64`) | **verified locally at 2.7.3; CI has a build-only cross-build gate, the run lane is still unwired** — every suite, fuzz harness, soak and example passes cross-built (`cyrius build --aarch64 --no-deps`) under `qemu-aarch64` | `.github/workflows/ci.yml` "aarch64 cross-build gate" (build-only); why the lane must *run* the binaries: see In-flight |

> **Cyrius 6.x build workflow**: stdlib provisioning is split from git-dep
> resolution. Run `cyrius lib sync --full` (copies the version-pinned snapshot —
> **111 files under 6.6.6**, 104 top-level + 7 under `lib/unicode/` — into `./lib/`)
> **before** `cyrius deps`, and build
> with `cyrius build --no-deps` so the build's auto-`deps` doesn't perturb the
> synced lib. **The `--full` flag is load-bearing**: since cyrius 6.4.x a bare
> `cyrius lib sync` copies only the declared `[deps].stdlib` *subset* and omits
> the toolchain modules sigil/sandhi reach into that the list does not name
> (`ct`/`keccak`/`bayan`/`result`/`thread_local` — the sidecar leaves outside
> `[deps].stdlib` under 6.6.6; the bare subset is 53 files there, 52 at 6.6.4).
> A missing toolchain module compiles to a runtime `ud2` (**SIGILL, not a build
> error**). CI + release both run `cyrius lib sync --full`.
>
> **Since 2.6.8, `cyrius deps` resolves nothing** — majra declares no git deps at
> all. The step is kept in CI because `cyrius deps --verify` is what enforces the
> lockfile. Two lessons since: plain `cyrius deps` returns *before* the lock
> write when it copies nothing, so a pin move needs `cyrius deps --lock` to
> rehash `lib/` (2.7.1); and a bare `cyrius deps` into an *empty* `lib/`
> resolves only the declared subset, which is how 2.7.2 shipped a 66-entry lock
> (see the lockfile paragraph below).

## Dependencies (resolved)

**majra has zero git dependencies since 2.6.8.** Everything below arrives in the
`cyrius lib sync --full` snapshot and tracks the toolchain pin.

| Dep | Resolved version | Pull path | Used by |
|---|---|---|---|
| `lib/sigil.cyr` | **3.12.18** (stdlib fold; unchanged 6.6.4 → 6.6.6, and sigil's latest tag) | `[deps].stdlib` → `cyrius lib sync --full` | `src/ipc_encrypted.cyr` (`aes_gcm_*`), `src/signed_envelope.cyr` (`ed25519_*`) — 6 symbols total. **Moved off `[deps.sigil]` at 2.6.8** — see below. Since 3.12.18 sigil itself reaches into `lib/sys.cyr` (`sys_uname`), so `sys` must be *present* in any hand-written include list — order is not load-bearing; forward references resolve via the fixup table ([cyrius-quirks.md](cyrius-quirks.md) #2) |
| `lib/patra.cyr` | **1.14.3** (stdlib fold; unchanged, latest tag) | `cyrius lib sync --full` | `src/patra_queue.cyr` (durable queue) |
| `lib/sandhi.cyr` | **1.9.17** (stdlib fold; unchanged, latest tag) | `cyrius lib sync --full` | `src/admin.cyr` (`HTTP_*` consts, `sandhi_server_*` server API) |
| `lib/sakshi.cyr` | **2.5.2** (stdlib fold; unchanged, latest tag) | `cyrius lib sync --full` | structured logging, pulled transitively by patra + sigil. majra's own `src/` calls no sakshi symbol. **Moved off `[deps.sakshi]` at the 6.5.18 pin** |
| `lib/chrono.cyr` | (stdlib fold) | `[deps].stdlib` (declared at 2.7.1) → `cyrius lib sync --full` | **direct since 2.7.3** — `src/envelope.cyr` (`clock_now_ns`, `clock_epoch_ns`, `sleep_ms`, behind `time_now_ns` / `time_epoch_ns` / `_majra_sleep_ns`). Fourteen entry points reach it: thirteen gained the include at 2.7.3, each right after `lib/syscalls.cyr`; `tests/test_backends.tcyr` already had it. `examples/pubsub_tiers.cyr` does not reach envelope/dag and needs none. All four sidecars list it (since 2.7.1). Was transitive-only before the raw-syscall sweep |
| `lib/sys.cyr` | (stdlib fold) | `[deps].stdlib` (declared at 2.7.3) → `cyrius lib sync --full` | transitive — sigil 3.12.18's `agnosys_uname` routes through `sys_uname`. majra's own `src/` calls no `lib/sys.cyr` symbol. Declared so `cyrius audit` / `cyrius bench` (which prepend the declared list) build clean and a bare `cyrius lib sync` copies it; `distlib` infers `sys` into all four sidecars without the declaration. `tests/test_backends.tcyr` includes it before `lib/sigil.cyr` — without it the build reported `OK` and lowered `sys_uname` to a trapping `ud2` |
| `lib/ct.cyr` | (stdlib fold) | `cyrius lib sync --full` | `src/signed_envelope.cyr`, `src/ipc_encrypted.cyr` (`ct_eq_bytes_lens`) |
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

Lockfile (`cyrius.lock`) carries SHA-256 over **111** resolved files — the whole
`lib sync --full` snapshot (104 top-level + 7 under `lib/unicode/`), **sorted**, plus a
`cyrius<TAB>6.6.6` trailer stamping the pin. Both are 6.6.x resolver changes:
6.6.3 sorts the lock (it was written in readdir order, so a lock committed from
one filesystem could not verify on another), and 6.6.4 stamps the pin as a
trailer and **refuses** a stdlib leaf whose snapshot hash moved under an
unchanged pin (`cyrius deps --relock` is the explicit accept). The 2.9.1 pin
move rewrote 27 hashes (core stdlib — alloc, vec, string, fmt, io, sys, sync,
chrono, the syscall peers …), added one (`alloc_cx.cyr`, the cx-target allocator
peer, unreached by majra) and moved the trailer; no folded first-party module
changed. **No commit-pin line since 2.6.8** (zero git deps). Held at 108 across 6.5.31 → 6.5.36; was 99
under 6.4.62–6.4.83, 97 under 6.2.11. 2.7.2's lock had **66** entries and no
trailer — the residue of a bare `cyrius deps` into an empty `lib/`, which
resolves only the declared `[deps].stdlib` subset rather than the snapshot CI
provisions. CI's `cyrius deps --verify` enforces match: 111 verified, 0 failed.

## Build footprint

| Target | Lines | Bytes |
|---|---|---|
| `dist/majra.cyr` (core) | 5,984 | 215 KB |
| `dist/majra-signed.cyr` | 6,237 | 226 KB |
| `dist/majra-admin.cyr` | 6,288 | 227 KB |
| `dist/majra-backends.cyr` | 9,845 | 359 KB |
| `src/` total | 10,646 lines across 23 files | — |

> 2.9.1 moved every bundle by comment lines only (+6 / +6 / +6 / +15 — the
> four re-tensed `sendto` notes); bodies otherwise byte-identical to 2.9.0's,
> sidecars unchanged. Binaries grew with the compiler, not the source:
> `build/majra` 215,536 → 219,792 B under 6.6.6.

> **Both hardening passes moved every bundle.** `src/` grew 5,813 → 7,166
> (2.6.9) → 7,633 lines (2.6.10), much of it rationale comments recording what
> each guard defends against — and, after the second pass, why several of
> 2.6.9's guards were wrong. 2.7.3 moved all four again (core 4,843 → 4,923;
> `CHANGELOG.md` § [2.7.3]).

## Test surface

| Suite | Entry point | Assertions | Notes |
|---|---|---|---|
| Core | `src/main.cyr` (binary self-tests) | 203 | runs as part of `cyrius build` smoke. 150 → 153 at 2.7.3; 154 at 2.8.0; 199 at 2.8.1 (P(-1) sweep regressions); 203 at 2.8.2 (heartbeat owned keys) |
| Expanded | `tests/test_core.tcyr` | 645 | broader unit coverage; grew across the 2.6.x relay/ratelimit/queue fix arc. 299 → 304 at 2.7.3 — the barrier checks now prove *blocking*, not a counter; 623 at 2.8.1 (P(-1) sweep; 621 under qemu, where one 47-second test is x86-only); 626 at 2.8.2 (relay dedup tombstone compaction; 624 under qemu); 645 at 2.9.0 (relay ownership + unsubscribe) |
| Backends | `tests/test_backends.tcyr` | 547 | redis / pg / ws / aes-gcm / signed_envelope / admin — 152 → 519 at 2.8.1 (P(-1) sweep: admin routes, SIGPIPE, ws handshake, encrypted-IPC salt/close); 547 at 2.9.0 (session handshake, Origin gate, anchored verify) |
| Patra-queue | `tests/test_patra_queue.tcyr` | 71 | separate entry — split out at 2.4.0 to stay under the then-16384 cc5 fixup cap (the table starts at 1,048,576 and grows on demand at the 6.6.6 pin — growable since cyrius 6.2.0); split kept as documented architecture |
| **CI total** | | **637** | |
| Live integration | `tests/test_live.tcyr` | 36 | requires Redis + PostgreSQL. **CI-only** — not runnable on a dev box without `redis:7-alpine` + `postgres:16-alpine` up; 7 Redis + 4 PostgreSQL categories |
| Fuzz harnesses | `fuzz/*.fcyr` | 3 binaries | model/oracle-checked since 2.8.2; `<harness> [iterations] [seed]`, seed printed; CI runs 500 iterations × 10 s timeout |
| Benchmarks | `benches/bench_all.bcyr` | 19 targets (`mq_lifecycle` + `mq_enqueue_4producers` since 2.8.2) | history tracked via `bench-history.csv` (never committed — **not present on a fresh clone**, so cross-release comparison means rebuilding the prior pin, as 2.6.8 and 2.7.3 did) |
| Examples | `examples/*.cyr` | 2 binaries | `managed_queue`, `pubsub_tiers`; CI builds + runs both |
| Soak | `tests/soak/soak_*.cyr` (4 files) | queue 5k ops, pubsub 2k topics, relay dedup+evict, heartbeat 100×20 + auto-evict | on-demand; all 4 clean under 6.6.4 at 2.7.3 and under 6.6.6 at 2.9.1, natively and under `qemu-aarch64`. `soak_queue`'s job_count assertion was rewritten at 2.6.9 — it had been asserting the unbounded-growth leak |
| **aarch64, cross-built** | every entry above via `cyrius build --aarch64 --no-deps <entry> build/<name>_aarch64 && qemu-aarch64 ./build/<name>_aarch64` | same counts (expanded 643 under qemu — the x86-only pair) | **verified locally at 2.7.3 and re-verified at 2.9.1 under 6.6.6; CI cross-builds the four suites (build-only gate), the run lane is still unwired.** All four suites + fuzz + soak + both examples pass. At 2.9.1 the expanded suite looped 100× under qemu and 100× natively with 0 failures / 0 crashes; core / backends / patra-queue 25× under qemu and 40× natively. (2.7.3: 100× qemu / 200× native, same result.) At 2.7.2 the expanded suite SIGSEGV'd 14/200 natively (~7 %) from the `cbarrier_arrive_and_wait` bug. Needs `qemu-user` on the runner; recipe in [`../guides/testing.md`](../guides/testing.md) |

## Distribution bundles (4 profiles)

| Bundle | Manifest section | Includes | Sidecar leaves |
|---|---|---|---|
| `dist/majra.cyr` | `[lib]` | core engine: error, counter, envelope, namespace, metrics, ratelimit, heartbeat, queue, pubsub, relay, barrier, ipc, transport, fleet, dag — 15 modules | 27 (the `[deps].stdlib` set, reordered by distlib; `sys` since 2.7.3) |
| `dist/majra-signed.cyr` | `[lib.signed]` | core + `signed_envelope.cyr` | 20 (incl. `sigil`, `sys`, `chrono`, `random`, `keccak` — 20 since 2.8.1; this row read 19 through 2.9.0) |
| `dist/majra-admin.cyr` | `[lib.admin]` | core + `admin.cyr` | 24 (incl. `sandhi`, `sigil` via `tls`, `sys`, `chrono`) |
| `dist/majra-backends.cyr` | `[lib.backends]` | everything — core + signed_envelope + admin + redis_backend + postgres_backend + ipc_encrypted + ws + patra_queue | 25 (incl. `sigil`, `sys`, `chrono`, `patra`, `sandhi`, `tls`, `async`, `dynlib`, `fdlopen` — 25 since 2.8.1; this row read 23 through 2.9.0) |

`cyrius distlib [<profile>]` regenerates each; CI's distribution-freshness gate
fails on stale diff. Sidecar diff vs 2.7.2: `sys` added to all four (+1 each),
`chrono` moved earlier in each list — it is a direct leaf of `src/envelope.cyr`
now, not a transitive one. A consumer provisioning from a sidecar (daimon's
shape) needs nothing; one hand-including a bundle under `--no-deps` must add
`lib/chrono.cyr` after `lib/syscalls.cyr`, and `lib/sys.cyr` — anywhere in
the list; order is not load-bearing for it — on the signed, admin and backends
profiles (all three named sidecars list `sigil` — admin reaches it through
`tls`).

> **Note on `dist/majra.deps`**: the default profile's sidecar mirrors the
> `[deps].stdlib` hint list as a set (27 entries) rather than a computed leaf
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
> dependency closure. daimon vendors `dist/majra.cyr` and ships a
> `daimon-aarch64` asset with it inside — the first non-x86_64 consumer; its
> 2.1.4 toolchain bump is what filed the 2.7.3 raw-syscall issue, from a
> `cyrius build --aarch64` of the bundle. daimon 2.1.1 reported the 2.7.1
> `_sub_new` collision with libro.

## Recent releases

| Tag | Date | Headline |
|---|---|---|
| 2.9.1 | 2026-09-21 | **Toolchain patch: cyrius 6.6.4 → 6.6.6.** Snapshot 110 → 111 files (27 changed, all core stdlib; the new one is `alloc_cx.cyr`), no first-party fold moved — sigil 3.12.18 / patra 1.14.3 / sandhi 1.9.17 / sakshi 2.5.2 byte-identical and each at its latest tag. Lock 111 sorted hashes + `cyrius\t6.6.6` trailer. The aarch64 `ESYSXLAT` routed set grew 44 → 60 rows and now routes `nanosleep` 35 and `sendto` 44; majra's code is unchanged and correct under both toolchains, four `sendto` comments re-tensed (the only `src/` diff — bundle bodies comment-only, sidecars untouched). 1,466 assertions natively and under `qemu-aarch64`; expanded suite 100× on each with 0 crashes; benchmarks head-to-head vs 6.6.4 within noise or faster (largest `pattern_exact` −11.8 %). One new upstream warning line (sigil's over-budget `var buf[262144]`, `note:` → `warning:` at 6.6.5; same storage). Roadmap's pin-move section shipped and deleted. |
| 2.9.0 | 2026-09-15 | **Two wire breaks, both security fixes** (semver.md gains the exception that permits them in a MINOR). Encrypted IPC derives a per-connection key from a 32-byte session handshake, closing cross-connection replay; signed envelopes gain a domain prefix and `verify(se, 0)` now returns 4 so unanchored callers fail closed. Adds a WebSocket Origin policy, `pg_rows_free` / `redis_array_free` / `hb_transitions_free`, relay unsubscribe and message refcounting. 1,466 assertions. |
| 2.8.2 | 2026-09-15 | Heartbeat trackers own their node-id keys (transitions still return the caller's pointer). Relay dedup eviction compacts tombstones, with the helper shared in `counter.cyr`. Fuzz harnesses gain model oracles, CI's iteration arg and a printed seed. New `mq_lifecycle` / `mq_enqueue_4producers` benches; `soak_queue` at 500k ops. 1,419 assertions. |
| 2.8.1 | 2026-09-15 | **P(-1) hardening sweep: 130 findings (3 critical, 17 high).** Encrypted IPC reused `(key, nonce)` across connections (per-handle salt, wire-compatible). Every admin route 404'd. Peers' disconnects SIGPIPE-killed the process. Relay dedup and pubsub keys were borrowed. Test binaries could exit 0 on failure. 1,412 assertions; cross-connection replay and 4 other API/wire items queued for 2.9.0. |
| 2.8.0 | 2026-09-15 | **Namespace minor.** Error codes gain `MAJRA_ERR_*` (the bare `ERR_*` stay as deprecated same-value aliases until 3.0.0). `ws_send_text` / `ws_recv_frame` → `majra_ws_*` under the semver collision exception: next to `lib/ws.cyr`, the 2.7.3 backends bundle did not build under 6.6.4 because of an arity disagreement. The hoosh `ratelimit_*` issue was deleted, since hoosh renamed its own limiter on 2026-07-23. P(-1) sweep moved to 2.8.1. |
| 2.7.3 | 2026-09-14 | **Raw x86_64 syscall numbers ran as DIFFERENT syscalls on aarch64-Linux — three independent failures in shipped code**, filed by daimon from a `cyrius build --aarch64` of the vendored bundle. cyrius's aarch64 backend renumbers 38 x86_64 syscall numbers at runtime (44 `ESYSXLAT` rows counting its six cyrius-private aliases) and passes every other number through verbatim, so `ipc_bind`'s fchmod, `uuid_generate`'s getrandom and the DAG backoff's nanosleep each did something else; only one of the three ever warned. majra now spells no *unrouted* syscall number in arch-neutral code (only `src/ipc.cyr`'s `ESYSXLAT`-routed networking vars and its per-arch sendto remain). The aarch64 run also surfaced an arch-independent `cbarrier_arrive_and_wait` defect as old as `src/barrier.cyr` (2.0.0) that two hardening passes walked past. Pin 6.6.2 → 6.6.4; `sys` joins `[deps].stdlib`; the lock is 110 sorted hashes plus a `cyrius\t6.6.4` trailer. Tests 629 → 637, mutation-verified; every suite, fuzz harness, soak and example also passes cross-built under `qemu-aarch64` (verified locally — CI gained a build-only cross-build gate and a raw-syscall grep gate; the run lane is still unwired). Benchmarks within noise of a 6.6.2 head-to-head. One agnos behaviour change (`time_now_ns` reads #95, not the tick-frozen #40). Full account: `CHANGELOG.md` § [2.7.3]. |
| 2.7.2 | 2026-09-10 | **Cyrius pin 6.5.36 → 6.6.2 — the `Result` / `Option` / `Either` value form — and `encrypted_ipc_send` had been returning a tag with a garbage payload.** Since 6.6.0 a `Result` is a two-register `(tag, payload)`; the single-value bind kept only the tag, so `return rc;` handed the caller whatever was in `rdx` (a bogus byte count on `Ok`, a garbage error code on `Err`). `encrypted_ipc_recv` had the matching shape — a one-argument `err_code_of`, a `payload()` accessor 6.6.0 deleted. Both now bind the pair; `test_backends` binds both halves at six sites. Also found `./lib/` holding 37 undeclared files shadowing the pinned stdlib (seven *older* than the pin): re-resolved from empty, binary byte-identical, lock rewritten to 66 entries with `cyrius deps --lock` — a residue 2.7.3 reverses, since the full `lib sync --full` snapshot is what CI provisions. 479 assertions across the three non-infra suites; no behaviour change beyond the fix. |
| 2.7.1 | 2026-08-30 | **Flat-namespace collisions with libro and the stdlib — and the build was broken.** `_sub_new` → `_majra_sub_new`: libro defines `_sub_new` with a different arity *and* semantics, and in a consumer linking both (daimon does) libro's one-argument call reached majra's two-argument function with an uninitialised second argument and the wrong allocation size; reported from daimon 2.1.1. `sha1` / `_sha1_rotl32` → `_majra_sha1` / `_majra_sha1_rotl32`, because the stdlib's new `lib/sha1.cyr` has a three-argument `sha1`. `[deps].stdlib` had never declared `chrono` or `random` though the build reached both transitively, and `cyrius build src/main.cyr` refused to emit with 2 reachable undefined functions — the manifest's "legacy hint" comment was wrong; the list still drives what is auto-prepended. Pin 6.5.35 → 6.5.36 (108 files), and the lesson that plain `cyrius deps` leaves the lock untouched when it copies nothing (`cyrius deps --lock` rehashes `lib/`). Known: `ws_recv_frame` / `ws_send_text` collide with `lib/ws.cyr` — documented public API, so the rename waits for a minor. 479 assertions, no behaviour change. |
| 2.7.0 | 2026-08-22 | **The four additive APIs both hardening passes deferred.** `pubsub_unsubscribe` + a per-subscriber lag policy (BLOCK stays the default; unsubscribe TOMBSTONES rather than removes, because the publish walk's safety rests on pushes never shifting). Opt-in parallel DAG tiers — **measured before building**: `thread_create`+`join` is 89.4us against a 0.184us CPU-bound step, a 486x ratio, so the default stays serial and `workflow_def_set_parallel` opts in. Type-tagged heartbeat trackers, closing the residual half of a memory-safety finding — `majra_admin_new` now DETECTS the tracker kind instead of trusting a caller declaration, and detection overrides an incorrect explicit kind. Per-key rate-limit stats, so `/ratelimit?key=x` answers about x. No existing signature changed. Tests 560 → 629, four suites mutation-verified. |
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
| **PostgreSQL: cleartext auth, no TLS** | Open — credentials and queries cross the wire in the clear; the connect fails closed on SCRAM rather than downgrading | plan: [roadmap § 2.9 line](roadmap.md) |
| ~~**`base64_*` collides with `lib/bayan.cyr`**~~ | **RESOLVED** at 2.6.8 — renamed to `majra_base64_encode` / `majra_base64_decode`. All four profiles emit zero `base64_*` duplicate-fn warnings (the unrelated upstream `uname_release` duplicate since 2.7.3 is tracked below) | CHANGELOG 2.6.8 · [`semver.md`](semver.md) § Documented exceptions |
| **Shared-memory IPC transport** | parked until a consumer hits the syscall-per-message ceiling | plan: [roadmap § 2.9 line](roadmap.md) |
| ~~**agnos `--agnos` full build (non-core)**~~ | **RESOLVED upstream at the 6.6.6 pin (2.9.1)** — the one thing left at 2.7.3 was a warning, `undefined function '_agnos_getenv'` (`lib/io.cyr` → `lib/args_agnos.cyr`, which no sidecar names), and the open question was whether majra declares the module or io.cyr includes it. cyrius 6.6.6 made `lib/io.cyr` self-sufficient: it includes `lib/args_agnos.cyr` itself. Re-measured at 2.9.1: a clean-room probe of `dist/majra-backends.cyr` + its sidecar leaves builds `OK` under `--agnos` with **zero** undefined functions; core has been agnos-clean since 2.5.0. What remains is majra-side and is not a blocker: `tests/test_backends.tcyr` and `tests/test_patra_queue.tcyr` do not cross-build for agnos (Linux `sys_unlink` / `sys_stat` arities, `SYS_SOCKETPAIR` / `SYS_RECVFROM` / `SYS_GETSOCKNAME` undeclared on that peer) — re-homed to the roadmap's 2.9 line | plan: [roadmap § agnos-portable suites](roadmap.md) · CHANGELOG 2.9.1 |
| **aarch64 CI lane (run, not just build)** | **raw-syscall class RESOLVED at 2.7.3** — majra spells no *unrouted* syscall number in arch-neutral code (only `src/ipc.cyr`'s `ESYSXLAT`-routed networking vars and its per-arch sendto remain), and every suite, fuzz harness, soak and example passes cross-built under `qemu-aarch64` (verified locally). CI now cross-builds the four suites (build-only gate) and greps for raw syscall literals, but the lane that *runs* the binaries under `qemu-aarch64` is still unwired — and it is the half that matters: two of the three defects emitted no build-time warning, so a build-only lane proves nothing. The roadmap trigger (formerly "any non-x86_64 consumer") is met — daimon ships a `daimon-aarch64` asset | [issues/archived/2026-09-14-raw-x86-syscall-numbers-aarch64.md](issues/archived/2026-09-14-raw-x86-syscall-numbers-aarch64.md) · plan: [roadmap § aarch64 CI lane](roadmap.md) · recipe: [`../guides/testing.md`](../guides/testing.md) |
| **`duplicate fn 'uname_release'` — `lib/sigil.cyr:746` vs `lib/sys.cyr:203`** | Open **upstream (sigil), harmless** — sigil 3.12.18 still defines a byte-identical copy of the accessor it now depends on `lib/sys.cyr` for; last definition wins, same body. Warns on any build that includes both under the 6.6.4 snapshot — **and the 6.6.6 one: re-checked at 2.9.1, `lib/sigil.cyr` is byte-identical across 6.6.4 → 6.6.6** (`test_backends`, `cyrius audit`, and consumers provisioning from any of the four sidecars — all four name both `sigil` and `sys`), majra or not. Nothing to do here; goes away when sigil drops its copy |
| **`array local over the per-fn frame budget gets STATIC storage` — `lib/sigil.cyr:25118`** | Open **upstream (sigil), harmless** — new *line* at the 6.6.5 pin, not new behaviour: sigil's crypto-bank init declares `var buf[262144]`, which has always fallen back to shared static storage; 6.6.5 promoted the compiler's `note:` for that fallback to a `warning:` ("one buffer shared by all calls and all threads") and scoped the static's name so it can no longer collide with a consumer's own `buf`. Prints on the same units as the duplicate above. Nothing to do here; clears if sigil moves the bank to `alloc()` | CHANGELOG 2.9.1 § Known issues · [dependency-watch.md](dependency-watch.md) | CHANGELOG 2.7.3 § Known issues · [dependency-watch.md](dependency-watch.md) |
| ~~**`ws_recv_frame` / `ws_send_text` collide with `lib/ws.cyr`**~~ | **RESOLVED** at 2.8.0 — renamed `majra_ws_send_text` / `majra_ws_recv_frame` under the collision exception; the backends bundle now builds next to `lib/ws.cyr` under 6.6.4 | CHANGELOG 2.8.0 · [`semver.md`](semver.md) § Documented exceptions |
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
