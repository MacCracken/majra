# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.9.0] - 2026-09-15

The six items the 2.8.1 P(-1) sweep confirmed but could not take in a PATCH.
**1,466 assertions** pass (core 203, expanded 645, backends 547, patra-queue
71), natively and cross-built under `qemu-aarch64`.

> ⛔ **TWO WIRE BREAKS. Both ends of a deployment upgrade together.** A 2.9.0
> encrypted-IPC peer cannot talk to a 2.8.x one, and 2.8.x envelope signatures
> do not verify on 2.9.0. Both are security fixes that have no compatible form;
> [`semver.md`](docs/development/semver.md) gains the exception that permits
> them in a MINOR, with the conditions that gate it. Migration:
> [`docs/guides/migration-2.9.0.md`](docs/guides/migration-2.9.0.md).

### Security — encrypted IPC replay across connections (wire break)

Every connection under one PSK derived the same key, so a frame captured on one
replayed into any later one. 2.8.1's per-handle nonce salt stopped the keystream
reuse; the replay needed a handshake.

- Each handle writes a **32-byte hello** at construction (`MJEIPCv1`, role,
  16-byte CSPRNG salt) and reads the peer's before its first frame. The AES key
  is `HKDF(PSK, initiator_salt || responder_salt, "majra-eipc-session-v1")`,
  so it is unique per connection and a captured frame authenticates nowhere
  else. Pinned by a test that replays a real captured frame into a second
  connection under the same PSK and requires it to fail.
- `encrypted_ipc_new` keeps its signature. Construction still does no blocking
  I/O — the hello write is 32 bytes and the peer hello is read on first use —
  so both ends of a socketpair can still be built in one thread.
- **A role clash is caught at the handshake**, before any frame is encrypted,
  and reports the new `MAJRA_ERR_IPC_ROLE` (104) rather than a generic
  `MAJRA_ERR_IPC`.
- `encrypted_ipc_rekey` installs the new PSK and **re-derives over the salts
  already exchanged** — no second handshake, so a reader parked in `recv` is
  never disturbed. It compares the new key against the PSK, not the derived
  session key.

### Security — signed envelopes (wire break + a fail-closed return)

- **Domain separation.** The signing input now begins with the 24 bytes
  `majra/signed-envelope/v1`, so an envelope signature cannot be confused with
  another Ed25519 message under the same key. Canonical length grows by 24, and
  the golden byte vectors move with it.
- **`signed_envelope_verify(se, 0)` returns 4 (`SIGNED_ENV_SELF_CONSISTENT`),
  not 0.** Unanchored, a 0 return had been indistinguishable from an anchored
  pass, so `if (verify(se, 0) == 0)` trusted an envelope an attacker could
  re-sign under their own key. The new code is non-zero on purpose: unchanged
  callers now **fail closed**.
- New `signed_envelope_verify_anchored(se, pk)` — a 0 key is a pk mismatch (2),
  never a pass, so the unanchored mode cannot be reached by accident.

### Security — WebSocket Origin policy

`_ws_accept_upgrade` discarded the request, so no consumer could refuse a
cross-site handshake (the CSWSH class). `ws_set_origin_check(fn)` installs a
policy called with the Origin value (0 when the header is absent) **before** the
101; refusing answers `403 Forbidden` and leaks no accept key.
`ws_origin_check()` reads it back. The default is unchanged: with no policy
installed, every Origin upgrades exactly as through 2.8.2.

### Added — results you can release

Values majra returned came off the bump allocator, which never reclaims, so a
long-lived client grew without bound. They are freelist-backed now:

- `pg_rows_free(rows)` — the `pg_query` / `pg_exec` rows and every cell.
- `redis_array_free(arr)` for array replies of strings, and
  `redis_array_free_shallow(arr)` for arrays of integers or nested arrays,
  whose elements are values rather than pointers.
- `hb_transitions_free(tr)` — the status-sweep transitions and their pairs. The
  id pointers inside belong to the caller and are untouched.
- Internally these use `_majra_vec_new` / `_majra_vec_push` / `_majra_vec_free`
  in `src/counter.cyr`: the same `lib/vec.cyr` layout on the freelist, with a
  push that frees the buffer a grow abandons (`vec_push` leaks it).

### Added — relay unsubscribe and message ownership

- `relay_unsubscribe(r, ch)` takes a channel out of the fan-out;
  `relay_subscriber_count(r)` reports what is left. Until now the only exit was
  to close the channel and wait for the next fan-out to notice.
- A delivered `RelayMessage` is shared by every subscriber that accepted it and
  nothing could free one. It carries a **refcount** now (appended at offset 64;
  offsets 0-56 unchanged, struct 64 → 72 bytes): the fan-out takes one per
  delivery, `relay_msg_release` drops one, and the last release frees it.
  `relay_msg_retain` / `relay_msg_refcount` complete the set. A message no
  subscriber accepted is freed by the relay, as before.

### Notes

- No public symbol was removed or changed in arity. The additions are
  `MAJRA_ERR_IPC_ROLE`, `SIGNED_ENV_SELF_CONSISTENT`,
  `signed_envelope_verify_anchored`, `ws_set_origin_check`, `ws_origin_check`,
  `pg_rows_free`, `redis_array_free`, `redis_array_free_shallow`,
  `hb_transitions_free`, `relay_unsubscribe`, `relay_subscriber_count`,
  `relay_msg_retain`, `relay_msg_release`, `relay_msg_refcount`.
- Every fix carries a test that fails without it: the replay test passes a
  captured frame on a mutated build that skips HKDF, and the role, Origin,
  ownership and release paths are each pinned.

## [2.8.2] - 2026-09-15

The three PATCH-safe items the 2.8.1 sweep left for the next patch. **1,419
assertions** pass (core 203, expanded 626, backends 519, patra-queue 71). No
public symbol changed.

### Fixed

- **Heartbeat trackers keyed their node map on the caller's id pointer.**
  `map_set` stores keys by reference, so a caller that registered an id from a
  per-request or reused buffer left the map probing freed or rewritten bytes.
  The tracker now owns a copy of each key and frees it on deregister and on
  eviction.
  - Transition pairs and `hb_list_by_status` / `chb_list_by_status` still
    return the caller's own id pointer. That is exactly what they returned
    before, and a node evicted in the same sweep that reports it never hands
    out a freed key.
  - Re-registering an id refreshes the node in place, so a pointer from
    `hb_get` stays valid. The node state grew from 40 to 56 bytes by appending
    fields; offsets 0-32 and `chb_get`'s 40-byte copy are unchanged.
- **Relay dedup eviction left hashmap tombstones that were never reclaimed.**
  Under sender churn, a bounded table ran out of empty slots, and every new
  sender's lookup probed the whole map under the relay mutex. Both eviction
  paths now compact.
  - The compaction helper moved to `src/counter.cyr` as `_majra_map_compact`,
    shared by ratelimit, heartbeat and relay. Heartbeat's duplicate copy is
    gone.

### Changed — fuzz, bench, soak

- **The fuzz harnesses now check results.**
  - They read CI's iteration argument (`build/<harness> [iterations] [seed]`)
    and print their seed, and an oracle violation exits 1 with the seed and
    step.
  - `fuzz_queue` models the priority queue: highest tier first, FIFO within a
    tier, exact lengths.
  - `fuzz_pubsub` checks `matches_pattern` against the frozen 2.8.0 matcher,
    over random strings on `[a b / + #]` as well as a topic pool.
  - `fuzz_heartbeat` models membership. One phase checks that every id pointer
    a sweep or list returns is the one registered; the other registers from
    buffers that are overwritten and freed at once.
  - Each oracle was mutation-checked: a broken tier order, a broken `+`
    match, and borrowed heartbeat keys each fail within 100 steps.
- `benches/bench_all.bcyr` gains `mq_lifecycle` (enqueue, dequeue, complete,
  release) and `mq_enqueue_4producers`.
- `soak_queue` runs 100,000 rounds (500k operations, was 1,000) and releases
  completed jobs.

## [2.8.1] - 2026-09-15

The P(-1) hardening sweep, run against 2.8.0. It confirmed **130 findings: 3
critical, 17 high, 48 medium, 62 low**. 95 are fixed (17 of them in part), 27
were test coverage gaps and got tests, and 5 need an API or wire change, so they
are queued for 2.9.0. Full report:
[`docs/audit/2026-09-15-audit.md`](docs/audit/2026-09-15-audit.md).

**1,412 assertions** pass: core 199, expanded 623, backends 519, patra-queue 71.
All suites also pass cross-built under `qemu-aarch64`, where the expanded suite
reports 621 because one 47-second concurrency test runs on x86 only. The
three fuzz harnesses, four soaks and both examples are clean on both arches.
No public symbol was added or removed. **The wire format is unchanged**: a
2.8.0 ↔ 2.8.1 encrypted-IPC interop probe (separate processes, both roles, both
directions) passes all four pairings.

### Security

- **Encrypted IPC reused `(key, nonce)` across connections** *(critical)*. Every
  connection under one PSK restarted its nonce at `(role, 0)`, so a reconnect
  or a second client repeated the keystream. Each handle, and each rekey, now
  draws a CSPRNG salt into nonce bytes 1-7. Receivers never read those bytes,
  which is why the change is wire-compatible. `encrypted_ipc_new` returns 0 if
  no entropy is available.
  - ⚠ **Still open**: a frame captured on one connection can replay into a
    later connection. The fix needs a handshake, which is a wire change, so it
    is queued for 2.9.0.
  - A frame carrying the receiver's own role now kills the handle.
- **Admin endpoint: every route returned 404** *(critical)*. Routes were
  compared with `str_eq` (Str) against the cstr from `sandhi_server_path_only`,
  and a crafted path dereferenced request bytes. Routes are now matched with
  `streq`.
  - ⚠ **Behaviour change**: the handler answers **400** unless `Host` is absent,
    an IP literal, or `localhost` (optional port). This blocks DNS rebinding
    against a loopback bind. A reverse proxy must forward an IP-literal or
    `localhost` Host.
- **SIGPIPE killed the process** when a peer disconnected. The writes in
  `ipc_send_frame`, WebSocket frames and handshakes, and the Redis/PostgreSQL
  clients now use `MSG_NOSIGNAL`. A failed PostgreSQL connection is marked dead.
- **`ratelimit_check` read the clock before taking its mutex**, so under
  contention `elapsed_ns` went negative and drained buckets into a lasting
  deficit.
- **The limiters' eviction handed out fresh quota.** The token-bucket sweep
  evicted buckets that had not refilled, and the sliding-window sweep evicted
  keys that were being actively rejected.
- **Relay dedup keyed on the caller's borrowed `from` pointer**, so a reused
  buffer let replays through. Its eviction freed entries without checking
  `map_delete`, a use-after-free and double free. Keys are owned now.
- `signed_envelope_verify(se, 0)` is documented as proving only
  self-consistency. Pass `expected_pk` whenever the result gates an action.

### Fixed

- **Pubsub topic/pattern keys were borrowed**, and compaction could re-point a
  live topic at an unsubscribe caller's temporary buffer.
- **`patra_queue_new` called `patra_init()` on every open**, swapping patra's
  process-wide mutexes under concurrent users.
- patra_queue ids from two handles on one file collided. Completing or failing
  a job also completed its undelivered duplicate-id sibling; status updates now
  require `status = 1`.
- **Serial `workflow_execute` kept running tier-mates after a `POLICY_FAIL`
  step failed** (fail-fast had regressed at 2.7.0).
- `fleet_submit` could lose a job to a concurrent `fleet_deregister_node`, and
  rebalance/deregister freed the `QueueItem` handle already handed to the
  caller.
- `encrypted_ipc_close` / `_rekey` wedged behind a reader parked in `read(2)`.
- Memory:
  - Redis commands and PostgreSQL queries no longer bump-allocate per call;
    3,000 Redis commands now grow nothing.
  - Ratelimit maps compact their tombstones.
  - `chb_fleet_stats` and the list-by-status calls no longer leak a key vec per
    call, including per `GET /fleet`.
  - Re-registering a heartbeat node no longer abandons the old state.
- `ManagedQueue` job ids came from one global shared by every queue but guarded
  per queue, so duplicate keys lost jobs.
- `counter_inc` / `counter_add` are one atomic add instead of a mutex round
  trip. The struct layout is unchanged.
- The remaining findings are in the audit report's table.

### Changed — tests and CI

- **A failing test binary could exit 0.** Every entry point ended with
  `syscall(SYS_EXIT, r)`, which ends only the main thread. With detached threads
  still alive, the process reported another thread's status, and CI checks exit
  codes. All suites, fuzz harnesses, soaks, examples and the bench binary now
  end with `sys_exit_group(r)`.
- 108 new regression assertions from the sweep plus the patra-queue sibling
  check. Most fail when their fix is reverted.
- CI's syscall gate now matches any `*SYS_` var and allows exactly `src/ipc.cyr`'s
  routed rows (41/42/43/48/49/50) plus its per-arch sendto (44/206).
- Docs: threat-model rows for nonce salting, cross-connection replay, admin DNS
  rebinding, anchored envelope verification and parked pubsub publishers. The
  roadmap gains **2.9.0** (the API/wire deferrals) and **Next patch** (heartbeat
  key ownership, relay tombstones, fuzz oracles).

### Performance

5-trial medians against a 2.8.0 baseline taken on the same machine the same day. No benchmark regressed by more than 1 %. The largest wins come from the atomic counter, the slot-walk `chb_fleet_stats`, and the pattern matcher and dequeue fast paths.

| Benchmark | 2.8.0 | 2.8.1 | Δ |
|---|---|---|---|
| `envelope_new` | 2057 ns | 2062 ns | +0 % |
| `pq_enqueue` | 608 ns | 616 ns | +1 % |
| `pq_dequeue` | 32 ns | 13 ns | -59 % |
| `pattern_exact` | 113 ns | 63 ns | -44 % |
| `pattern_wildcard_+` | 109 ns | 67 ns | -39 % |
| `pattern_wildcard_#` | 68 ns | 28 ns | -59 % |
| `pattern_no_match` | 53 ns | 5 ns | -91 % |
| `pubsub_publish_nosub` | 197 ns | 99 ns | -50 % |
| `pubsub_1sub_publish` | 1108 ns | 1034 ns | -7 % |
| `direct_channel_send` | 409 ns | 410 ns | +0 % |
| `heartbeat_100nodes` | 1422 ns | 1440 ns | +1 % |
| `fleet_stats_100` | 10942 ns | 2229 ns | -80 % |
| `ratelimit_check` | 1699 ns | 1563 ns | -8 % |
| `relay_send` | 1533 ns | 1426 ns | -7 % |
| `barrier_cycle` | 2240 ns | 1703 ns | -24 % |
| `circuit_state` | 3 ns | 3 ns | timer floor |
| `counter_inc` | 48 ns | 4 ns | -92 % |

## [2.8.0] - 2026-09-15

Namespace minor: majra's error codes gain a `MAJRA_` prefix, and the two
WebSocket functions that collided with `lib/ws.cyr` are renamed. **638
assertions** pass (core 154 · expanded 304 · backends 152 · patra-queue 28),
natively and cross-built under `qemu-aarch64`, along with three fuzz harnesses,
four soaks and both examples. `cyrius fmt --check` and `cyrius lint` (0
warnings) are clean, `cyrius deny` reports 0 violations, and all four bundles
are regenerated. No behaviour changed.

### Changed — error codes are `MAJRA_ERR_*`; bare `ERR_*` is deprecated

`src/error.cyr` declared its 20 codes bare (`ERR_NONE` … `ERR_WORKFLOW_STORAGE`,
`ERR_IPC_FRAME_TOO_LARGE` … `ERR_IPC_JSON`). Enum constants share one flat
namespace across every library a consumer links, and under cyrius 6.6.4 `cyrius
lint` notes each bare name: `ERR_*` is reserved for the sakshi base logger. None
collided with anything in the snapshot.

- **New `MAJRA_ERR_*`** for all 20 codes, same values. Every in-tree use moved:
  `src/error.cyr` (`err_name`), `src/barrier.cyr`, `src/ipc.cyr`,
  `src/ipc_encrypted.cyr`, `src/main.cyr`, `tests/test_core.tcyr`.
- **The bare names stay** as same-value aliases in the same enums, so existing
  code compiles unchanged. A MINOR may add constants but not rename them
  ([`semver.md`](docs/development/semver.md)). They are removed at 3.0.0, or
  sooner for any single name that a stdlib module starts declaring.
  `semver.md` gains a **Deprecations** section recording this.
- `test_error` asserts that all 20 aliases still equal their replacements
  (core 153 → 154).
- ⚠ `cyrius lint` still notes the 20 aliases. The notes go away only when the
  aliases do.

### Changed — `ws_send_text` / `ws_recv_frame` → `majra_ws_send_text` / `majra_ws_recv_frame`

**Breaking for `dist/majra-backends.cyr` callers**, the only profile carrying
`src/ws.cyr`. It is taken under `semver.md`'s collision exception, and both
conditions were probed under 6.6.4:

| name | majra | `lib/ws.cyr` |
|---|---|---|
| `ws_send_text` | `(fd, data, len)` → 0 / -1 | `(ws, msg)` → bytes written |
| `ws_recv_frame` | `(fd)` → frame struct | `(ws, opcode_out, len_out)` → payload pointer |

- The old names were not merely order-fragile. A clean-room unit with the
  backends sidecar leaves, `lib/ws.cyr` and the **2.7.3** bundle **fails to
  build** under 6.6.4: `duplicate fn 'ws_send_text' disagrees about arity: this
  one takes 3, the one in lib/ws.cyr takes 2`, and the same for `ws_recv_frame`.
  The 2.8.0 bundle builds and runs in the same unit.
- **Migration**: rename the two call sites. Arguments and return values are
  unchanged. A missed site is a compile error (`expects 2 arguments, got 3`
  against `lib/ws.cyr`, or an undefined function without it), never a silent
  rebind. No aliases: keeping the old name *is* the collision.
- Updated in-tree: `tests/test_backends.tcyr`, the `src/ws.cyr` header comments,
  `README.md` and the `docs/architecture/overview.md` data-flow diagram. Both
  rows are in `semver.md`'s renames table.

### Removed — the hoosh `ratelimit_*` collision issue

`docs/development/issues/2026-06-23-ratelimit-fn-collision-namespace.md` asked
majra to rename `ratelimit_*` / `sliding_window_*` because hoosh defined its own
`ratelimit_new` / `ratelimit_check`. That collision was the consumer's to
resolve, and hoosh already has: its limiter has been `hoosh_ratelimit_new` /
`hoosh_ratelimit_check` since 2026-07-23. The issue is deleted, and majra's
limiter names stay as they are.

### Roadmap

The P(-1) hardening sweep, proposed for a 2.7.4, is now the 2.8.1 item.

## [2.7.3] - 2026-09-14

Toolchain patch — **cyrius 6.6.2 → 6.6.4** — plus an aarch64 filing in the
class 6.6.4 swept from its own stdlib, and a barrier defect the aarch64 run
surfaced that turns out to be arch-independent and as old as `src/barrier.cyr`.
**637 assertions** pass (core 153 · expanded 304 · backends 152 · patra-queue
28), three fuzz harnesses, four soak runs and both examples are clean, and —
new for this release — **every suite, fuzz harness, soak and example also passes
cross-built for aarch64 under `qemu-aarch64`** (the expanded suite looped 100×
there and 200× natively with zero crashes; the other three suites 25× each). Benchmarks are within noise of
a 6.6.2 head-to-head (five-trial medians; largest move `pattern_exact` −7 %,
`circuit_state` 3 ns → 2 ns is timer-floor quantisation).

### Fixed — raw x86_64 syscall numbers ran as DIFFERENT syscalls on aarch64-Linux

⛔ **P1 for any aarch64-Linux consumer; three independent failures in shipped
code.** Filed 2026-09-14 by daimon from a `cyrius build --aarch64` of the
vendored `dist/majra.cyr`
([`docs/development/issues/archived/2026-09-14-raw-x86-syscall-numbers-aarch64.md`](docs/development/issues/archived/2026-09-14-raw-x86-syscall-numbers-aarch64.md));
this is the class cyrius 6.6.4 swept from its own stdlib (its new
`raw_syscall_literals_routed` gate scans `lib/` + `cbt/` only, and its
consumer pin-sweep list does not name majra — nothing had cross-built majra
until daimon did). cyrius's aarch64 backend renumbers 38 x86_64 syscall
numbers at runtime (`ESYSXLAT`; 44 rows counting its six ≥1000 private
aliases) and passes every other number through **verbatim** — so a stray
x86_64 number is not an error, it is a different, valid syscall. Every claim in the filing was re-derived here
against the 6.6.4 emitter before anything was changed.

| site | was | on aarch64-Linux it issued | now |
|---|---|---|---|
| `src/ipc.cyr` `ipc_bind` | `syscall(91, fd, 0600)` — fchmod | **capset(2)** with `fd` as the header pointer → `-EFAULT` → **every `ipc_bind` failed** | `sys_fchmod(fd, 384)` |
| `src/envelope.cyr` `uuid_generate` | `syscall(318, …)` — getrandom | `-ENOSYS` × 8 attempts → **every envelope id was 0/0** (fail-closed, but total) | `sys_getrandom(…)` on every target; the agnos/non-agnos split is gone |
| `src/dag.cyr` retry backoff | `syscall(35, &ts, &rem)` — nanosleep | **unlinkat(2)** handed a stack address as `dirfd` → error at once, never `-EINTR` → **the backoff never slept**; a file-removal call with a pointer-derived path | `_majra_sleep_ns(attempt * 10 ms)` |

Confirmed before the fix by running majra's own core suite cross-built under
`qemu-aarch64` at 2.7.2: `envelope id_hi non-zero` and `evicted after cycle 2`
both failed. **Only the 318 line ever produced a build-time warning**, and only
because the name collided with the aarch64 peer's `SYS_GETRANDOM` (278) — as a
dep bundle prepended after the stdlib leaves, majra's 318 also overrode the
peer's for the consumer's whole translation unit. The same override hit the
**agnos** peer (`SYS_GETRANDOM` = 45 there): the `#ifdef CYRIUS_TARGET_AGNOS`
arm called `sys_getrandom`, which spells the enum name — so it issued 318,
the kernel fell through to -1, and 2.7.2's envelope ids were 0/0 on agnos too,
along with every other `sys_getrandom` caller in a unit that included the
bundle. The other two sites were silent on every toolchain.

**What changed.** majra no longer spells a syscall number in arch-neutral code:

- `var SYS_CLOCK_GETTIME`, `var SYS_GETRANDOM` and `var _SYS_FCHMOD` are gone.
  `time_now_ns` / `time_epoch_ns` delegate to `lib/chrono.cyr`'s `clock_now_ns`
  / `clock_epoch_ns`; `uuid_generate` calls `sys_getrandom`; `ipc_bind` calls
  `sys_fchmod`. On Linux chrono issues the same routed 228 — the difference is
  that cyrius's `raw_syscall_literals_routed` gate scans stdlib code, so a
  trimmed row would now be caught upstream instead of here.
- **New `_majra_sleep_ns(total_ns)`** (`src/envelope.cyr`) — majra's one sleep
  primitive, internal (leading underscore, no public listing) so that this
  stays a PATCH under [`semver.md`](docs/development/semver.md). `sleep_ms` is `poll(NULL, 0, ms)` on Linux/macOS (7 is a routed
  row), kernel32 `Sleep` on PE and #41 on agnos; it discards poll's return, so
  the function sleeps to a **monotonic deadline** instead, which carries the
  EINTR-with-remainder intent of the nanosleep loop it replaced. Each request
  is clamped to 1 ms ≤ `ms` ≤ 1 s (`poll(…, 0)` returns immediately; x86_64
  `poll` takes a C `int`, so ≥ 2^31 ms would truncate negative into an
  infinite wait; the loop re-sleeps the remainder), and **termination does
  not depend on the clock advancing**: a reading that has not moved past the
  pre-sleep one (agnos #95 returns -1 for the whole uptime when the kernel
  refused its TSC calibration) charges the request just made instead. The
  review's first cut recomputed `left` from the deadline alone, which on a
  dead clock never returned — worse than the bounded `sys_sleep_ms` it
  replaced on that target. Probed with a stub clock: 10 ms → one sleep,
  2.5 s → three, both return. The DAG backoff, `test_heartbeat_eviction`,
  the parallel-tier and circuit-breaker tests and `soak_heartbeat` all use it.
- The five networking numbers (`_SYS_SOCKET` 41 … `_SYS_LISTEN` 50) **stay as
  target-neutral `var`s** — all five are routed rows, and the peers' `SYS_SOCKET`
  spelling does not exist on agnos or Windows. `src/redis_backend.cyr` and
  `src/postgres_backend.cyr` now reuse `_SYS_SOCKET` / `_SYS_CONNECT` from
  `src/ipc.cyr` (earlier in every include order) instead of their own raw 41/42.
- Test code: raw `socketpair` 53 → `SYS_SOCKETPAIR` (declared on both Linux
  peers; 53 is `fchmodat` on aarch64), and the `ipc_bind` mode check reads
  `st_mode` through `sys_stat` + the peer's `STAT_MODE` (16 on aarch64, 24 on
  x86_64 — a hardcoded 24 reads `st_uid` there).
- **Every entry point that reaches `src/envelope.cyr` includes
  `lib/chrono.cyr`** — thirteen gained it at 2.7.3, right after
  `lib/syscalls.cyr`; `tests/test_backends.tcyr` already had it. The header
  of `src/envelope.cyr` records the requirement. The four `dist/*.deps`
  sidecars have listed `chrono` since 2.7.1, so a consumer provisioning from
  a sidecar — daimon's shape — needs nothing; a consumer hand-including the
  bundle under `--no-deps` must add the include. Verified with a clean-room probe per profile (sidecar leaves in
  order, then the bundle, then `envelope_new` + `_majra_sleep_ns` + `ipc_bind`):
  zero undefined functions, all four run.

⚠ **agnos behaviour change, stated rather than hidden**: chrono's
`clock_now_ns` reads #95 (`sys_uptime_us`, rdtsc) since cyrius 6.6.1, where
`time_now_ns` read #40 (`sys_uptime_ms`, timer ticks). #40 is frozen for a
foreground `run` program (IF is cleared, the 100 Hz ISR never fires), so every
majra timestamp on that path measured zero; #95 is the only correct monotonic
source there, and the resolution improves from the 10 ms tick to µs.

**Tests that would have caught it, added:** `test_dag_retry` now brackets the
run with `time_now_ns()` and asserts ≥ 10 ms for one retry (it asserted only
the outcome and the attempt count, which a never-sleeping backoff satisfies);
`test_envelope` asserts two envelopes get **distinct** ids (a dead getrandom
was caught by `id_hi != 0`; one returning the same bytes twice was not) and
that `_majra_sleep_ns(5 ms)` sleeps ≥ 5 ms. All three are mutation-verified: a
no-op `_majra_sleep_ns` fails six assertions across two suites; constant
entropy fails the id check. ⚠ On the lane CI runs (x86 only) these detect a
reintroduced raw number only when run on aarch64, so CI also gained a grep
gate — no `syscall(<number>` in code, no `var SYS_*` outside `src/ipc.cyr`'s
five routed rows — and an aarch64 **cross-build** gate that fails on any
`duplicate symbol 'SYS_` / raw-syscall diagnostic. Both mutation-verified
against a reintroduced 35 and a reintroduced `var SYS_GETRANDOM`.

### Fixed — `cbarrier_arrive_and_wait` never arrived, and crashed one run in sixteen

⛔ **Arch-independent, and as old as the file (2026-04-08).** Found because the
aarch64 run of the expanded suite SIGSEGV'd intermittently after
`cbarrier_force: ok`; bisected, then caught under qemu's gdbstub with a
`CYRIUS_SYMS` map: `_map_find+0xf8`, reading through a pointer that was a
hashmap *entry*, not a map.

`cbarrier_arrive_and_wait` passed `load64(cbs)` — the map — to
`_cbarrier_do_arrive`, whose first line is `load64(cbs_ptr)` again. So
`map_get` received the map's **entries array** as a header. With slot 0
empty the fake header has `cap = 0`, the probe loop never runs, and the call
returned `ERR_BARRIER` immediately **without arriving** — every time, on every
arch, since the 2.0.0 cutover the file arrived in. With slot 0 occupied (whichever hash seed put the name
there: one run in sixteen), `cap` is that entry's value pointer, the probe
index is masked against it, and `_map_find` reads through a wild pointer.
**Measured at 2.7.2 on native x86: 14 SIGSEGVs in 200 runs of
`tests/test_core.tcyr`** — the CI "Expanded tests" step has had a ~7 % chance
of failing on every push, and two hardening passes walked past it because the
threaded test asserted only that three workers incremented a counter, which an
instant error return satisfies.

The fix is the one argument. The test now proves the *blocking*: two of three
workers arrive, 50 ms pass, and the done-count must still be **0** before the
third is spawned; every return code must be 0; a single-participant barrier
releases at once with 0; an unknown name is `ERR_BARRIER`, not a hang.
Mutation-verified by restoring the 2.7.2 argument: three assertions fail, and
one of three runs died mid-test. After the fix: **0 crashes in 200 native runs and
100 `qemu-aarch64` runs.**

### Fixed — the core ratelimit tests were sensitive to a 1 ms window

`test_ratelimit` (and two expanded-suite siblings) built their burst-then-reject
buckets at 1000 tokens/sec, so the fourth check had to land within 1 ms of the
first or a token had already refilled. Natively four checks take ~14 µs;
under `qemu-aarch64` the **first** call alone costs ~1.2 ms (TCG translating
the map and allocator paths cold) and `rl reject after burst` failed for a
reason unrelated to the limiter. Those buckets are 1 token/sec now — same
assertions, a one-second window.

### Changed — cyrius 6.6.2 → 6.6.4, and the lock grows a pin trailer

- **Pin `6.6.2` → `6.6.4`.** `lib/` resynced from the 6.6.4 snapshot (110
  files, `lib/unicode/` included); folded modules move **sigil 3.12.16 →
  3.12.18, patra 1.14.1 → 1.14.3, sandhi 1.9.16 → 1.9.17, sakshi 2.5.1 →
  2.5.2**. No `src/` symbol changed meaning under the new pin; the bundle
  bodies moved only where this release's own fixes moved them.
- **`cyrius.lock` is 110 hashes + a `cyrius<TAB>6.6.4` trailer, sorted.** Two
  6.6.x resolver fixes land at once: 6.6.3 sorts the lock (it was written in
  readdir order, so a lock committed from one filesystem could not verify on
  another), and 6.6.4 stamps the pin as a trailer and **refuses** a stdlib leaf
  whose snapshot hash moved under an unchanged pin (`cyrius deps --relock` is
  the explicit accept). 2.7.2's 66-entry lock was the residue of a bare
  `cyrius deps` into an empty `lib/`; this one is the full `lib sync --full`
  snapshot, which is what CI provisions and verifies. `cyrius deps --verify`:
  110 verified, 0 failed.
- **sigil 3.12.18 depends on `lib/sys.cyr`** (its `agnosys_uname` routes
  through `sys_uname` instead of a raw x86_64 `syscall(63, …)`, which is
  `read(2)` on aarch64). `tests/test_backends.tcyr` includes `lib/sys.cyr`
  before `lib/sigil.cyr` — without it the build still reported `OK` but warned
  `undefined function 'sys_uname'` and lowered it to a trapping `ud2` — and
  `"sys"` joins `[deps].stdlib`. Presence is what matters, not order: a unit
  that includes sigil first and sys later builds and runs (forward references
  resolve through the fixup table), which is the order the three named
  sidecars use. `distlib`'s compile-verify infers `sys` into all four sidecars
  on its own (measured: with the declaration removed, `dist/majra.deps` still
  gains it as a "re-added leaf"); the declaration is what keeps `cyrius audit`
  / `cyrius bench` — which prepend the declared list — building clean.
- Formatter drift in five files (`src/dag.cyr`, `src/ipc_encrypted.cyr`,
  `src/ws.cyr`, both `.tcyr` suites — continuation indent only, present under
  6.6.2 as well) and eight pre-existing lint warnings (blank lines, three
  >120-byte lines) cleared, so `cyrius fmt --check` and `cyrius lint` are
  clean across `src/`, `tests/`, `fuzz/`, `benches/`, `examples/`.

### Known issues

- **`duplicate fn 'uname_release'` between `lib/sigil.cyr:746` and
  `lib/sys.cyr:203`** — sigil 3.12.18 still defines a byte-identical copy of
  the accessor it now depends on `lib/sys.cyr` for. Upstream (sigil), harmless
  (last definition wins, same body), and visible to any consumer that links
  both under the 6.6.4 snapshot — all four majra sidecars name both. The
  warning names whichever file came second (`lib/sys.cyr:203:1 … first
  defined in lib/sigil.cyr` when sigil precedes sys, as the three named
  sidecars order them).
- **aarch64 CI lane still build-only.** Everything this release fixes was
  verified locally under `qemu-aarch64` (recipe in
  [`docs/guides/testing.md`](docs/guides/testing.md)); CI now cross-builds
  the four suites for aarch64 and fails on syscall diagnostics, but does not
  run them. The filing's step 7 — a build-only lane proves little here, since
  two of the three sites emitted no warning — stands as the roadmap item.
- `ws_recv_frame` / `ws_send_text` still collide with `lib/ws.cyr` (unchanged
  from 2.7.1). Queued for 2.8.0 as `majra_ws_send_text` /
  `majra_ws_recv_frame` under `semver.md`'s collision exception — see
  [`roadmap.md`](docs/development/roadmap.md) § 2.8.0.

## [2.7.2] - 2026-09-10

Toolchain patch: **cyrius 6.5.36 → 6.6.2**, the `Result` / `Option` / `Either`
*value form*. All **479 assertions** pass (core 299 · backends 152 · patra-queue
28), three fuzz harnesses and the 15-benchmark suite are clean, and both
examples build and run.

### Fixed — `encrypted_ipc_send` returned a tag with a garbage payload

⛔ **Silent, and the reason this is a Fixed rather than a Changed.** Since
cyrius 6.6.0 a `Result` is a two-register `(tag, payload)` value. `src/ipc_encrypted.cyr`
ended `encrypted_ipc_send` with:

```
var rc = ipc_send_frame(load64(e), frame, frame_len);
fl_free(frame);
mutex_unlock(mtx);
return rc;
```

The single-value bind kept only the **tag**, so `return rc;` handed the caller a
Result whose payload was whatever happened to be in `rdx`. On the `Ok` path that
is a bogus byte count; on the `Err` path `err_code_of` reads a garbage error
code. The frame must be freed and the mutex released before returning, so the
Result cannot simply be forwarded — it is now rebuilt explicitly from both
halves.

`encrypted_ipc_recv` had the matching shape: `err_code_of(frame_result)` passed
one argument to the stdlib's two-argument accessor, and `payload(frame_result)`
called an accessor cyrius 6.6.0 deleted. Both now bind the pair.

### Fixed — `./lib/` had 37 undeclared files shadowing the pinned stdlib

`cyrius build` warned that seven bundled libs differed from the version-pinned
snapshot (ganita, niyama, yukti, vani, mabda, sankoch, yantra — all *older* than
the pin). The resolved `lib/` held **103** files against a manifest that declares
**66**: the residue of a `cyrius lib sync --full`, which dumps the entire stdlib
snapshot rather than the declared module set. Re-resolved from empty
(`rm -rf lib && cyrius deps`); the built binary is byte-identical, so nothing
was reaching the stale copies — but the lock *was* recording them:
`cyrius.lock` had **108** entries and had not been rewritten since 2026-08-30,
because majra declares no `[deps.NAME]` git entries and plain `cyrius deps`
returns before the lock write. Regenerated with the explicit `cyrius deps --lock`;
now 66 locked, 66 verified, 0 failed.

### Changed — value-form migration

`tests/test_backends.tcyr` binds both halves at six call sites
(`encrypted_ipc_rekey` ×2, `ipc_send_frame`, `ipc_recv_frame` ×2, `ipc_bind`)
and drops four `payload()` reads. `src/main.cyr`'s `test_error` uses the
two-argument `err_code_of(tag, val)`.

Note that `is_err_result(ipc_send_frame(a, "", 0))` — a Result passed directly
as a call argument — needs no change: the tag lands in the first parameter
register, which is exactly what `is_err_result` wants.

## [2.7.1] - 2026-08-30

Namespace and toolchain patch. No behaviour change: **479 assertions** pass
across the three non-infra suites, and the 15-benchmark suite shows no
regressions.

### Fixed — flat-namespace collisions with libro and the stdlib

Cyrius has one flat function namespace, so two libraries a consumer links that
define the same name resolve by include order and the loser is silently
shadowed. Three of majra's internal symbols were doing that:

- **`_sub_new` → `_majra_sub_new`.** `libro` defines `_sub_new` too, with a
  different signature *and* different semantics: `(pattern)` allocating 24 bytes
  via `alloc`, against majra's `(chan, filter_fn)` allocating 40 via `fl_alloc`.
  In a consumer linking both — daimon does — majra's won by include order, so
  `libro.cyr`'s one-argument call reached majra's two-argument function with an
  uninitialised second argument and the wrong allocation size. Reported from
  daimon 2.1.1.

- **`sha1` → `_majra_sha1`, `_sha1_rotl32` → `_majra_sha1_rotl32`.** The cyrius
  stdlib now ships `lib/sha1.cyr`, whose `sha1` has a **different arity** —
  `sha1(data, len, digest_out)` against majra's `sha1(data, len)`. A consumer
  linking both would bind two-argument calls to a three-argument function.
  Both symbols are internal to the WebSocket upgrade handshake, called from one
  site in `src/ws.cyr`, and appear in no public API listing, so this is not a
  breaking change.

### Fixed — the build was broken

`[deps].stdlib` never declared **`chrono`** or **`random`**, though `src/`
calls `clock_now_ms`, `clock_now_ns`, `clock_epoch_secs`, `sleep_ms` and
`random_bytes`. `cyrius build src/main.cyr` failed outright with *"refusing to
emit binary with 2 reachable undefined function(s)"*. The manifest's own comment
claimed the list was a "legacy hint" superseded by `lib sync --full` — it is
not: the list still drives which modules are auto-prepended, so an undeclared
module is vendored into `lib/` but never included. Both are now declared.

### Changed

- **Cyrius pin `6.5.35` → `6.5.36`**; `lib/` resynced with `lib sync --full`
  (108 files), `cyrius.lock` regenerated, all four dist profiles rebuilt.

  The lock regeneration is a required step here and easy to miss: `lib sync`
  does not touch `cyrius.lock`, and `cyrius deps` only rewrites it after it
  actually copies something. majra declares no `[deps.NAME]` git dependencies —
  only `[deps].stdlib` — so `cyrius deps` is a no-op and nothing regenerates the
  lock, leaving CI's `cyrius deps --verify` failing on the nine stdlib files that
  changed between 6.5.35 and 6.5.36 (`io`, `sandhi`, `sankoch`, `sigil`,
  `tls_native_hs12` and four `syscalls_*`). The fix is `cyrius deps --lock`,
  which rehashes `lib/` directly. Sibling repos with git deps do not hit this,
  because their `cyrius deps` run rewrites the lock as a side effect.

### Known issues

- **`ws_recv_frame` and `ws_send_text` still collide with `lib/ws.cyr`**, and
  majra's implementations differ from the stdlib's. Unlike the symbols fixed
  above, these are **documented public API** — README points consumers at them
  directly — so renaming them is a breaking change and belongs in a minor, not
  this patch. Until then, a consumer that links both majra and the stdlib `ws`
  module gets whichever came last in include order.

## [2.7.0] — 2026-08-22 — finishing what the audits deferred

**629** assertions green across four suites (core 299, expanded/backend 152,
patra-queue 28, core-binary 150), 0 failed. Fuzz 3/3, soak 4/4, benchmarks
17/17, examples 2/2, `cyrius deny` 0 violations.

The four additive APIs the two hardening passes deferred. A MINOR because each
adds public functions; **no existing signature changed** and no existing
behaviour changed except where noted below.

### `pubsub_unsubscribe` + per-subscriber lag policy

The deferral both audits documented most carefully. Fan-out blocks by design —
2.5.3 established that backpressure contract — so a subscriber ABANDONED
without draining wedged publishes to its topic permanently, and there was no
unsubscribe with which to break it.

`chan_try_send` was tried during the first audit and rejected: it trades a
wedge for silent message loss. So the policy is now per subscription and the
caller's to choose, with **BLOCK remaining the default**:

| Policy | On a full ring |
|---|---|
| `PUBSUB_LAG_BLOCK` | park until there is room (default, unchanged) |
| `PUBSUB_LAG_DROP_NEWEST` | discard the message being published |
| `PUBSUB_LAG_DROP_OLDEST` | evict the oldest queued message, then enqueue |
| `PUBSUB_LAG_UNSUBSCRIBE` | self-heal: the subscription deactivates itself |

New: `pubsub_unsubscribe`, `pubsub_unsubscribe_pattern`,
`pubsub_subscribe_with_policy`, `pubsub_set_policy`, `pubsub_subscriber_count`,
`pubsub_dropped_count`. Drops are counted per subscription, so a lagging
consumer is observable rather than silent.

**Unsubscribe tombstones, it does not remove.** `pubsub_publish` snapshots each
subscriber list and walks it unlocked, which is sound only because pushes
append and never shift — the invariant 2.5.3 relied on. A `vec_remove` would
move live entries under an in-flight walk, so unsubscribe clears an `active`
flag instead. Reclamation replaces the vec rather than shifting one, so an
in-flight walk keeps reading valid memory.

Consequence, documented on the function: **delivery stops promptly, not
instantly.** A publish already walking its snapshot may deliver one more
message, so the channel stays valid and caller-owned.

### Parallel DAG tier execution — opt-in, and measured first

`dag.cyr` executed tiers serially while the module header claimed
`thread_create`/`join` parallelism. The roadmap gated this on a measurement
rather than an intuition, so it was measured:

| | |
|---|---|
| `thread_create` + `thread_join` | **89.4 µs** |
| a CPU-bound step body | **0.184 µs** |
| ratio | **~486×** |

Break-even is a step lasting roughly 89 µs. Below that, parallel tiers are
dramatically *slower*; above it — an HTTP call, a query — the spawn cost
vanishes. majra cannot tell which kind of step it has, so **it does not
choose: the default stays serial** and `workflow_def_set_parallel(def, n)`
opts in, with the numbers recorded at the call site.

Serial and parallel share one `_dag_run_step`, so retry, backoff and the
null-result fallback cannot drift apart. Every step writes into its own job
slot; nothing is shared between workers and **no worker touches the context**.
Tier-mates are independent by construction, so deferring all context writes
until the batch joins is equivalent to writing them as we go — and it means the
executor never has to synchronise against majra's own state. `n` is a hard cap,
applied as bounded batches rather than a thread per step.

Two caveats stated in the code: the **executor must be thread-safe**, and
**POLICY_FAIL is weaker under parallelism** — a failing step's tier-mates have
already run by the time the failure is observed, so the run's outcome stays
deterministic but a sibling's side effects are not prevented.

### Type-tagged heartbeat trackers

The residual half of a memory-safety finding carried across two releases.
`chb_fleet_stats` reads a mutex pointer at offset 16, which on a basic
`hb_tracker` was 8 bytes past the allocation — read, then locked. 2.6.9 added a
caller-declared kind; 2.6.10 found the two-argument constructor still had to
*trust* it, because the two tracker types were indistinguishable from their
pointers.

Both layouts now carry a magic at offset 0, so `majra_admin_new` **detects**.
Detection also overrides an incorrect explicit kind passed to
`majra_admin_new_ex`, so a caller can no longer reintroduce the out-of-bounds
read by lying about its tracker.

### Per-key rate-limit statistics

`/ratelimit` reported the limiter's global counters regardless of the key, so
an operator reading `/ratelimit?key=tenant-a` saw fleet-wide totals. 2.6.9
labelled the response `"scope":"global"`; this is the actual answer.

New: `ratelimit_stats_for_key`, the `ratelimit_key_*` accessors, and
`ratelimit_stats_free`. `/ratelimit?key=<k>` is now `"scope":"key"`; an unseen
key is reported unknown rather than as zeroes that would look like a quiet
tenant.

### Behavioural notes

- `hb_tracker` 16 → 24 bytes, `chb_tracker` 24 → 32, `TokenBucket` 16 → 32,
  Subscriber 16 → 40, WorkflowDefinition 32 → 40. All internal layouts; no
  accessor signature changed.
- The ratelimit eviction test probes freelist recycling with a size-matched
  `fl_alloc`, which must track the TokenBucket's real size or it lands in a
  different size class. Noted in place.

### Testing

582 → 629 assertions. Four of the new suites are **mutation-verified** — the
tracker detection, the pubsub drop counter, the transport vtable arity, and the
parallel-tier overlap each turn red when the fix is reverted. The parallel
tests assert peak observed concurrency, so "it used the width it was given" and
"it never exceeded it" are both checked rather than assumed.

## [2.6.10] — 2026-08-22 — repairing 115 findings introduced 50 new ones

**560** assertions green across four suites (core 150, expanded 252, backend
130, patra-queue 28), 0 failed. Fuzz 3/3, soak 4/4, benchmarks 17/17, examples
2/2, `cyrius deny` 0 violations.

The second P(-1) pass required by [`CLAUDE.md`](CLAUDE.md) step 10 — *"repeat
if heavy"*. 2.6.9 rewrote roughly 7,000 lines repairing the first audit's 115
findings, and **those repairs had never been audited**.

**68 confirmed, 10 refuted — and 50 of the 68 were introduced by 2.6.9 itself.**
Full write-up in
[`docs/audit/2026-08-22-audit-pass2.md`](docs/audit/2026-08-22-audit-pass2.md).

That number is the point of this release. Every 2.6.9 fix was individually
reasonable and the suite was green at every step, and it still shipped a
critical security-control bypass, a memory-safety fix that was a verified
no-op, a self-deadlock, and a use-after-free it created out of a merely-racy
read.

### CRITICAL — the rate limiter fails open after an idle period

To stop fast polling discarding fractional credit, 2.6.9 replaced an
unconditional clock reset with `consumed_ns = refill * 1000000000 / rate_x1000`.
`refill` is bounded by nothing — only `tokens` is clamped to burst — so a bucket
idle long enough overflows i64, `consumed_ns` comes back NEGATIVE, and
`last_ns + consumed_ns` drives the clock **backward**. Every later call then
sees a larger elapsed time, refills to full burst, and returns allowed: a
permanent, silent bypass for that key. For a 1000 req/s limiter that is ~2.6
hours idle, i.e. a reconnecting tenant reaches it unaided.

### The socket-permission fix was a no-op, and the header promised otherwise

2.6.9 called `fchmod(fd, 0600)` **after** `bind(2)`. `bind` creates the
filesystem inode from the sockfs mode masked by umask and `connect(2)` checks
*that* inode; `fchmod` afterwards touches only the sockfs inode. Reproduced
under umask 022: the path stays at **0755** while `fstat` reports 0600.

This made 2.6.9 worse than the gap it replaced, because the module header began
asserting owner-only access and majra checks no peer credentials — so a
consumer trusting it got a world-connectable endpoint where connecting *is*
authenticating. `ipc_bind` had no test at all; it has one now, mutation-verified.

### Other high-severity 2.6.9 regressions

- **The RESP parser truncated ordinary Redis replies.** `if (elem == 0) {
  return arr; }` cannot tell a read failure from a nil bulk string or the
  integer `:0`, so a rate-limiter reply or an `MGET` with a missing key came
  back empty with its remaining elements still queued — offsetting the
  connection permanently. No hostile server needed.
- **Encrypted IPC could self-deadlock.** `recv` was put under the same mutex as
  `send` and held across a blocking `read(2)`, so a receiver waiting on a silent
  peer blocked every send on that handle. Split into independent send/recv
  mutexes.
- **The circuit breaker latched OPEN forever** if a half-open probe never
  reported back.
- **`sha1` stopped being reentrant** — the leak fix moved its schedule to
  module scope, so concurrent callers got a digest of neither message.
- **An existing `.patra` file lost every job on upgrade** — the `STR` → `TEXT`
  change was not detected on reopen, so jobs dequeued with empty payloads and
  were still marked running.
- **`chb_get` became a use-after-free** — it returns the internal `NodeState`
  pointer, and 2.6.9 started freeing those on eviction. Now returns a
  caller-owned copy.
- **`namespace_new`'s new 0 return had no guards behind it.**
- **`mq_dequeue` consumed concurrency slots nothing could return**, ratcheting
  a queue toward its cap until it stopped dequeuing.

### Pre-existing, missed by the first pass

**SQL injection in the PostgreSQL workflow API.** `pg_save_workflow_def`,
`pg_get_workflow_def` and `pg_delete_workflow_def` spliced caller strings into
single-quoted literals with no escaping. Pass 1 found and fixed exactly this
shape in `patra_queue` and did not look here — a lens gap: its `syscall-fs-sql`
lens was pointed at `patra_queue` and `main.cyr`, so `postgres_backend`'s
storage API fell between that lens and the wire-protocol one.

Also: no inbound PostgreSQL message was ever freed (one leak per row on a
multi-row query), and `_barrier_state_new` / `chb_register_with_telemetry` /
`relay_evict_stale_dedup` each still carried the exact defect 2.6.9 fixed in
their twin.

### Tests that could not fail

The pass audited 2.6.9's own new assertions and found tautologies — which is
how the first audit's bugs shipped. `test_ws_framing` ran everything against
`fd = -1`, where the guard path and the failure path both return -1.
`test_resp_limits` asserted only that constants were positive. Nothing anywhere
exercised `transport_send`/`_recv` or the malformed-DAG rejection.

All four are real now, and the two most load-bearing are **mutation-verified**:
dropping `_resp_poison` turns the RESP test red, and restoring `fncall2` makes
the transport test observe `len` as a stack value instead of 7.

### New

- `docs/guides/migration-2.6.9.md` — the consumer migration guide for both
  releases, covering all three signature changes, the behavioural changes, and
  the `.patra` format change.
- `redis_is_alive`, and a RESP parser that poisons a connection it can no
  longer parse rather than letting the caller keep using a desynced stream.
- `mq_take_queued` — removing a queued job without treating it as an execution.
  `fleet_rebalance` and `fleet_deregister_node` were using `mq_dequeue`, which
  refuses at the concurrency cap, so a saturated node could neither be
  rebalanced nor drained.
- Teardown functions for everything majra hands out; see the migration guide.

### Not fixed, deliberately

`majra_admin_new`'s two-argument default still trusts its contract — the two
tracker types are not distinguishable from their pointers, and flipping the
default keeps incorrect callers safe only by silently removing `/fleet` from
every correct one (verified: it turns two legitimate assertions red). The real
fix, a type tag in both layouts, is filed as 2.7.0 work. Full `workflow_execute`
teardown and `_resp_buf`'s process-global line buffer are likewise structural.
The four deliberate deferrals from pass 1 stand unchanged.

## [2.6.9] — 2026-08-22 — the first P(-1) hardening pass

**515** assertions green across four suites (core 150, expanded 236, backend
101, patra-queue 28), 0 failed, from a cold `rm -rf build lib` rebuild. Dep
hashes 108 verified / 0 failed. Fuzz 3/3, soak 4/4, benchmarks 17/17, examples
2/2, `cyrius deny` 0 violations.

The first audit under the P(-1) process, and the first entry in `docs/audit/`.
**115 findings confirmed, 2 refuted** across 23 `src/` modules — every one
produced by one reviewer and re-checked by a separate adversarial verifier
instructed to default to *refuted* when uncertain. Full write-up, method, and
the complete finding list in
[`docs/audit/2026-08-22-audit.md`](docs/audit/2026-08-22-audit.md).

| Severity | Count |
|---|---|
| Critical | 2 |
| High | 30 |
| Medium | 49 |
| Low | 34 |

### ⚠ Breaking changes

Three public signatures moved. All three are unavoidable — the old forms cannot
be made correct.

- **`encrypted_ipc_new(ipc_fd, key_ptr)` → `encrypted_ipc_new(ipc_fd, key_ptr,
  role)`.** Pass `ENCRYPTED_IPC_INITIATOR` at one end and
  `ENCRYPTED_IPC_RESPONDER` at the other. See the critical finding below.
- **`majra_admin_serve(admin, addr, port)`** now takes `addr` as a dotted-quad
  string and parses it. It previously forwarded the value to `sockaddr_in`,
  which wants a packed integer — so the documented `"127.0.0.1"` call bound to
  the low 32 bits of a `char*`.
- **`transport_send` / `transport_recv`** now forward all three arguments the
  vtable contract documents. No in-tree caller existed; a consumer implementing
  the documented 3-arg signature was reading a garbage length.

Also behavioural, without a signature change:

- **`namespace_new` now returns 0** for a prefix containing `/`, `:`, `#`, `+`
  or a control byte.
- **`patra_queue`'s `payload` column is now `TEXT`** rather than `STR`. An
  existing `.patra` file keeps its old 255-byte-capped column; delete or
  migrate it.
- **`mq_job_count`** now counts jobs that have not reached a terminal state,
  because terminal transitions release the record. It previously only grew.

### CRITICAL — both directions of an encrypted IPC channel shared one nonce space

Both endpoints share one pre-shared key, and each derived nonces from its own
counter starting at 0 with **nothing in the 12-byte nonce distinguishing the two
directions**. The first message each way used a byte-identical `(key, nonce)`
pair, and so did every later pair at the same counter.

Under AES-GCM this is the one failure the construction cannot survive: an
identical `(key, nonce)` means an identical keystream, so XORing the two
ciphertexts yields the XOR of the two plaintexts — and a nonce collision leaks
the GHASH subkey, which lets an attacker forge tags for arbitrary messages.
Confidentiality and integrity fall together.

The role now occupies nonce byte 0. Added replay protection at the same time
(AEAD gives integrity, not freshness — a captured frame replayed cleanly
forever): the peer's counter must strictly increase and its nonce must carry
the *peer's* role, so an attacker cannot reflect our own frames back at us.

### CRITICAL — PostgreSQL NULL columns crashed the client

`_pg_read_be32` composes four **zero-extended** `load8` results, so it can never
return a negative number: PostgreSQL's NULL sentinel (int32 `-1`, on the wire as
`FF FF FF FF`) came back as `4294967295`. The `col_len < 0` NULL guard in
`pg_query` was therefore unreachable dead code, and a NULL column took the value
branch — `fl_alloc(4294967296)` plus a 4 GiB `memcpy` out of a DataRow body a
few bytes long.

Reachable from ordinary traffic, not a hostile server: majra's own
`pg_init_workflow_tables` declares five nullable columns. `test_live` never
fired it because it only selects non-NULL literals. Verified with a compiled
probe before fixing, and fixed with a sign-extending `_pg_read_i32`.

### Highlights from the 30 high-severity findings

- **WebSocket never implemented the 127 / 64-bit length form**, so the sentinel
  was used as a literal payload length and the eight length bytes were consumed
  as payload — a peer-controlled frame desync. External research put two 2026
  CVEs on this exact code shape (CVE-2026-54466, CVE-2026-1528).
- **`ws_send_text` emitted headerless payloads** at 65536 bytes or more: the
  if/elif chain had no else.
- **RESP bulk length wrapped into a tiny allocation.**
  `$9223372036854775807` makes `blen + 2` wrap negative; `_fl_class` routes a
  negative size down the `size <= 16` path, so `fl_alloc` returned a 16-byte
  block that `str_new` labelled `INT64_MAX` long.
- **`redis_connect` and `pg_connect` ignored `host`**, always dialling
  loopback.
- **The managed queue's concurrency cap was advisory** — checked in one
  critical section, incremented in another.
- **`mq_cancel` had no effect on delivery**; dequeue overwrote the state
  unconditionally.
- **`fleet_rebalance` corrupted its source node's accounting**: a steal marks
  the job RUNNING and nothing reversed it, so a node eventually stopped
  dequeuing entirely.
- **`relay_send` emitted out of sequence order** — the sequence was claimed
  under the mutex, the fan-out done after releasing it.
- **The token bucket refilled at zero under fast polling**: the truncated
  refill was 0 but the clock was reset anyway, discarding fractional credit.
- **`patra_queue` spliced caller payloads into SQL literals**, and discarded
  every `patra_exec` return.
- **`cbarrier_arrive_and_wait` returned results through file-scope globals**
  serialised only by the per-set mutex, so concurrent barrier sets clobbered
  each other.
- **The admin endpoint read 8 bytes past an `hb_tracker`** and locked whatever
  it found.
- **`namespace_new` was unvalidated**, and it is a tenant-isolation boundary: a
  `#` in a prefix turned that tenant's own wildcard into a cross-tenant
  subscription.
- **`src/ipc.cyr`'s socket was left at the ambient umask** (world-connectable
  on a typical 022 process) with no peer-credential check — connecting *was*
  authenticating. Now 0600.

### Performance

Measured head-to-head against a `dda5b72` build of the same benchmark, three
trials each: `pubsub_publish_nosub` **-35%**, `fleet_stats_100` **-16%** (both
from removing `map_keys` bump allocations from hot paths), `ratelimit_check`
and `circuit_state` unchanged. One regression below the flag threshold:
`pq_enqueue` **+7.9%** — the priority-queue code was not modified, so this is
most likely code-layout movement.

`circuit_state` first regressed 2ns → 49ns from the mutex added for the
half-open probe gate; a lock-free fast path for the common below-threshold case
restored it to 3ns.

### Tests

410 → 515 assertions, concentrated on the defects themselves. `src/ipc.cyr`
shipped in all four dist profiles with **no test in any suite** — which is why
its defects went unnoticed — and now round-trips over a real socketpair.

`tests/soak/soak_queue.cyr` asserted `job_count == total_enqueued`; it was
codifying the unbounded-growth leak, and now asserts the map drains to 0.

### Not fixed, deliberately

pubsub's blocking fan-out (a backpressure contract 2.5.3 established and a test
asserts; dropping would trade a wedge for silent message loss), PostgreSQL
SCRAM-SHA-256 + TLS (a feature, not a repair — the connect now fails closed on
auth type 10 rather than downgrading), per-key ratelimit stats, and parallel DAG
tier execution. Each is documented at the call site and listed in the audit.

## [2.6.8] — 2026-08-22 — the folded-module sweep finishes the job

**410** assertions green across four suites (core 150, expanded 200, backend 43,
patra-queue 17), 0 failed. Dep hashes **108 verified / 0 failed**. Fuzz (3/3),
soak (4/4), benchmarks (17/17) and examples (2/2) clean from a cold
`rm -rf build lib` rebuild.

### Changed — `[deps.sigil]` git dep → `[deps].stdlib`

**2.6.7 fixed the version and left the shape.** That release swept every
`[deps.X]` where X is a folded stdlib module and moved sigil 3.12.7 → 3.12.9 to
match the toolchain. It read the hazard as *a stale pin downgrading a folded
module* — true, but the smaller half. The declaration itself was the problem.

`distlib` classifies a git dep **out of the stdlib leaves**, so
`dist/majra-signed.deps` and `dist/majra-backends.deps` were published without
naming `sigil` at all — the two profiles that exist *because* they carry crypto.
Verified in a clean room against the shipped 2.6.7 sidecar: a consumer
provisioning exactly what it declares gets

```
warning: undefined function 'ed25519_init'
warning: undefined function 'ed25519_sign'
warning: undefined function 'ed25519_verify'
warning: undefined function 'ct_eq_bytes_lens'
OK
```

— note the `OK`. An undefined fn lowers to a trapping `ud2`, so this is not a
build failure a consumer would notice; it is a **SIGILL the first time an
envelope is signed**. Both sidecars now carry `sigil`, and the same clean-room
build resolves all four symbols.

This is the identical shape the manifest already warns about for sakshi
(*"kavach hit exactly that"*) — sigil was simply never re-read under that rule
after the 6.5.x fold made it a stdlib module. The `⚠ do not re-add` note now
sits over both.

**Consequence for pinning**: sigil tracks the toolchain fold; the cyrius pin is
the only knob that moves it. `cyrius.lock` drops to **zero git deps** — 108
pure hashes, no commit-pin line — and `cyrius deps` resolves nothing, which
removes the overlay-ordering hazard both 2.5.2 and 2.6.7 were written to
counteract.

### Changed — Cyrius pin 6.5.31 → 6.5.35

Snapshot holds at **108 files**. Three move: `patra` 1.13.9 → 1.13.10, plus
`bayan` and `vani` (majra calls neither). Every other folded module is
unchanged, sigil included — 6.5.35 folds the same 3.12.9 the manifest already
named, so this bump carries no dep-version movement of its own.

No formatter drift: 0/23 `src/` files reformat under 6.5.35, where 6.5.31 had
moved `src/ws.cyr`. Lint output is byte-identical under both pins (the
`src/error.cyr` bare-`ERR_*` notes and the `src/admin.cyr:77` untracked deferral
are pre-existing, and unchanged).

**No benchmark delta.** Rather than assert this, the same `benches/bench_all.bcyr`
was built against both toolchains and run head-to-head. A first-pass read
suggested a +6.4% `pattern_wildcard_+` regression and a −28%
`pubsub_publish_nosub` win; across four further trials both pins converge inside
noise on every target (`pq_enqueue` 567–587ns both, `pattern_wildcard_+` 100–111
vs 101–102ns). The first-run spread was warm-up, not codegen.

### Fixed — `base64_encode` / `base64_decode` → `majra_base64_encode` / `majra_base64_decode`

`src/ipc_encrypted.cyr` defined `base64_encode` and `base64_decode`, and so does
`lib/bayan.cyr` — as thin wrappers over `bayan_base64_encode` /
`bayan_base64_decode`. `cyrius distlib backends` had been reporting it for as
long as bayan has carried those aliases:

```
warning:dist/majra-backends.cyr:4396:1: duplicate fn 'base64_encode' (last definition wins; first defined in lib/bayan.cyr)
warning:dist/majra-backends.cyr:4427:1: duplicate fn 'base64_decode' (last definition wins; first defined in lib/bayan.cyr)
```

The usual duplicate-symbol warning is benign — two definitions that agree, one
of them wins, nobody notices. **These two do not agree.** majra's
`base64_decode` allocates a 16-byte `{ptr, len}` struct and returns a pointer to
it; bayan's returns a scalar `i64`. Under `last definition wins`, a consumer of
`dist/majra-backends.cyr` whose include order puts bayan after majra silently
retargets *majra's own* decode call sites at bayan's implementation, and the
`load64(dec)` / `load64(dec + 8)` unpacking reads whatever happens to sit at
that address. Not a wrong answer — a wild read.

It was never observed corrupting anything in-tree, because
`tests/test_backends.tcyr` establishes an include order that puts majra last and
the bundle is emitted in that same order. But that is ordering doing the work,
not correctness, and the ordering belongs to the *consumer*, not to us.

Renamed with the `majra_` prefix already used for the other cross-cutting public
surface (`majra_admin_*`, `majra_err`). Both warnings are gone — confirmed
against this release's manifest, not just the one the fix was written on, since
the sigil stdlib-leaf move above changes how `distlib` classifies leaves. The
private helpers `_b64_encode_byte` / `_b64_decode_byte` / `_b64_chars` were
already underscore-scoped and don't collide, so they keep their names.

Only `dist/majra-backends.cyr` moves — the base, signed, and admin profiles
deliberately exclude `src/ipc_encrypted.cyr` and `src/ws.cyr`.

**Why this ships in a PATCH.** A name whose meaning was decided by the
consumer's include order was never covered by the API promise, so renaming out
of the collision isn't an API change to break. Recorded in
[`docs/development/semver.md`](docs/development/semver.md) § Documented
exceptions, which this release also adds, along with the two conditions that
gate it — the definitions must genuinely disagree, and the rename must produce a
compile error at every affected call site.

**Consumer migration.** A consumer calling majra's `base64_encode` /
`base64_decode` gets a compile error, not a silent behaviour change. Two ways
out:

- **Want majra's `{ptr, len}` decode** — rename the call to
  `majra_base64_encode` / `majra_base64_decode`.
- **Only ever wanted a base64** — call bayan's `base64_encode` /
  `base64_decode` (or `bayan_base64_*` directly) and drop the majra dependency
  for that call. Note the different decode contract: scalar, not `{ptr, len}`.

### Bundle movement, in full

Three of the four bundle bodies stay byte-identical — for `majra`,
`majra-signed` and `majra-admin` the whole `dist/*.cyr` diff is a banner line,
because those profiles deliberately exclude `src/ipc_encrypted.cyr` and
`src/ws.cyr`. **`dist/majra-backends.cyr` moves**, and only by the rename: two
`fn` definitions and one call site.

All four `.deps` sidecars are regenerated; `majra`, `majra-signed` and
`majra-backends` gain `sigil`.

## [2.6.7] — 2026-08-20 — the last folded-module pin that lagged the toolchain

**410** assertions green across four suites (core 150, expanded 200, backend 43,
patra-queue 17), 0 failed. Dep hashes 108 verified / 0 failed. Fuzz, benchmarks
and examples clean.

### Changed — `[deps.sigil]` 3.12.7 → 3.12.9

**This is the fix.** sigil is a *folded* stdlib module, and `cyrius deps`
applies a declared dep's copy on top of the `lib sync --full` snapshot on every
resolve. A `[deps.sigil]` behind the fold therefore downgrades `lib/sigil.cyr`
for every transitive consumer — and majra sits under both **agnosai** and
**bote**.

It was never actually observed downgrading anything: agnosai pins sigil 3.12.9
directly and libro pins 3.12.9, so in practice one of those won and
`lib/sigil.cyr` resolved to 3.12.9 everywhere checked. But that is resolution
*order* doing the work, not correctness — the identical shape, in patra, is what
broke agnosai's CI at 2.0.2 and took four repos to trace.

Found by sweeping the whole dependency closure — every `[deps.X]` where X is a
folded stdlib module, compared against what the pinned toolchain ships. This was
the only remaining mismatch of five such declarations.

### Changed — Cyrius pin 6.5.20 → 6.5.31

Eleven minors behind. Picks up the folds shipped since (sakshi 2.4.11, patra
1.13.9, yukti 2.3.8, niyama 1.0.7, mabda 4.1.0, ganita 1.1.4, yantra 1.0.3).

`src/ws.cyr` reformatted for 6.5.31's canonical continuation indent — 0/23 files
were unformatted under 6.5.20, 1/23 under 6.5.31, so this is the formatter
moving, not drift. `git diff -w -- src/` is empty. Only `dist/majra-backends.cyr`
changed as a result, since the base/signed/admin profiles deliberately exclude
the backend modules.

### Fixed — `version-bump.sh` told you to regenerate 2 of 4 bundles

Its next-steps line read `cyrius distlib && cyrius distlib backends`, omitting
`signed` and `admin`. The CI gate checks all four and fails on any stale bundle,
so following the script's own instructions produced a red build. Corrected to
list every profile.

## [2.6.6] — 2026-08-13 — the relay's fan-out could wedge its sender

### Fixed

- **relay** — **a full subscriber ring blocked the SENDER, forever.** Both
  fan-out paths pushed with `chan_send`, which futex-waits for space; Rust's
  `Relay` is a `tokio::sync::broadcast` and `Relay::send` is
  `let _ = self.tx.send(..)` (`fleet/relay.rs:133`) — it **never blocks**. A full
  ring overwrites its oldest slot and the lagging receiver observes
  `RecvError::Lagged`. Now `chan_try_send`, so a full ring drops rather than
  waits.

  ⚠ **2.6.5 is what made this reachable.** Before it, every subscriber channel
  was 256 deep regardless of the capacity asked for, so wedging needed 256
  undrained messages; honouring the capacity made it reachable at whatever depth
  the caller names — `relay_with_capacity(id, 2)` deadlocked on the **third**
  send. Reproduced directly: three sends against a depth-2 relay never returned,
  killed by `timeout`. majra has no unsubscribe, so the wedge is permanent
  rather than transient, and an agnosai caller wedges a pooled worker thread.

  Found by an adversarial review of the 2.6.5 change set, not by the suite —
  the 2.6.5 capacity test filled the ring with `chan_try_send` **directly on the
  channel**, routing around `relay_send` and so around the blocking call.

### Added

- A deadlock guard: a depth-2 relay takes a third and fourth `relay_send` and
  both return. Mutation-verified — restoring `chan_send` hangs the suite
  (`timeout` rc 124) rather than failing an assertion.

## [2.6.5] — 2026-08-13 — the relay's capacity was discarded and its timestamp was unportable

### Fixed

- **relay** — **`capacity` was accepted and thrown away.** `relay_subscribe`
  hardcoded `chan_new(256)`, so a relay built asking for 4 and one asking for
  4096 behaved identically; Rust's `Relay::new` sizes its `broadcast::channel`
  from the argument. New `relay_with_capacity(node_id, capacity)` and
  `relay_capacity(r)`; `relay_new` delegates with the 256 default, so nothing
  that does not ask for a depth changes. A non-positive capacity falls back
  rather than building a channel nothing could be sent through.

- **relay** — **the message timestamp was CLOCK_MONOTONIC.** A RelayMessage is
  serialised and sent to another node, and Rust stamps it `DateTime<Utc>` — a
  portable instant. `time_now_ns()` measures from an arbitrary per-boot zero, so
  the number carried on the wire was meaningless outside the emitting process:
  two nodes could not order each other's messages by it, and a persisted one
  could not be read back after a reboot. Now `clock_epoch_ns()`.

  ⚠ **This changes what the field MEANS, not any decision majra makes.** Dedup
  and ordering are by `seq`, and nothing in the module compares timestamps.

  ⚠ **The wall-clock reader is majra's own `time_epoch_ns` (`src/envelope.cyr`),
  NOT the stdlib's `clock_epoch_ns`.** `chrono` is not among the modules a
  `--no-deps` build prepends, and `--no-deps` is exactly what CI passes — so the
  stdlib call compiled locally, where a full `lib sync` had put `chrono.cyr` on
  disk, and failed in CI with `undefined function 'clock_epoch_ns'`. Adding
  `chrono` to `[deps] stdlib` does **not** fix it; that list controls
  provisioning, not what a `--no-deps` build has in scope. `time_epoch_ns` sits
  beside the `time_now_ns` that already does this dance and differs only in the
  clock id (`CLOCK_REALTIME` rather than `CLOCK_MONOTONIC`), so majra's time
  sources stay in one place and no dependency is added.

  Both reported by agnosai 2026-08-13, which had carried them in
  `src/fleet/relay.cyr` as "owed to majra".

### Added

- `relay_msg_timestamp(m)` — there was no accessor for offset 40 at all.
- `RELAY_DEFAULT_CAPACITY` (256), named rather than repeated.
- 9 relay regression tests (`test_core` **189 -> 198**). Mutation-verified:
  restoring the hardcoded `chan_new(256)` and reverting to `time_now_ns()` each
  fail loudly. A depth of 2 is the capacity discriminator — the third send with
  nobody draining must be refused, where 256 swallows it.

### Changed

- The `Relay` struct grows **96 -> 104 bytes**; `capacity` is **appended**, so no
  existing field offset moves.

## [2.6.4] — 2026-08-13 — the rate limiter never refused anything

### Fixed

- **ratelimit** — **a key built per request got a fresh full-burst bucket every
  time, so neither limiter refused anything.** `ratelimit_check` and
  `sliding_window_check` store the caller's key pointer directly
  (`map_set` is `store64(ep, key)` — a borrow, not a copy), and the entry lives
  until eviction. Every real caller derives its key per request — a header, a
  peer address, a token — so the map was holding a pointer the caller was free
  to reuse or release, and once it did, the bucket became unreachable under its
  own key. Both limiters now own their key (`_ratelimit_key_own`, an `fl_alloc`
  copy on the same freelist as the bucket).

  ⚠ **It passed the obvious test.** A string literal is one pooled address
  reused at every call site, so `ratelimit_check(rl, "k1")` in a loop shares a
  bucket and looks correct — which is how this survived since the module was
  written. The regression tests build every key at a fresh address.

  Reported by agnosai on 2026-08-13, measured through its HTTP handler: three
  identical requests produced **3 active keys and 0 rejections**.

- **ratelimit** — **the eviction sweep leaked its own scratch on every run.**
  `map_keys` builds a vec and `to_remove` was a second one; both come from
  `vec_new_a(default_alloc())` — the global **no-free** bump — and `vec_push`
  growth abandons each old buffer there as well. A 100k-key sweep burned well
  over a megabyte that never came back, so the routine whose entire purpose is
  bounding a limiter's memory consumed more of it the better it worked. The
  sweep now **allocates nothing**: it walks the entries array through the map's
  own public accessors (`map_entries` / `map_cap`), the same iteration
  `map_keys` and `map_iter` do internally. Deleting during the walk is safe
  because `map_delete` only tombstones the slot it finds — it never moves,
  reallocs or rehashes.

- **ratelimit** — **eviction leaked both halves of every bucket it removed.**
  `ratelimit_evict_stale` called `map_delete` and dropped the key and the
  16-byte bucket on the floor, so the one mechanism built to bound a limiter's
  memory grew it instead — unbounded under churning keys. Both are now returned
  with `fl_free`, after the `map_delete` that needs the key to find the entry.
  Safe because `_map_find` never key-compares a tombstone and `_map_grow_a`
  rehashes only live entries, so the stale pointer left in the slot is never
  dereferenced.

### Notes

- ⚠ **`ratelimit_check`/`sliding_window_check` take a cstr, not a `Str`.** The
  bucket map is `KeyTypeCstr` — it hashes with `hash_str` and compares with
  `streq`, both reading bytes from the pointer. A `Str` VALUE passed instead is
  read as its header, whose first eight bytes are the data pointer, so identical
  content at two addresses hashes two ways. That mismatch is what agnosai hit.
  Callers holding a `Str` must pass `str_cstr(s)`. Documented at both entry
  points; the signatures are unchanged.
- ⚠ `sliding_window_*` still has **no eviction sweep**, so its entries live for
  the process. That predates this release and is unchanged by it; only
  `ratelimit_*` is bounded in time.

### Added

- **ratelimit** — `total_evicted` in `ratelimit_stats` (offset 24) is now wired
  to a real counter, cumulative across sweeps, and surfaced by
  `_admin_ratelimit_json`. It had been hardcoded `0` with an "unused for now"
  note; with eviction now reclaiming memory, the churn it measures is worth
  seeing. `RateLimiter` grows 48 -> 56 bytes, which is the same freelist class.
- 6 ratelimit/sliding-window regression tests — `test_core` **159 → 189
  assertions**, plus one in `test_backends` for the new `evicted` field (**242 →
  249** across the three runnable suites). Every one keys on bytes at a fresh
  address, because a string literal is one pooled address reused at every call
  site and a pointer-keyed limiter passes happily under literals.
  Mutation-verified, 7 probes / 7 kills: borrowing the key in either limiter,
  dropping either `fl_free` in the sweep, swapping the map to `map_new_str`, and
  a sweep that ignores its idle threshold are all caught.

## [2.6.3] — 2026-08-12 — the `fl_alloc` stopgap is retired; upstream fixed it properly

### Changed

- **Toolchain pinned to cyrius 6.5.20** (was 6.5.18), which makes **`fl_alloc`
  thread-safe** — the defect majra reported from `test_relay_receive_is_reentrant`
  on 2026-08-10. The fix landed in 6.5.19; 6.5.20 is taken because it re-folds
  **patra 1.13.0**, and patra reaches majra as a stdlib module. patra 1.12.12 —
  what every earlier snapshot folded — carried its own `[deps.sakshi]` at
  **2.4.2** against the 2.4.10 the same snapshot shipped, and `cyrius deps`
  overlays a git dep's resolution on top of the `lib sync --full` snapshot on
  **every `cyrius build`**, recursing through sibling manifests. majra was never
  hit (it consults no patra manifest — patra arrives from the fold, and majra's
  `src/` calls no sakshi symbol), but the pin should name a snapshot without the
  hazard in it.

  The filing described **one** race: two threads popping the same block off
  `_fl_heads[cls]`. Upstream found **five** and locked all of them behind a
  process-wide CAS spinlock — `fl_init`'s check-then-set, the pop, the push, the
  arena bump, and an arena refill whose `mmap` left a **~2 µs unlocked window**
  (roughly 1,000× wider than the pop) that could return a block **running off the
  end of its mapping**. The large (>4096) path stays lock-free; it touches no
  shared state. The lock costs nothing until threads exist (a `_threads_active`
  gate).

- **`relay_receive_ex` allocates its result struct AFTER the unlock again.**

  2.6.1 pulled the 16-byte `fl_alloc` *inside* the critical section as a
  stopgap, because an unsynchronised `fl_alloc` could hand two concurrent
  `relay_receive` callers the same block — which corrupted the dedup table (a
  message arriving under another sender's sequence, then rejected as a
  duplicate) and, under real contention, faulted.

  ⚠ **That stopgap was never a complete fix**, and the header said so: it
  serialised majra against *majra* while any other thread in the process calling
  `fl_alloc` still raced it. With the allocator fixed upstream, the block goes
  back outside the lock, which is where it belongs — it is private to the call
  and nothing under the lock reads it. **The lock is held across a
  subscriber-list walk on a hot pub/sub path**, so shortening it is the point of
  undoing the workaround, not a cosmetic tidy.

  No API change; `relay_receive` / `relay_receive_ex` return the same struct.

## [2.6.2] — 2026-08-11 — the priority queue: O(n²) drain, and an unguarded negative index

### Performance — a pop is now O(1) amortised, and 6,000x faster at depth

`pq_dequeue` took the front of a tier with `vec_get(tier, 0)` followed by
`vec_remove(tier, 0)`. `vec_remove` shifts every remaining element down a slot, so
**one pop cost O(n) in the tier depth and draining a queue of n cost O(n²)**.

Measured, mean cost of a single `pq_dequeue` while draining a tier of that depth:

| depth | before | after |
|---|---|---|
| 2,000 | 2.00 µs | **34 ns** |
| 4,000 | 4.02 µs | **34 ns** |
| 8,000 | 7.92 µs | **33 ns** |
| 16,000 | 15.56 µs | **34 ns** |
| 200,000 | 198.70 µs | **33 ns** |

Before, doubling the depth doubled the per-pop cost — the signature of a linear pop
rather than cache noise; at 200,000 a full drain was roughly **40 seconds of memmove**.
After, the cost is flat across a 100x range, which is what O(1) looks like.

⚠ **This was never the pathological case — it is the DESIGN case.** A priority queue
exists so that low-priority work is *allowed* to accumulate behind high-priority work,
so the deep-tier path is precisely the one that has to stay cheap. Reported by a
consumer (agnosai) whose `llm/inference_queue` puts background summarisation behind
interactive crew tasks.

**How.** Each tier gains a read index; a pop advances it instead of moving the
survivors, and the consumed prefix is reclaimed once the head passes the midpoint —
so each surviving item is copied at most once per doubling of the head, amortised
O(1). A fully drained tier costs nothing to reclaim: length and head both reset with
no copying, which is the common shape for a queue that empties between bursts.

⚠ **Swap-with-last was NOT used**, though it is the other obvious O(1) trick. It
destroys FIFO within a priority level, which `queue_item_new`'s monotonic id and
`ManagedQueue`'s semantics both depend on.

### Changed

- `PriorityQueue` is **88 bytes**, was 48 — five read heads at offsets 48-87. The
  tier pointers (0-39) and the total (40) keep their offsets, so `pq_len` and
  `pq_is_empty` are untouched. Nothing outside `src/queue.cyr` reads the struct
  directly; verified by grep before the layout change.

### Fixed — `pq_enqueue` clamped only ONE end, and the missing end was memory-unsafe

The over-range clamp (`pri >= NUM_PRIORITIES`) had always been there. A **negative**
priority went straight through to `load64(pq + pri * 8)`, which for `pri = -1` reads
eight bytes **before** the struct and then `vec_push`es onto whatever that decoded to.
An out-of-bounds read followed by a write through the result — not a wrong answer, a
memory-safety bug.

Reachable from anything that computes a priority rather than naming one:
`queue_item_new(priority, payload)` stores the caller's value verbatim, so a consumer
mapping its own enum onto this one is a single arithmetic slip from `-1`.

Both ends now clamp to `PRIORITY_NORMAL`.

⚠ **Without the guard the suite does not fail an assertion — it dies mid-test.** The
three preceding queue tests print `ok`, then the process is gone: no assertion message,
no summary line. Verified by removing the guard and running.

`test_queue_priority_clamping` covers `-1`, a far-negative, and an over-range value;
asserts all three land in NORMAL **in arrival order** (clamping must not reorder), and
that a real `PRIORITY_CRITICAL` still preempts a clamped item.

### Changed — lint is clean across `src/`, was 13 warnings in two files

All 13 were `line exceeds 120 characters`, pre-existing and untouched by the
queue work: 8 in `src/ws.cyr`, 5 in `src/postgres_backend.cyr`.

⚠ **`src/postgres_backend.cyr`'s were long SQL literals, and the emitted SQL is
byte-identical** — verified by reassembling every `pg_exec` / `str_builder`
string from `git show HEAD:` and from the new file and comparing the two sets:
**8 strings, identical**. The three `CREATE TABLE` statements now build through
`str_builder`, which is the idiom `pg_save_workflow_def` in the same file already
used; the splits fall on `, ` boundaries inside the column list.

⚠ **A `\`-continued string literal was NOT used**, in either file.
`cyrius fmt` reindents inside multi-line literals and the leading spaces land
**in the string** — which for a SQL statement means silently corrupted DDL. The
`ws.cyr` HTTP/101 response is split into three `add_cstr` calls at header
boundaries for the same reason.

`ws.cyr`'s other seven were the SHA-1 big-endian word assembly and the 20-byte
digest spill; both are now one operation per line and read better for it.

### Added

- `test_queue_deep_fifo_and_compaction` — the existing FIFO test used **three**
  items, too shallow to reach the interesting states. This drains 500 at one
  priority across ~9 compactions, refills after a full drain (the head-reset
  branch), and runs 400 interleaved push/pops so the head advances while the tier
  never empties.

  ⚠ It asserts the **backing vec length**, not just order. Deleting `_pq_compact`
  outright leaves every ordering assertion passing — order is correct either way —
  while the tier grows to hold all 400 pushes forever. That is a memory defect an
  order test structurally cannot see. Both mutants (compaction removed; head not
  reset after compacting) are verified to fail this suite.

**Gate:** 4 suites, **159 + 42 + 36 assertions, 0 failures**, with live Redis and
PostgreSQL. `cyrius lint` clean across every file in `src/`, `cyrius fmt` clean
across `src/`, `tests/` and `benches/`, `vet` 27 deps / 0 untrusted, `deny` 0
violations, `deps --verify` 107/107, all four `dist/` bundles regenerated and
idempotent. Toolchain and deps were already current: cyrius **6.5.18**, sigil
**3.12.7**; `lib/` diffs clean against the pinned snapshot.

## [2.6.1] — 2026-08-10 — `relay_receive` raced the ALLOCATOR, not the relay

### Fixed — a concurrent `relay_receive` could be handed another sender's message

`relay_receive_ex` allocated its result struct with `fl_alloc(16)` **after** releasing its
mutex. **`fl_alloc` is not thread-safe** — `lib/freelist.cyr` manipulates the global
`_fl_heads` free lists with plain loads and stores, no mutex and no atomics — so two
threads in that window could be handed the **same block**.

The symptom was not a crash at the allocation. It was wrong data in the dedup table: a
message arriving under another sender's sequence number and correctly rejected as a
duplicate, surfacing as an intermittent

```
FAIL: every strictly-increasing message from sender-a is accepted
FAIL: no message was wrongly dropped as a duplicate
```

— which reads like a defect in the dedup logic, the one part of this path 2.6.0 had just
audited. Under heavier contention it **faulted** instead of failing an assertion.

⚠ **2.6.0's reentrancy fix was correct and is untouched.** It made the dedup table's
mutation safe. This is a second, independent race in the same function, in an allocation
that happens *after* that mutex is dropped — which is why auditing the locking again
would never have found it. The fix allocates the 16 bytes inside the critical section, a
section that already walks the subscriber list.

Measured, 40 concurrent instances of `tests/test_core.tcyr`:

| build | failures |
|---|---|
| before | **4 / 40**, one a core dump |
| after | **0 / 40** |

A single run passes either way; only contention separates them, which is why this reached
CI rather than a local test sweep.

### Fixed — the reentrancy test was measuring the allocator, not the relay

`test_relay_receive_is_reentrant` built its 400 messages per worker **inside** the worker
threads, with `fl_alloc`. The two workers therefore raced the allocator, and the test
could fail for a reason unrelated to `relay_receive`'s reentrancy. Messages are now built
up front on the calling thread and the workers only read them, so the test measures the
invariant it names.

Filed upstream as
`cyrius/docs/development/issues/2026-08-10-fl-alloc-is-not-thread-safe-and-says-nothing.md`:
`lib/freelist.cyr` documents neither the constraint nor a safe variant, and it ships
beside `thread.cyr`.

### Changed

- **Toolchain pin 6.5.14 → 6.5.18.**
- **`[deps.sigil]` 3.12.6 → 3.12.7.**
- **`sakshi` moved from `[deps.sakshi]` into `[deps].stdlib`, at 2.4.10.**

  That git pin was **defensive**, and the thing it defended against is gone. It existed
  because sigil's own manifest declared `[deps.sakshi]`, and `cyrius deps` overlays a git
  dep's resolution on top of the `lib sync --full` snapshot — so left implicit, sigil
  silently downgraded `lib/sakshi.cyr` on every build behind an unnamed "1 bundled lib(s)
  differ" warning. **sigil 3.12.7 dropped that dep**, so there is nothing to counteract:
  `patra` reaches majra as a *stdlib* module from the snapshot, not as a git dep, so its
  manifest is never consulted. majra's `src/` calls no sakshi symbol and none of the four
  bundles reference one.

  ⚠ **Do not re-add a `[deps.sakshi]` here to "pin" it.** On a library that publishes
  bundles, a git dep makes `distlib` reclassify the module out of the **stdlib leaves**,
  dropping it from the `.deps` sidecars and breaking clean-room consumers — kavach hit
  exactly that and had to revert it.

### Verified

- **241 assertions, 0 failures** across all four suites, including `tests/test_live.tcyr`
  (36) run against **real Redis and PostgreSQL** in Docker rather than skipped — the
  containers, and the `pg_hba.conf` rewrite the CI job performs, were stood up locally so
  the live path was actually exercised.
- `cyrius bench` clean.
- All four `dist/` bundles regenerated — umbrella **and** the `backends` / `admin` /
  `signed` profiles. ⚠ A bare `cyrius distlib` writes only `dist/majra.cyr`; each profile
  needs its own `cyrius distlib <name>`, and skipping them ships a bundle stamped with the
  previous version.
- **40 concurrent instances of `test_core`, 0 failures**, against 4/40 before the fix.
- `lib/sakshi.cyr` holds at **2.4.10 through a build**, with no shadow warning.

## [2.6.0] — 2026-08-08

**`relay_receive` was not reentrant, and three smaller relay defects alongside
it.** All four were reported by agnosai, which drives majra's relay from a
100-worker `sandhi_server_run_pooled` pool. Minor bump rather than patch: two
new public functions and one appended stats field.

### Fixed — `relay_receive` raced itself (Critical for threaded consumers)

Two independent problems, either one sufficient to corrupt a dedup decision:

- **It stashed its arguments and dedup state in FILE-SCOPE globals** —
  `_recv_r`, `_recv_msg`, `_dedup_map`, `_dedup_key`, `_dedup_seq`. Their own
  comment said they "predate cc5", were a cc3 local-clobbering workaround, and
  were kept because removing them "would require a structural refactor". That
  refactor is this release: they are parameters, and the globals are gone.
- **It took no lock at all**, while every other mutating entry point
  (`relay_send`, `relay_subscribe`, `relay_evict_stale_dedup`) locked `r + 40`.
  So the `seen` map was mutated unsynchronised even setting the globals aside.

`relay_receive` now holds the mutex across the dedup decision, the eviction
sweep and the counter updates, then **releases it before the subscriber
fan-out** — the same shape 2.5.3 gave `pubsub_publish`, so a slow or full
subscriber channel cannot block an unrelated receive.

**Verified by a threaded test, not by inspection.** `test_relay_receive_is_reentrant`
runs two workers with distinct sender ids, 400 messages each, strictly
increasing sequences — so with correct per-call state there is no interaction
and every receive must be accepted. Against the pre-fix code it fails
reproducibly (3/3 runs) on `no message was wrongly dropped as a duplicate`:
the race made real messages look like replays.

Also removed: `_relay_check_dedup`, dead since `relay_receive` inlined its
logic — referenced only by the comment that justified the globals.

### Fixed — `is_broadcast` was computed and thrown away

`relay_receive` derived whether a message was a broadcast and then returned the
bare message, so a consumer could not tell a broadcast from a direct message
without re-inspecting the envelope.

**Added `relay_receive_ex`**, returning an `IncomingMessage` (16 B:
`incoming_is_broadcast`, `incoming_msg`). `relay_receive` is unchanged in
behaviour and return type — it now delegates and drops the flag — so existing
consumers keep working.

### Added — sequence-gap detection

A gap means at least one message from a sender was lost in transit. The message
is still delivered; the gap is what a caller needs to request a resend.

- **`relay_last_seq(r, from)`** — the last sequence seen from a sender, or `-1`
  if unknown. Taken under the lock.
- **`sequence_gaps`** in `relay_stats`, counted when `seq != last + 1`, and also
  when a *first* message from a new sender arrives at `seq != 1`.
- A duplicate is **not** counted as a gap — it increments `duplicates_dropped`,
  as before.

⚠ **`relay_stats` grew from 40 to 48 bytes, with `sequence_gaps` APPENDED at
offset 40.** Offsets 0–32 are unchanged, so a reader built against 2.5.x still
reads the right fields; `test_relay_stats_layout_unchanged` pins each one.

### Documented — bounded dedup opens a replay window

`relay_set_max_dedup` evicts the least-recently-seen sender when the table
overflows, and evicting a sender forgets its last-seen sequence — so that
sender's already-delivered messages become acceptable again and are fanned out a
second time. This is **off by default** (`relay_new` sets `0` = unbounded) and
that default is correct for a strict deliver-once contract; the behaviour is now
stated at the call site rather than left to be discovered.

### Changed — toolchain and dependencies

- `cyrius` **6.5.10 → 6.5.14**. Three-step resolve, 107 files.
- `[deps.sigil]` **3.12.1 → 3.12.6** — five releases of RSA fixes, including the
  PKCS#1 v1.5 and PSS **authentication bypasses** (a forged signature verifying)
  that 3.12.3–3.12.6 closed. majra reaches sigil through `ipc_encrypted` and the
  TLS path, so this is not optional.
- `[deps.sakshi]` stays 2.4.8 — already what the 6.5.14 fold carries
  (hash-verified identical).

### Verification

`test_core` **142 passed / 0 failed** (was 112 — 30 new assertions), plus
`test_backends` 42/42 and `test_patra_queue` 17/17. The **full soak set** was run
because this touches relay dedup: `soak_relay` (dedup correct, eviction
bounded), `soak_queue`, `soak_pubsub`, `soak_heartbeat` — all OK. All four dist
profiles regenerated and verified to carry the new API.

⚠ **Pre-existing and NOT fixed here:** `cyrius distlib`'s self-check reports
`undefined variable 'CLOCK_MONOTONIC'` in the generated bundles
(`src/envelope.cyr:37` uses it; `lib/syscalls.cyr:40` includes the platform file
that defines it). This is **not** a consumer-visible defect — a program that
includes `dist/majra.cyr` builds and links fine, verified — and it reproduces on
2.5.3 with 8 errors, so it predates this release. Worth its own investigation.

## [2.5.3] — 2026-07-28

**Two silent data-loss races and a namespace-isolation bypass in the routing
core.** This is the first release in the 2.5 line to change `src/` logic — all
four dist bundle bodies move. Every fix below was reproduced with a probe
before the change and re-run after: the probes are the acceptance criteria, and
the durable ones are now in-tree as regression assertions. **321/321 CI**
(150 core + **112** expanded, up from 96 + 42 backends + 17 patra_queue) +
3/3 fuzz + 4/4 soak + **36/36 live**.

The trigger was a third-party report that "majra's pubsub subscribe path is
broken", filed with no repro. It was right, for a reason the reporter didn't
identify — and the same root cause was also eating queue jobs.

### Fixed
- **`pubsub_subscribe` could hand a caller a channel that was never registered
  (silent, ~0.1-0.9% under contention).** All three subscribe entry points ran
  `chan_new` + `_sub_new` **before** taking the hub mutex. `chan_new` is safe —
  it uses `alloc`, which carries a CAS spinlock — but `_sub_new` uses
  `fl_alloc`, and `lib/freelist.cyr` pops its size-class free list with a plain
  load/store pair and **no lock at all**. Two concurrent subscribers could
  therefore be handed the *same* 16-byte subscriber block; both wrote it, the
  vec received the same pointer twice, and one caller walked away with a live
  channel that no subscriber entry pointed at. That caller's `chan_recv` blocks
  **forever**, and `pubsub_publish` counted the aliased subscriber twice so the
  delivered count concealed it. Measured: **1-7 orphaned channels per 800**
  concurrent subscribes, 5 of 8 runs affected; **0 across 10 runs** after
  moving the lock above the allocations. Regression test:
  `test_pubsub_concurrent_subscribe`.
- **`mq_enqueue` silently lost jobs under concurrency — the same class, worse
  blast radius.** It performed `queue_item_new` (an `fl_alloc`), `_next_job_key()`,
  and `fl_alloc(72)` all before locking, and `_next_job_key()` increments the
  `_mq_next_job_id` global with a plain read-modify-write. Two enqueues could
  take the **same job key**, and the second `map_set` overwrote the first job
  with no error anywhere. Measured: **788-796 of 800** jobs surviving, 3 of 6
  runs affected; **800/800 across 6 runs** after extending the existing mutex to
  cover key generation and both allocations. Regression test:
  `test_managed_queue_concurrent_enqueue`.
- **A lagging subscriber froze the entire hub, including unrelated topics and
  `pubsub_subscribe` itself.** `pubsub_publish` held the hub mutex across
  `chan_send`, which **blocks** when a subscriber's 64-slot channel is full — so
  the publisher parked inside the critical section. Publishes to other topics
  with empty channels blocked; `pubsub_subscribe` on a brand-new topic blocked.
  Measured behind a single slow-but-well-behaved consumer: publish latency on an
  unrelated, always-empty topic went **4.7us → 10.7ms avg / 213ms worst**, versus
  the ~1us/op in `docs/architecture/overview.md`. Publish now snapshots each
  subscriber list as a `(data, len)` pair under one short lock and walks it
  unlocked — safe because `vec_push` grows into a *new* bump-allocated buffer
  and never frees the old one, and pushes only append. `hashed_channel_send` had
  the identical shape and got the same treatment. Regression test:
  `test_pubsub_no_head_of_line_block` (it hangs rather than fails if this
  regresses — the honest signal for a liveness bug).
- **`pubsub_publish` over-reported `delivered` on closed channels.**
  `chan_send` returns −1 for a closed channel; the count incremented anyway.
  Now only a `chan_send` returning 0 counts.

### Changed — pattern matching (behavior change, read this)
- **`#` and `+` are now honored only when they occupy a whole level.**
  Previously both were matched anywhere in a token, so a pattern could escape
  its namespace prefix: **`"tenant-a#"` matched `"tenant-a-evil/secret"`** and
  `"sensors/temp+"` matched `"sensors/temperature"`. `namespace_wildcard(ns)`
  builds exactly `"<prefix>/#"`, so a single dropped `/` turned tenant isolation
  into a prefix scan over other tenants. `#` must additionally be the final
  character (MQTT-3.1.1 §4.7.1.2); anywhere else it is now a literal byte.
- **`"<prefix>/#"` now also matches the bare topic `"<prefix>"`**, per
  MQTT-3.1.1 §4.7.1.2 ("`sport/#` also matches the singular `sport`, since `#`
  includes the parent level"). Previously it did not.
- These two items **change matching semantics**, which `docs/development/semver.md`
  treats as off-limits for a PATCH. They ship in a patch anyway because the old
  behavior is a defect against the module's own documented "MQTT-style" contract
  and the first item is a security bug — but a consumer that (deliberately or
  accidentally) relied on mid-token wildcards will see subscriptions stop
  matching. 8 new assertions in `test_pubsub_pattern_matching_extended` pin the
  new semantics.

### Performance
- **Net neutral after two optimization passes; no regression >10%.** 17 targets
  × 7 trials, pre-fix vs post-fix, compared on min *and* median. The naive
  correct version cost **+110% on `pattern_exact`** and **+40% on
  `pubsub_publish_nosub`** — the former from computing wildcard alignment on
  every character, the latter from a `map_get` (string hash) per loop iteration.
  Both were restructured: alignment is checked inside each wildcard branch so a
  literal byte costs what it always did, and publish snapshots once instead of
  re-resolving the map per subscriber. Final: `pattern_exact` **−2.4%**,
  `pubsub_publish_nosub` **+0.3%**, `pubsub_1sub_publish` **+0.3%**. The one
  residual is `pattern_wildcard_#` at **+4ns** (+8.9% min / +11.1% median) —
  that is the level-alignment check itself, and it is the price of the
  isolation fix.

### Distribution
- All four bundle **bodies** change (first `src/` logic change in the 2.5 line):
  `majra.cyr` 3,187 → **3,284** lines, `majra-signed.cyr` 3,333 → **3,430**,
  `majra-admin.cyr` 3,320 → **3,417**, `majra-backends.cyr` 4,788 → **4,885**.
  The `signed` and `admin` `.deps` sidecars gain `alloc`, which those profiles
  now reference directly.

## [2.5.2] — 2026-07-28

**Toolchain pin `6.4.62` → `6.4.83` + sigil `3.11.1` → `3.12.1` (latest) +
sakshi pinned forward `2.4.3` → `2.4.6`, and a `cyrius bench` repair.** No
majra source-logic change: the four dist bundle **bodies stay byte-identical**
(the whole `git diff dist/` is four banner lines, 2.5.1 → 2.5.2; the `.deps`
sidecars regenerate identically). Full matrix re-ran clean under the new pin —
**305/305 CI** (150 core + 96 expanded + 42 backends + 17 patra_queue) +
**3/3 fuzz** + **4/4 soak** + **17/17 bench** targets.

The 6.4.62 → 6.4.83 span leaves majra's surface alone: the `lib sync --full`
snapshot holds at **99 files**, `cyrius.lock` holds at **99 hashes + 1
commit-pin**, and `cyrius lint` output is byte-identical across all 23 `src/`
files under the two toolchains (both linters run against the same tree, 104
lines of output, zero diff). One toolchain change is visible in-tree, via the
stdlib snapshot's bundled sakshi — see below.

### Changed
- Toolchain pin `6.4.62` → `6.4.83`; `[deps.sigil]` tag `3.11.1` → `3.12.1`.
  sigil's `dist/sigil.cyr` grows 25,391 → **26,254 lines**. majra's sigil
  footprint is unchanged at six symbols — `ed25519_{init,sign,verify}`
  (`src/signed_envelope.cyr`) + `aes_gcm_{global_init,encrypt,decrypt}`
  (`src/ipc_encrypted.cyr`); the constant-time pk compare is still stdlib
  `ct_eq_bytes_lens`. `core` and `admin` pull no sigil.
- **The two pins are an atomic move: the crypto profiles now floor at cyrius
  ≥ 6.4.64.** sigil 3.12.1 stopped hardcoding its crypto-bank thread-local slot
  (`_SIGIL_CBANK_SLOT = 8` at 3.11.1) and now allocates it dynamically —
  `_SIGIL_CBANK_SLOT = -1` plus a CAS-gated `thread_local_alloc()`. That symbol
  does not exist in the 6.4.62 stdlib snapshot (`TLOCAL_MAX_SLOTS = 16`, no
  allocator); it first appears in **6.4.64** (`TLOCAL_MAX_SLOTS = 128`).
  Verified by building the same consumer against both sigil bundles on both
  snapshots. So **sigil 3.12.1 cannot be paired with cyrius 6.4.62** — the build
  fails with `refusing to emit binary with N reachable undefined function(s)`.
  sigil's own comment says "requires cyrius >= 6.4.65"; 6.4.64 is where the
  symbol lands, so 6.4.65 is conservative-safe. majra's builds were never at
  risk (`tests/test_backends.tcyr` pulls `lib/tls.cyr` → `lib/thread_local.cyr`
  ahead of sigil), but **downstream consumers of `majra-signed` /
  `majra-backends` are** — see the README note below.
- **README's consumer include contract was incomplete, and is now verified
  rather than asserted.** The dist bundles are pure `src/` concatenation with
  **zero `include "lib/…"` lines**, so the consumer supplies every stdlib module
  both majra *and sigil* reach into — but the README only ever named `lib/ct.cyr`
  and (for admin/backends) `lib/sandhi.cyr`. Building a clean consumer against
  each shipped bundle shows `signed` additionally needs `thread_local`, `io`,
  `fs`, `chrono`, `bayan`, `keccak`, `random`; `admin` needs `net`, `io`,
  `chrono`, `async`, `dynlib`, `fdlopen`, `sakshi`, `random` + `tls` before
  `sandhi`; `backends` needs the union. README now carries the per-profile table.
  **`lib/thread_local.cyr` was already required at sigil 3.11.1** (for
  `thread_local_{init,get,set}`) — that gap is pre-existing, not new here.
- **New explicit `[deps.sakshi]` block, pinned to the published latest
  (`2.4.6`).** sakshi reaches majra's compilation unit only transitively (patra
  for the durable queue, sigil for its logging floor) — but sigil's own manifest
  declares `[deps.sakshi] tag = "2.4.3"`, and `cyrius deps` overlays that
  resolution *on top of* the `lib sync --full` snapshot, which ships 2.4.6 under
  the 6.4.83 pin. Left implicit, every build **downgraded** `lib/sakshi.cyr` and
  printed `./lib/ shadows version-pinned … sakshi 2.4.3 (pinned: 2.4.6)`.
  Declaring it at the top level pins the resolution forward and the warning is
  gone. The span is backward-compatible for majra: 2.4.4 is purely additive
  (128-bit W3C trace-id), 2.4.5 fixes the agnos `_sk_open` `O_RDWR`→`AO_WRONLY`
  fold (a real read-path bug on the agnos target), 2.4.6 is a pin catch-up.
  `cyrius.lock`'s commit-pin moves `2.4.3` → `2.4.6`; the hash count is unchanged.

### Fixed
- **`cyrius bench` / `cyrius audit` could not compile `benches/bench_all.bcyr`.**
  Both resolve the manifest `[deps].stdlib` list into the compilation unit —
  which includes `tls` and `sandhi` — while the bench entry point included
  neither `fdlopen` (called by `lib/tls.cyr`) nor `async` (called by sandhi). The
  driver stopped at `error: refusing to emit binary with 4 reachable undefined
  function(s)`, so `cyrius bench` reported `0 passed, 1 failed` without running a
  single benchmark. The entry point now carries the same explicit toolchain
  includes the test entry points already had (`async`, `dynlib`, `fdlopen` — see
  `tests/test_backends.tcyr`). **This was pre-existing, not a 6.4.83
  regression** — reproduced identically at the 2.5.1 state with
  `CYRIUS_HOME` pinned to 6.4.62. CI was unaffected because it builds benches
  via `cyrius build --no-deps benches/*.bcyr`, which never injects the manifest
  stdlib set; the breakage was confined to the `bench`/`audit` convenience path
  the release process actually leans on.

### Docs
- **[`cyrius-quirks.md`](docs/development/cyrius-quirks.md) §6 rewritten — it was
  describing 6.1.x behavior that no longer holds.** The entry claimed an
  undefined symbol is *always* a warning + runtime `ud2`. The toolchain now
  splits on reachability: a **reachable** call site is a hard `error: refusing
  to emit binary with N reachable undefined function(s)` and **no binary is
  written**, while an **unreachable** one still warns and emits the `ud2`.
  Verified empirically at both 6.4.62 and 6.4.83 — this is a stale-doc fix, not
  a 6.4.83 change. The entry also now records that reachability is computed over
  whatever the *driver* injects, so a green CI (`--no-deps`) does not imply
  `cyrius audit` compiles — the trap this release's bench fix walked into.
- §7 gained the snapshot-shadowing rule (a `lib sync --full` copy can be
  silently downgraded by a `cyrius deps` overlay carrying an inherited tag) and
  its stale `cyrius lib sync` invocations were corrected to `--full`; same
  correction in [`testing.md`](docs/guides/testing.md), which still had the
  pre-6.4.x form. §5's "latest sigil" reference caught up 3.7.10 → 3.12.1.
- [`threat-model.md`](docs/development/threat-model.md) had two pre-existing
  stale claims: the Crypto-trust-boundary line still cited the **sigil 3.7.8**
  pin (four bumps behind) and Supply Chain still said "one first-party dep".
  Both corrected; the latter now also notes that a transitive dep whose version
  is inherited from another manifest is not a version majra controls.
- `state.md` / `dependency-watch.md` / `roadmap.md` / `doc-health.md` /
  `README.md` / `CLAUDE.md` refreshed for the new pins.

### Verified
- 150 core + 96 expanded + 42 backends + 17 patra_queue = **305/305, 0 failed**.
- 3/3 fuzz harnesses (500 iters × 10s timeout), 4/4 soak suites.
- **Live integration: 36/36, 0 failed** — 7 Redis + 4 PostgreSQL categories
  against `redis:7-alpine` + `postgres:16-alpine`. (`state.md` had recorded 32
  for this suite; `docs/guides/testing.md` had the correct 36. Corrected.)
- Clean-consumer builds against all four shipped bundles (`core`, `signed`,
  `admin`, `backends`) — each compiles and runs from an entry point that has
  only the documented include set, which is how the README table above was
  derived rather than asserted.
- `cyrius vet src/main.cyr` → 27 deps, 0 untrusted, 0 missing;
  `cyrius deny src/main.cyr` → 27 deps, 0 violations; `cyrius fmt --check`
  clean across `src/` + `tests/` + `fuzz/` + `benches/`.
- Cold rebuild from scratch (`rm -rf build lib && lib sync --full && deps &&
  build --no-deps`) passes, and `cyrius deps --verify` reports **99 verified,
  0 failed** against the committed lockfile. The sakshi commit-pin
  (`bfc127f8…`) matches GitHub's `2.4.6` tag object, and the resolved
  `lib/sigil.cyr` hashes identical to `dist/sigil.cyr` at sigil's `3.12.1` tag
  — so the local `path = "../sigil"` resolution and CI's git resolution agree.
- **Benchmarks: no regression.** 17 targets × 7 trials on each pin (6.4.62
  baseline vs 6.4.83), compared on both min and median: every delta lands
  within **±3.4%**, none over the 10% flag threshold. The eye-catching
  single-run swings (`pattern_wildcard_+` "+24%", `barrier_cycle` "+13%") were
  run-to-run noise — the same 6.4.83 binary produced both 85 ns and 118 ns for
  `pattern_exact` — which is why the comparison is distribution-based.

## [2.5.1] — 2026-07-13

**Toolchain pin `6.3.15` → `6.4.62` + sigil `3.9.8` → `3.11.1` (latest) + a
sigil-footprint review.** No majra source-logic change: the four dist bundle
**bodies stay byte-identical** (only the version banner moves 2.5.0 → 2.5.1).
Full matrix re-ran clean under the new pin — **305/305 CI** (150 core + 96
expanded + 42 backends + 17 patra_queue) + **3/3 fuzz** + **4/4 soak**.

The cyrius 6.3.15 → 6.4.62 span is almost entirely agnos syscall wrappers,
the async-runtime target split, and DX diagnostics — nothing touching majra's
surface. Two toolchain changes are visible in-tree:
- **`cyrius lib sync` default is now the declared `[deps].stdlib` *subset*
  (40 files); the whole snapshot is `--full` (99 files).** CI + release
  already invoke `cyrius lib sync --full`; the CLAUDE.md quick-start was
  corrected to match. `cyrius.lock` carries **99** resolved-file hashes.
- **Per-profile `distlib` `.deps` sidecars re-subsetted** (cyrius 6.4.48 fix +
  the 6.3.32 built-in `std` group). The three profile sidecars drop `fmt`/
  `syscalls` (now implied by the always-resolved `std` group) and `assert`
  (test-only, unreferenced by the bundles); `signed`/`admin` additionally drop
  `alloc`, while `backends` keeps `alloc` (referenced directly by its extra
  modules). The `.cyr` bodies are unchanged; regeneration is idempotent, so
  the CI freshness gate stays green.

### Sigil-footprint review (`[lib.<type>]` per-primitive profiles)
sigil 3.11.0 added twelve per-primitive `[lib.<type>]` distlib profiles
("pull only the crypto you need"). majra's **entire** sigil surface is six
symbols — `ed25519_{init,sign,verify}` (`src/signed_envelope.cyr`) and
`aes_gcm_{global_init,encrypt,decrypt}` (`src/ipc_encrypted.cyr`); the
constant-time pk compare is stdlib `ct_eq_bytes_lens`, **not** sigil. The
`core` and `admin` profiles correctly pull **no** sigil.
- **majra keeps the full `dist/sigil.cyr`.** Its only local sigil consumer
  (`tests/test_backends.tcyr`) exercises *both* primitives, and the two narrow
  closures `sigil-ed25519.cyr` + `sigil-aes.cyr` (~2k lines each) **overlap on
  121 functions** (Ed25519 uses SHA-512 internally; both share sigil's u256
  field arithmetic + crypto-scratch + random floor). Combined they emit 121
  "last-definition-wins" duplicate-fn warnings — noisier and more fragile than
  the full bundle's single *deduplicated* closure (which resolves clean). sigil
  publishes no `dist/sigil/index.cyml`, so the clean `modular = [...]` dedup
  path is unavailable. The per-primitive win is real only for a **single**-
  primitive consumer — a `signed`-only downstream (e.g. secureyeoman) should
  pull `dist/sigil-ed25519.cyr` (~2k lines) instead of the full 25,391-line
  bundle. Recorded in [`dependency-watch.md`](docs/development/dependency-watch.md).
- The bump also banks sigil **3.9.9**'s crypto-bank thread-local-slot fix
  (`_SIGIL_CBANK_SLOT` moved 0 → 8): slot 0 collided with **patra**'s SQL
  scratch slot, corrupting state in a process that links *both* — exactly the
  `backends` profile (sigil crypto + `patra_queue`).

### Fixed
- **Undersized `var X[N]` buffers in the test/soak harnesses (same class the
  2.5.0 audit fixed in `src/`, missed in the non-CI files).** A function-local
  `var X[N]` is **N bytes**, so a 16-byte `struct timespec` written into
  `var ts[2]` (2 bytes) overflows. Since cyrius 6.3.13 moved `var X[N]` locals
  onto the guarded thread stack, `tests/soak/soak_heartbeat.cyr`'s phase-B
  offline-eviction sleep was silently corrupting its own node count →
  **`FAIL B: nodes prematurely evicted after cycle 1`** (the soak set is not in
  the default CI pass, so it went uncaught at 2.5.0 despite already being live
  at 6.3.15). Every over-written buffer is now sized to the bytes it holds:
  `soak_heartbeat.cyr` + `test_core.tcyr` `ts[2]→ts[16]`; `test_backends.tcyr`
  `key[4]→key[32]` (×2), `nonce[2]→nonce[12]`, `buf[2]→buf[4]`. All four soak
  suites now pass (was 3/4). An adversarial review pass empirically pinned the
  underlying rule and corrected [`cyrius-quirks.md`](docs/development/cyrius-quirks.md) §4:
  a **function-local** `var buf[N]` is **N bytes**, but a **module-level/global**
  `var buf[N]` is **N × 8 bytes** (N `i64` slots) — so the shipped globals
  (`redis_backend.cyr` `_resp_buf[512]` = 4096 B, `error.cyr` `_err_msg_buf[64]`
  = 512 B) are correctly sized and were *not* touched.

### Changed
- Toolchain pin `6.3.15` → `6.4.62`; `[deps.sigil]` tag `3.9.8` → `3.11.1`.

## [2.5.0] — 2026-06-30

**agnos target support for the core pub/sub engine** (base-stack agnos-readiness
migration, tier 1 — the shared blocker under bote/t-ron/hoosh). majra had **zero**
`CYRIUS_TARGET_AGNOS` guards, so its core engine failed to compile on `--agnos`
(`undefined variable SYS_FUTEX`). The core (`dist/majra.cyr`) is now agnos-clean:

- **`barrier.cyr` / `queue.cyr`** — the `futex(FUTEX_WAIT/WAKE)` fast-path (no such
  syscall on agnos, and `SYS_FUTEX`/`FUTEX_*` are Linux-only stdlib constants) is
  now `#ifndef CYRIUS_TARGET_AGNOS`-guarded. On agnos the wait becomes a
  `sys_sched_yield()` spin-yield (cooperative scheduler) and the wake is a no-op —
  correct producer/consumer + barrier semantics on the single-core model.
- **`envelope.cyr`** — `time_now_ns` uses `sys_uptime_ms()` (#40, monotonic) on
  agnos instead of `clock_gettime` (#228, out of the frozen 0-63 range); UUID
  generation uses `sys_getrandom` (#45) instead of the Linux `getrandom` (#318).
- **`dag.cyr`** — retry backoff uses `sys_sleep_ms()` (#41) on agnos instead of
  `nanosleep` (#35, = `sysinfo` on agnos → mis-dispatch).
- **`ipc.cyr`** — AF_UNIX domain-socket transport (in the core bundle) fail-closes
  on agnos (`ipc_bind`/`ipc_accept`/`ipc_connect` → `Err(ERR_IPC)`), keeping the raw
  Linux socket numbers (41/42/43/49/50) off the agnos target.

Toolchain pin `6.2.11` → `6.3.15`. Host build byte-identical (all changes are
additive `#ifdef` branches). Core dist verified agnos-clean (`SYS_FUTEX`
references are all `#ifndef`-guarded; no raw `228`/`318`/`35`).

**Known residual (non-core):** `patra_queue.cyr` — the optional persistent-queue
backend — pulls `patra`, whose `lib/patra.cyr` still references `SYS_LSEEK`
unguarded on agnos. `patra_queue` is **excluded from the default `dist/majra.cyr`**
(core) profile, so no consumer that pulls the core engine (bote/t-ron/hoosh) is
affected; it only blocks a full `--agnos` build of the majra daemon + the
`backends` profile. Tracked for the patra migration.

### Fixed
- **Undersized array-local buffer overflows (host crash under cyrius ≥ 6.3.13).**
  Several `var X[N]` locals were too small for the bytes written into them:
  `var ts[2]` (2 bytes) for a 16-byte `struct timespec` (`envelope.cyr`
  `time_now_ns`, `dag.cyr` backoff, `main.cyr`), `var buf[2]` for 16 random bytes
  (`envelope.cyr` `uuid_generate`), and 1-byte frame headers written 2–4 bytes
  (`ipc.cyr`, `ws.cyr`, `postgres_backend.cyr`). A function-local `var X[N]` is
  **N bytes**, so these overflowed. It was *latent* before cyrius **6.3.13**, when
  local arrays lived in a shared global/BSS buffer (the overflow scribbled adjacent
  globals harmlessly); 6.3.13 moved `var X[N]` locals onto the **thread stack**
  (with a `PROT_NONE` guard page), so the same overflow now smashes the stack →
  `SIGSEGV`. Surfaced as a `test_core` segfault in `test_relay_skip_routing` (it
  calls `time_now_ns`) the moment the pin moved to 6.3.15. Every buffer is now
  sized to the bytes actually written. `test_core` **96/96**, `test_patra_queue`
  **17/17** green.

### Added
- agnos (`CYRIUS_TARGET_AGNOS`) support for the core engine (barrier/queue/
  envelope/dag/ipc): futex→sched_yield, clock→uptime_ms, getrandom→#45,
  nanosleep→sleep_ms, AF_UNIX IPC fail-closes.

### Changed
- Toolchain pin `6.2.11` → `6.3.15`.

## [2.4.7] — 2026-06-15

Cyrius toolchain minor bump **6.1.35 → 6.2.11** (first move onto the
6.2.x line) plus a routine dependency bump **sigil 3.7.10 → 3.7.14**
(latest). No majra source-logic change; the four distribution bundle
bodies are byte-identical to 2.4.6 (only the version banner moves). The
6.2.x stdlib snapshot grew the lib-sync set **88 → 97 files**, and
sigil 3.7.14 rolls transitive **agnosys 1.3.2 → 1.4.3**. All 305 CI
assertions + 3 fuzz harnesses + 4 soak suites pass under the new
toolchain.

### Changed

- **Cyrius toolchain pin 6.1.35 → 6.2.11** (`cyrius.cyml [package].cyrius`).
  First step onto the 6.2.x line; stdlib / codegen fixes pulled in via
  `cyrius lib sync` + `cyrius deps`. The lib-sync snapshot is now 97
  `.cyr` files (was 88 under 6.1.35).
- **sigil 3.7.10 → 3.7.14** (`[deps.sigil]`). Routine patch bump tracking
  latest. Transitive **agnosys 1.3.2 → 1.4.3**. The four bundle bodies
  stay byte-identical — sigil's `signed`/`backends` surface is unchanged
  across 3.7.10 → 3.7.14.
- **`cyrius.lock`** now carries SHA-256 over **97** resolved files (was
  88), reflecting the larger 6.2.x stdlib snapshot. CI's
  `cyrius deps --verify` enforces the match.

## [2.4.6] — 2026-06-11

Cyrius toolchain refresh within the 6.1.x line. Pin **6.1.24 → 6.1.35**,
and a routine dependency bump **sigil 3.7.8 → 3.7.10** (latest). No
majra source-logic change; the four distribution bundle bodies are
byte-identical to 2.4.5 (only the version banner moves). The one
mechanical adjustment the toolchain forced: `bigint` was dropped from
the cyrius 6.1.35 stdlib snapshot (94 → 88 files), and majra never
called it — sigil 3.x bundles its own `u256_*` field arithmetic — so
its lone stale `include` and the `[deps] stdlib` hint entry were
removed. All 305 CI assertions + 3 fuzz harnesses + 4 soak suites pass
under the new toolchain.

### Changed

- **Cyrius toolchain pin 6.1.24 → 6.1.35** (`cyrius.cyml [package].cyrius`).
  Eleven patch-level cyrius releases of stdlib / codegen fixes pulled in
  via `cyrius lib sync` + `cyrius deps`.
- **sigil 3.7.8 → 3.7.10** (`[deps.sigil]`). Routine patch bump now that
  sigil tracks latest under the cyrius 6.x toolchain (the 2.9.0 asm-NI
  pin was retired at 2.4.5). Transitive **agnosys holds at 1.3.2**. The
  four bundle bodies stay byte-identical — sigil's `signed`/`backends`
  surface is unchanged across 3.7.8 → 3.7.10.
- **`bigint` removed from the stdlib surface.** The cyrius 6.1.35 stdlib
  snapshot dropped `lib/bigint.cyr` (snapshot 94 → 88 files). majra had
  no `big_*` call sites — `tests/test_backends.tcyr` carried a stale
  `include "lib/bigint.cyr"` (a leftover from before sigil 3.x bundled
  its own `u256_*` ops) and `cyrius.cyml [deps] stdlib` listed `bigint`
  as a hint. Both removed; `cyrius deps` no longer errors on the missing
  module. `cyrius.lock` now carries **88** hashes (was 94).

### Verified

- Core (main.cyr smoke): **150/150**.
- `tests/test_core.tcyr`: **96/96**.
- `tests/test_backends.tcyr`: **42/42** — `aes_gcm_roundtrip`,
  `encrypted_ipc`, `signed_envelope`, `admin` all green on the sigil
  3.7.10 surface under cyrius 6.1.35.
- `tests/test_patra_queue.tcyr`: **17/17**.
- Fuzz (heartbeat/pubsub/queue): clean. Soak (queue/pubsub/relay/
  heartbeat): clean. All four dist bundles regenerated at v2.4.6
  (bodies byte-identical to 2.4.5; only the version banner moved).

## [2.4.5] — 2026-06-10

Cyrius 6.x migration. Toolchain pin **5.10.44 → 6.1.24**, and with the
6.x compiler the long-standing sigil crypto-NI blocker finally clears:
sigil moves **2.9.0 → 3.7.8** (latest), the first sigil bump since the
2.4.0 line. No majra API, ABI, or wire-format drift; the four
distribution profiles keep their public surface. All 305 CI assertions
+ 3 fuzz harnesses + 4 soak suites pass under the new toolchain.

### Changed

- **Cyrius toolchain pin 5.10.44 → 6.1.24** (`cyrius.cyml [package].cyrius`).
- **sigil 2.9.0 → 3.7.8** (`[deps.sigil]`). The 2.9.0 pin existed solely
  to dodge the AES-NI / Ed25519-NI `[rbp-N]` asm-offset SIGILL on cyrius
  5.10.x. Under cyrius 6.x that whole failure class is gone — sigil's NI
  asm migrated to the `param_load` pseudo (cyrius 6.0.67+), so the latest
  release rides the toolchain cleanly. Transitively rolls **agnosys 1.0.4
  → 1.3.2** (zero `SYS_OPEN` refs — the dormant aarch64 cross-build
  blocker is resolved as a side effect).
- **Build workflow: `cyrius lib sync` now precedes `cyrius deps`, and
  builds pass `--no-deps`.** Cyrius 6.x split stdlib provisioning
  (`cyrius lib sync` copies the version-pinned 94-file snapshot into
  `./lib/`) from git-dep resolution (`cyrius deps`). A bare `cyrius deps`
  leaves a partial `./lib/` that omits the toolchain modules
  agnosys/sandhi reach into (`slice`, `tls`), and cyrius 6.1.x compiles
  an unresolved call to a runtime-trapping `ud2` rather than failing the
  build — so the omission surfaces as a SIGILL, not a link error.
  `cyrius.lock` now carries 94 hashes (was 3). CI + release workflows
  updated.

### Migrated

- **`src/admin.cyr` → sandhi server API.** The HTTP server surface was
  renamed `http_*` → `sandhi_server_*` in the cyrius 6.x stdlib reorg
  (`http_send_status` → `sandhi_server_send_status`, `http_server_run` →
  `sandhi_server_run`, etc. — same signatures). The `HTTP_*` status
  constants are unchanged. The admin/backends bundles carry the new
  calls; consumers of those profiles must include `lib/sandhi.cyr`.
- **`src/signed_envelope.cyr`: `ct_eq` → `ct_eq_bytes_lens`.** sigil
  retired its bundled `ct_eq` at 3.0.2; the constant-time dual-length
  compare now comes from the stdlib `lib/ct.cyr`. signed/backends
  consumers must include `lib/ct.cyr`.
- **Test/fuzz entry-point include surface widened** for the cyrius 6.x
  stdlib split: `tests/test_backends.tcyr` adds ct/chrono/async/sakshi/
  dynlib/fdlopen/tls; `tests/test_patra_queue.tcyr` and
  `fuzz/fuzz_queue.fcyr` add the `thread` (mutex moved off `sync.cyr`'s
  twin) / `src/metrics.cyr` includes they were transitively relying on.

### Verified

- Core (main.cyr smoke): **150/150**.
- `tests/test_core.tcyr`: **96/96**.
- `tests/test_backends.tcyr`: **42/42** — `aes_gcm_roundtrip`,
  `encrypted_ipc`, `signed_envelope`, and `admin` all green on the
  sigil 3.7.8 surface under cyrius 6.1.24 (these are exactly the paths
  that ud2-SIGILL'd before the `ct_eq` / lib-sync fixes).
- `tests/test_patra_queue.tcyr`: **17/17**.
- Fuzz (heartbeat/pubsub/queue): clean. Soak (queue/pubsub/relay/
  heartbeat): clean. All four dist bundles regenerated at v2.4.5.

## [2.4.4] — 2026-05-11

Cyrius toolchain refresh. No source change; no API, ABI, or
wire-format drift. All 305 CI assertions + 3 fuzz harnesses + 4
soak suites pass under the new pin. Sigil stays held at 2.9.0 —
upstream P1 ([sigil asm stack-frame drift](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md))
is still open at sigil 3.1.1 (the 5/11 sigil patch was the
stdlib annotation pass, not the NI-path fix).

### Changed

- **Cyrius toolchain pin bumped 5.10.34 → 5.10.44** (`cyrius.cyml [package].cyrius`).
  Ten patch-level cyrius releases worth of stdlib / codegen
  bugfixes pulled in via `cyrius deps`. `cyrius.lock` unchanged
  — sigil/sakshi/agnosys all resolve to the same git tags
  (2.9.0 / 2.0.0 / 1.0.0).
- **Dist bundles regenerated** at v2.4.4. Bundle bodies are
  byte-identical to 2.4.3; only the version banner line moved.
  Sizes unchanged: `dist/majra.cyr` 3127 lines / 85 KB,
  `dist/majra-signed.cyr` 3273 lines / 90 KB,
  `dist/majra-admin.cyr` 3259 lines / 90 KB,
  `dist/majra-backends.cyr` 4727 lines / 137 KB.

### Verified

- Core (main.cyr smoke): **150/150**.
- `tests/test_core.tcyr`: **96/96**.
- `tests/test_backends.tcyr`: **42/42** — including
  `signed_envelope`, `aes_gcm_roundtrip`, and `encrypted_ipc`,
  which sit directly on the sigil 2.9.0 surface and would have
  SIGILL'd at the first asm dispatch had the toolchain bump
  perturbed the reference paths.
- `tests/test_patra_queue.tcyr`: **17/17**.
- Fuzz (`cyrius fuzz`): **3/3** harnesses pass (heartbeat / pubsub / queue).
- Soak: **4/4** (queue 5k ops, pubsub 2000 topics, relay dedup +
  eviction, heartbeat 100 nodes × 20 cycles + auto-eviction).
- `cyrius lint src/main.cyr`: 0 warnings.
- `cyrius vet src/main.cyr`: 27 deps, 0 untrusted, 0 missing.
- `cyrius fmt src/main.cyr --check`: clean.

## [2.4.3] — 2026-05-10

`patra_queue` retire-the-workarounds patch. No API or wire-format
change; all 305 assertions still pass. Cleans up the only meaningful
piece of tech debt the 2.4.2 toolchain bump exposed.

### Changed

- **`src/patra_queue.cyr` now uses server-side SQL** for its three
  hot paths. patra (resolved via the cyrius stdlib at v1.9.3 now,
  not the 1.1.1 the workarounds were written against) supports
  `WHERE`, `ORDER BY`, `LIMIT`, and the `COUNT`/`MAX` aggregates:
  - `_pq_load_next_id`: `SELECT MAX(id) FROM jobs` — single-row
    aggregate, no full-table scan to find the largest id at open.
  - `patra_queue_dequeue`: `SELECT * FROM jobs WHERE status = 0
    ORDER BY priority ASC, id ASC LIMIT 1` — the dequeue ordering
    (lower priority number = higher priority, ties broken by id)
    is now server-side. Drops the ~30-line client-side scan + sort.
  - `_pq_count_where_status`: `SELECT COUNT(*) FROM jobs WHERE
    status = N` — single-int aggregate result instead of
    fetching every row to bump a counter.
  Behaviour preserved against `tests/test_patra_queue.tcyr`
  (17/17). Useful at queue sizes where the prior O(n) scans were
  starting to matter; correct at any size.
- **`tests/test_patra_queue.tcyr`** ported from raw `syscall(SYS_UNLINK, ...)`
  to the `sys_unlink()` helper — same arch-portability cleanup as
  `src/ipc.cyr` got in 2.4.2.

## [2.4.2] — 2026-05-10

Toolchain + dep refresh. No source API changes; no consumer-visible
behaviour drift. Brings majra onto the same cyrius/sigil floor as
the rest of the first-party tree (agnosys 1.2.4, agnostik 1.2.1,
libro 3.0.1-track).

### Changed

- **Cyrius toolchain pin bumped 5.4.17 → 5.10.34.** Matches the
  current first-party floor (agnosys/agnostik). Notable upstream
  changes spanning this range: arch-peer include resolution now
  expects `~/.cyrius/versions/<V>/lib` (5.10.9+) — CI installer
  updated accordingly; richer fmt/lint/vet/capacity surfaces;
  DCE (`CYRIUS_DCE=1`) available for release binaries; raised
  fixup cap; stdlib `ct_eq_bytes` family (the prerequisite for
  sigil's 3.0.2 `src/ct.cyr` retirement).
- **Sigil dep held at 2.9.0.** Investigated 2.9.5 and 3.1.0 — both
  fail under cyrius 5.10.34 with SIGILL inside different inline-asm
  hot paths (2.9.5: ed25519; 3.1.0: aes-gcm). The asm blocks in
  sigil 2.9.5+ hardcode `[rbp-N]` parameter offsets that match
  cyrius's pre-5.5 stack-frame layout but drift under 5.10.x's
  expanded prologue. 2.9.0 keeps the software AES + reference
  ed25519 paths (no architecture-specific asm dispatch), so it
  rides through the toolchain bump unchanged. Re-evaluate once
  sigil ships an AES-NI/ed25519 path that emits cyrius-stable
  asm or migrates off raw byte arrays. Filing the offset-drift
  upstream as an issue.
- **`lib/` is no longer committed.** Added `/lib/` to `.gitignore`;
  the directory is repopulated by `cyrius deps` from the
  version-pinned manifest. Matches agnosys / agnostik / yukti /
  patra convention. Prevents stale stubs from prior cyrius
  versions sitting in tree.
- **HTTP server surface moved from vendored copy to stdlib `sandhi`.**
  The old `lib/http_server.cyr` (committed in-tree during the M1
  fold-out window) is gone. `src/admin.cyr` and
  `tests/test_backends.tcyr` now pull `HTTP_BAD_REQUEST` /
  `http_send_status` / `http_server_run` from `lib/sandhi.cyr`,
  which is the cyrius stdlib bundle of sandhi 1.3.3 (folded into
  the stdlib at the M6 milestone). `tls` added to `[deps] stdlib`
  because sandhi references `TLS_EARLY_DATA_ACCEPTED` at parse
  time — without it, cyrius's deps-aware build can't validate
  the dep graph.
- **`src/ipc.cyr` ported to `sys_unlink()`** (was raw
  `syscall(SYS_UNLINK, ...)`). The portable helper picks the right
  syscall per target arch; raw `SYS_UNLINK` is x86_64-only and
  blocks cross-builds. Code-hygiene change — keep using the helpers
  on either side of the syscall boundary so a future aarch64 build
  isn't blocked by majra's own code.
- **aarch64 cross-build is NOT wired into CI.** Tried it; blocked
  downstream-of-the-sigil-pin: with `[deps.sigil] = "2.9.0"` we get
  agnosys 1.0.4 transitively, and that agnosys version's
  `lib/agnosys.cyr:791` uses raw `syscall(SYS_OPEN, ...)` (x86_64-only;
  aarch64 Linux uses `SYS_OPENAT`). The 5.10.34 cc5_aarch64 errors on
  the undefined symbol even with `CYRIUS_DCE=1`. **Note: agnosys
  mainline (1.2.4) has zero `SYS_OPEN` refs** — the bug was fixed
  upstream long ago. We just can't pick up the fixed agnosys without
  bumping past sigil 2.9.0, which is gated on the asm-stack-frame
  drift issue (see roadmap "Waiting on upstream"). When the sigil
  P1 lands, agnosys rolls forward transitively and the aarch64 build
  unblocks. All majra consumers run x86_64 server-side; no blocker
  for shipping 2.4.2 without an aarch64 artifact.
- **CI installer fetches the source archive at the version tag** for
  `lib/` (the stdlib snapshot). 5.10.x release tarballs ship `bin/`
  + `deps/` only — no `lib/`. The official `install.sh` covers this
  via a source bootstrap (`git clone` + self-host build), but CI
  doesn't need the bootstrap path; fetching the tagged source archive
  and copying `lib/` from it is the minimal-cost equivalent.
- **CI / release modernized.** Adopted the agnosys/agnostik pattern:
  versioned `~/.cyrius/versions/<V>/lib` toolchain layout (required
  by 5.10.9+ for arch-peer include resolution), `cyrius deps` step,
  `cyrius.lock` hash verification (best-effort until the lockfile
  lands in-tree), aarch64 cross-build (best-effort when
  `cc5_aarch64` ships), all four `cyrius distlib` profiles in the
  freshness gate, fmt-by-diff (drift detection works around the
  `--check` no-op in cyrius 5.9+).
- **CLAUDE.md** — cyrius pin reference + sigil tag refreshed; quirks
  list trimmed for the cc5 5.10.x floor; lib/ now described as
  resolved-by-`cyrius deps` rather than vendored-in-tree.

## [2.4.1] — 2026-04-20

Docs + soak-test cleanup cycle. No API changes; no new deps.

### Added
- **Three new soak targets** — `soak_pubsub` (2000-topic dispatch), `soak_relay` (dedup correctness + eviction under `max_dedup`), `soak_heartbeat` (register/heartbeat/deregister + auto-eviction). All pass cleanly on 5.4.17. See `tests/soak/README.md`. Completes the soak-test infrastructure seeded in 2.4.0.

### Changed
- **Docs sweep across the tree** — README (v2.4.x module map + 4 dist profiles + sigil-dep note), CLAUDE.md (305-assertion matrix, cc5 5.4.17 quirks, new `map_new_str` guidance), `docs/architecture/overview.md` (new modules + 4-profile matrix), `docs/development/dependency-watch.md` (first-party sigil dep per-profile), `docs/development/threat-model.md` (rows for signed_envelope, admin, patra_queue), `docs/guides/testing.md` (current 341 assertions with separate `test_patra_queue` entry).
- **Roadmap** — QUIC + AES-NI paired as the next sigil arc (sigil 2.10 or 2.9.1 will bundle X25519 + AES-NI dispatch wiring). HKDF-as-gap note removed (shipped in sigil 2.9.0).

## [2.4.0] — 2026-04-20

Engineering-backlog minor release. All four roadmap items shipped;
additive-only (no breaking changes to the 2.3.x surface).

### Changed
- **Cyrius toolchain pin bumped to 5.4.17** (was 5.4.12-1 at start of 2.4.0-dev cycle). Brings in: (a) the `lib/hashmap.cyr` Str-key fix (5.4.14) — new `map_new_str()` + content-derived `hash_str_v`; resolves the ~3% collision rate surfaced by majra's own soak test and filed as `cyrius/docs/development/issues/stdlib-hashmap-str-key-collision.md`; (b) refreshed `lib/fnptr.cyr` and `lib/toml.cyr`; (c) bundled `lib/sigil.cyr` now 2.9.0.
- **Sigil dep bumped 2.8.4 → 2.9.0** (`cyrius.cyml` `[deps.sigil]`, `lib/sigil.cyr` refreshed). 2.9.0 adds HKDF (RFC 5869) and stages the AES-NI scaffold; majra's AES-GCM surface is unchanged on the wire, and the software AES-GCM path still runs (AES-NI is deferred at the sigil layer pending the cc5 inline-asm codegen fix scheduled for 5.5.x — filed at `cyrius/docs/development/issues/inline-asm-stores-silently-drop-when-fn-included.md`).
- **`src/queue.cyr`** switched from `map_new()` to `map_new_str()` for the managed-queue job map. Soak test's `mq_job_count` invariant is now authoritative (was informational-only under the hashmap bug). All 305 assertions pass.

### Added

- **Soak-test infrastructure** (`tests/soak/`) with `soak_queue.cyr`
  as the first target — 5k-round managed-queue lifecycle stress.
  Flushed out a real upstream cyrius stdlib bug along the way:
  `hash_str` in `lib/hashmap.cyr` expects a cstr but is routinely
  called with Str struct pointers (via `map_set(m, str_from_int(id),
  ...)`) — produces ~3% collision rate. Filed upstream at
  `cyrius/docs/development/issues/stdlib-hashmap-str-key-collision.md`.
  Soak test reports the `mq_job_count` (map-backed) discrepancy
  informationally and asserts on counter-backed `mq_total_completed`
  for the authoritative invariant. `tests/soak/README.md` documents
  the workflow.

- **Sigil-signed envelopes** (`src/signed_envelope.cyr`) — Ed25519
  signatures over a deterministic canonical encoding of envelope
  fields (`id_hi|id_lo|timestamp|to_kind|len-prefixed from|to_name|
  payload`). API: `signed_envelope_new(e, sk, pk)` /
  `signed_envelope_verify(se, expected_pk)`. Verify codes: 0 ok,
  1 bad input, 2 pk mismatch, 3 invalid signature. 9 assertions
  in `test_backends` — clean roundtrip, tamper detection, identity
  binding via `expected_pk`.

- **HTTP admin/metrics endpoint** (`src/admin.cyr`) — read-only
  observability surface over `lib/http_server.cyr`. Routes: `/health`,
  `/fleet` (JSON fleet stats), `/ratelimit` (JSON ratelimiter stats).
  Localhost-only by default; NO auth, NO mutation. Operator-facing,
  intended behind a reverse proxy for anything beyond a single host.
  5 assertions in `test_backends` — handler wiring and JSON body
  content. Socket-accept loop test belongs in `test_live` (follow-up).

- **Patra-backed persistent queues** (`src/patra_queue.cyr`) — durable
  alternative to the in-memory managed queue. Single `jobs` table
  in a `.patra` file, survives process restart. API:
  `patra_queue_new(path)` / `patra_queue_enqueue(q, priority, payload)`
   / `patra_queue_dequeue(q)` / `patra_queue_complete(q, id)` /
  `patra_queue_fail(q, id)` plus queued/running/completed counts.
  Priority matches `src/queue.cyr` convention (CRITICAL=0 highest,
  BACKGROUND=4 lowest). 17 assertions in a new `test_patra_queue`
  entry point (separate from `test_backends` to stay under the cc5
  16384 fixup cap) — enqueue, priority-ordered dequeue, complete,
  and reopen-with-persistence verified.

- **Two new dist profiles** to keep the default bundle lean:
  - `[lib.signed]` → `dist/majra-signed.cyr` (core + signed envelopes,
    requires sigil at consume-time) — 3215 lines
  - `[lib.admin]` → `dist/majra-admin.cyr` (core + admin endpoint) —
    3201 lines

### Tests (all suites on 5.4.12-1)

- core (`./build/majra`): 150 pass
- expanded (`tests/test_core.tcyr`): 96 pass
- backends (`tests/test_backends.tcyr`): 42 pass (was 25 in 2.3.1, +17 from
  signed_envelope + admin)
- patra_queue (`tests/test_patra_queue.tcyr`): 17 pass (new entry point)
- **Total: 305 assertions, up from 271 in 2.3.1** (+34)
- Fuzz: 3/3 clean, bench 17/17 clean
- Soak: `soak_queue` runs 5k ops to completion (flags the hashmap
  informational metric as expected)

### Notes

- The patra_queue dequeue and filter paths scan all rows client-side
  because patra 1.1.1 returns a null result set for queries with a
  `WHERE` clause (verified; works for WHERE without problem once given
  the right syntax but our column-list SELECTs returned null for
  reasons that looked schema-dependent — kept the SELECT * + client
  filter path for now; revisit when patra gains a more tolerant SQL
  parser or we adopt column indices directly).
- Admin endpoint is **localhost-only by design** — binding to 0.0.0.0
  without a fronting proxy that handles auth is a misuse.



## [2.3.1] — 2026-04-20

Patch release: wires sigil 2.8.4's real AES-256-GCM into `src/ipc_encrypted.cyr`
(the 2.3.0 stub was non-functional — no downstream consumer was relying on
the previous plaintext-in-base64 behavior), and rolls the Cyrius toolchain
pin forward through the 5.4.9–5.4.12-1 arc. Tests 267 → 271 (+4 from a
revived multi-threaded `cbarrier_arrive_and_wait` case that crashed under
5.4.8 and was fixed upstream in 5.4.10).

### Changed
- **Cyrius toolchain pin bumped to 5.4.12-1** (was 5.4.8 when 2.3.0 shipped). Brings in four upstream fixes: (a) the `_thread_spawn` inline-asm clone trampoline in `lib/thread.cyr` (5.4.10) that fixes the RBP/child-stack race crashing multi-threaded `cbarrier_arrive_and_wait` — see cyrius `docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` (filed by majra 2.3.0); (b) an aarch64 SP-alignment fix in the same trampoline (5.4.11, LDP-pair load instead of two LDRs to avoid SIGBUS); (c) the `cyriusly` version-manager script + arch-peer syscalls packaging restored in 5.4.12 (5.4.11 release tarballs dropped `cyriusly` from `bin/`); (d) the bundled `lib/sigil.cyr` now reliably resolves to 2.8.4 in 5.4.12-1 (5.4.10 and 5.4.12 shipped stale 2.8.3 snapshots — being fully addressed in the 5.4.x closeout by removing hardcoded-version multi-sourcing). majra independently vendors `lib/sigil.cyr` at 2.8.4 per the `[deps.sigil]` pin, so the stdlib bundle version isn't load-bearing here.

### Fixed
- **Multi-threaded `cbarrier_arrive_and_wait` now works.** `tests/test_core.tcyr` revives the 3-thread blocking test that was stubbed-out with a non-blocking-only fallback under 5.4.8. Expanded suite: 92 → 96 assertions. Removed the local `tests/repro_aaw_crash.cyr` — fixed upstream.

### Added
- **Real AES-256-GCM** in `src/ipc_encrypted.cyr` — the crypto path is no longer a stub. Wires in sigil 2.8.4's `aes_gcm_encrypt` / `aes_gcm_decrypt` (NIST SP 800-38D, constant-time tag verification, key zeroization on close).
- **sigil vendored as a dep** — `cyrius.cyml` gains `[deps.sigil] tag = "2.8.4"` pointing at `dist/sigil.cyr`; `lib/sigil.cyr` (bundled ~5.8k lines) is committed so CI doesn't need `cyrius deps` resolution for the backends profile.
- **AES-GCM roundtrip test** in `tests/test_backends.tcyr` — encrypts, decrypts with valid tag, and decrypts with a flipped-bit tag to confirm the AEAD contract (error + zeroed plaintext) holds through the wire layer. Backend suite: 20 → 25 assertions.

### Changed
- **Wire format for encrypted IPC** changed from `base64(nonce || plaintext_stub)` to `nonce(12) || ciphertext(N) || tag(16)` — the real GCM shape, no base64 overhead. Incompatible with any prior (stub-era) frames, but there were no such frames in production: the prior impl was plaintext-in-base64 and never semantically secure.
- **Removed stub AES S-box** from `src/ipc_encrypted.cyr` (was 32 of 256 bytes, never functional). Sigil owns the full FIPS-197 S-box now.
- **`encrypted_ipc_close`** now zeroes the 32-byte key buffer before close (defense-in-depth; was leaving the PSK in memory).

### Docs
- **`docs/development/roadmap.md`** — AES-256-GCM moves from "Open Items" (AES-NI stub) to shipped-via-sigil. AES-NI hardware acceleration remains deferred at the sigil layer (pending Cyrius inline asm).

## [2.3.0] — 2026-04-19

Brings majra onto the modern Cyrius 5.4.x manifest + distribution
convention. No runtime behavior change; this is the scaffold
refresh libro did in its 1.1.0 → 2.0 arc, catching majra up.

### Changed
- **Cyrius toolchain pinned to 5.4.8** (cc5), up from 3.2.6 (cc3). 14-minor jump pulls in: `\r` escape, negative literals, compound assignment, undefined-function-as-error, 16384 fixup cap (up from 8192), and the PE-aware backend from 5.4.8.
- **Manifest `cyrius.toml` → `cyrius.cyml`** — matches first-party convention (libro, yukti, cyrius, sakshi, patra, sigil). Now uses `[package] / [build] / [lib] / [lib.backends] / [deps]` sections. `version = "${file:VERSION}"` makes `VERSION` the single source of truth.
- **CI toolchain resolution**: `.github/workflows/{ci,release}.yml` no longer hardcode `CYRIUS_VERSION`. They grep the pin out of `cyrius.cyml` at install time, same shape as libro / yukti.
- **`scripts/version-bump.sh`** simplified — `cyrius.cyml` uses `${file:VERSION}` so there's nothing to sed in the manifest after a bump.

### Added
- **`dist/majra.cyr`** (core engine, ~3k lines) and **`dist/majra-backends.cyr`** (~4.2k lines, adds redis / postgres / ipc_encrypted / ws). Produced by `cyrius distlib` (default) and `cyrius distlib backends` respectively. Consumers (daimon, AgnosAI, hoosh, sutra, stiva) pick which surface to pull via `[deps.majra] modules = ["dist/majra.cyr" | "dist/majra-backends.cyr"]`. Same distribution contract as libro — see `CLAUDE.md` § Distribution Contract.
- **`[lib.backends]` profile** in `cyrius.cyml` — bundles the 4 backend modules alongside the core 15 for consumers that want the full surface.
- **CI manifest-completeness gate** — asserts every `include "src/*.cyr"` in `src/main.cyr` is listed under `[lib] modules`. Mirrors libro's guard; prevents silently shipping a bundle missing a module.
- **CI dist-freshness gate** — regenerates both bundles and fails if `git diff dist/` is non-empty. Bundles must be regenerated and committed alongside any `src/` change.
- **Release asset**: both `dist/*.cyr` bundles now attached to the GitHub Release alongside the source tarball and `build/majra` binary.

### Docs
- **`CLAUDE.md` rewritten** — dropped cc3-era quirks that are resolved under cc5 (`\r`, negative literals, `+=`, fixup cap, `map_get`-after-`map_set`). Added the distribution contract and CI gates. Build commands reflect `cyrius.cyml` / `cyrius distlib`.
- **`README.md` updated** — `v2.3.0` header, `[deps.majra]` integration snippet, build section reflects `dist/` bundles. Removed the `0 - priority` idiom from the Redis quickstart (cc5 supports negative literals).
- **`docs/architecture/overview.md`** — added "Distribution profiles" table explaining `dist/majra.cyr` vs `dist/majra-backends.cyr`; backends section renamed to `[lib.backends] profile only`; cc3-era "clobbers locals" principle rewritten to reflect cc5 improvement.
- **`docs/development/roadmap.md`** — relay dedup + barrier `arrive_and_wait` moved to "revisit under cc5" (cc3 root cause expected to be fixed); added patra 1.1.1 integration and `lib/http_server.cyr` evaluation items.
- **Relocated stale benchmark dumps** — `benchmark-rustvcyrius2.md` + `benchmarks.md` moved from repo root into `docs/benchmarks/`. Empty `programs/` directory removed.

### Source modernization (cc5 idioms)
- **`src/redis_backend.cyr`** — `_sb_crlf` now uses `str_builder_add_cstr(sb, "\r\n")`; dropped the byte-13/byte-10 `store8` hack and its 4-line scratch buffer. Replaced `return 0 - 1;` with `return -1;`.
- **`src/dag.cyr`** — `map_set(in_degree, sid, 0 - 1)` → `map_set(in_degree, sid, -1)`.
- **`src/main.cyr`** — backend-module include comment reframed: the split is now a distribution-profile decision, not a fixup-cap workaround (cap is 16384 on cc5, up from 8192 on cc3).

### Stdlib refresh
- **17 stdlib modules re-vendored from Cyrius 5.4.8** — `alloc`, `args`, `base64`, `bench`, `chrono`, `fmt`, `fnptr`, `hashmap`, `http`, `json`, `math`, `patra`, `sakshi`, `str`, `string`, `toml`, `vec`. `sakshi_full.cyr` kept as-is (not in upstream).

### Repo hygiene
- **`.gitignore` pruned** — removed Rust-era entries (`/target/`, `criterion/`, `proptest-regressions/`, `supply-chain/.cache/`, `lcov.info`, `tarpaulin-report.html`, `fuzz/target/`) that remained after the 2.0 Rust→Cyrius port. Added `.claude/`.

## [2.2.0] — 2026-04-09

### Changed
- **Cyrius toolchain updated to v3.2.6** (cc3 compiler)
- **Stdlib synced to v3.2.6** — updated `hashmap.cyr`, `hashmap_fast.cyr`, `json.cyr`, `string.cyr`
- **`map_count` → `map_size`** across all source modules (17 call sites) — uses new idiomatic alias
- **Chained `if/break` fix** in `postgres_backend.cyr` — uses compound `||` conditions per cc3 3.2.6 fix
- **Bench file extension**: `bench_all.cyr` → `bench_all.bcyr` for `cyrius bench` auto-discovery

### Added
- **New stdlib modules from 3.2.6**:
  - `patra.cyr` — structured storage, SQL queries, transactions, SHA-256
- **New stdlib functions**:
  - `map_get_or(m, key, default)` / `fhm_get_or(m, key, default)` — get with default value
  - `map_size(m)` / `fhm_size(m)` — count aliases
  - `strstr(haystack, needle)` — substring search

### Fixed
- `json.cyr` upstream fix: chained `if/break` inside while loops (broken in cc3 < 3.2.6)

## [2.1.1] - 2026-04-09

### Changed
- Cyrius toolchain pinned to v3.2.5 (cc3 compiler, minimum version)

## [Unreleased]

## [2.1.0] — 2026-04-09

### Changed
- **Cyrius stdlib synced to v3.2.1** — vendored `lib/` updated from 28 to 35 modules, all existing modules refreshed to upstream
- **Binary size**: 93 KB → 108 KB (expanded stdlib)
- **Build tooling references**: `cc2` / `cyrb` → `cyrius` across README, CONTRIBUTING, dependency-watch docs
- **Test runner**: fixed benchmark invocation (direct build+run instead of `cyrius bench`)

### Added
- **7 new stdlib modules** vendored from Cyrius 3.2.1:
  - `sakshi.cyr` / `sakshi_full.cyr` — structured logging/tracing (v0.8.0, enum-based log levels)
  - `base64.cyr` — base64 encode/decode
  - `chrono.cyr` — timestamp formatting and parsing
  - `csv.cyr` — RFC 4180 CSV parser/writer
  - `hashmap_fast.cyr` — optimised hashmap variant
  - `http.cyr` — minimal HTTP/1.0 client
- **Upstream stdlib improvements** pulled into 9 existing modules:
  - `assert.cyr` — `assert_lt`, `assert_gte`, `assert_lte`, `assert_nonnull`
  - `io.cyr` — file locking: `file_lock`, `file_unlock`, `file_trylock`, `file_lock_shared`
  - `string.cyr` — `atoi()` for string-to-integer parsing
  - `regex.cyr` — bugfix: `str_replace` now uses `str_data`/`str_len` correctly
  - `str.cyr` — bugfix: `str_join` uses `str_builder_add` for Str separators
  - `syscalls.cyr` — inotify wrappers: `sys_inotify_init`, `sys_inotify_add_watch`, `sys_inotify_rm_watch`
  - `hashmap.cyr` — `map_iter` support via fnptr
  - `callback.cyr` — syscalls include for timing
  - `tagged.cyr` — `option_print`/`result_print` support

### Fixed
- Stale `cc2`/`cyrb` references in documentation (README.md, CONTRIBUTING.md, dependency-watch.md)
- Test runner benchmark command (`cyrius bench` → direct build+run of `benches/bench_all.cyr`)

## [2.0.0] — 2026-04-08

**Full port from Rust to Cyrius.** All 19 modules re-implemented from scratch with zero external dependencies.

### Changed
- **Language**: Rust → Cyrius (compiled via `cc2`, statically linked)
- **Build system**: Cargo → `cyrb` / direct `cc2` compilation
- **Dependencies**: 25 Rust crates → 0 (Cyrius stdlib only)
- **Binary output**: library crate → standalone executable (~93 KB)
- **Generics**: `T: Send + Clone + Serialize` → `i64` (pointer to heap struct)
- **Traits**: `MajraMetrics`, `Transport`, `WorkflowStorage` → function pointer vtables
- **Async/await**: tokio → threads + mutexes + futex wait/wake
- **DashMap**: → mutex-protected hashmap
- **Floating point**: `f64` rate tokens → fixed-point i64 (x1000 scaling)
- **UUID**: `uuid` crate → 128-bit random via `getrandom` syscall
- **Timestamps**: `chrono` → `clock_gettime(CLOCK_MONOTONIC)` nanoseconds

### Added
- **Redis backend** (`redis_backend.cyr`) — full RESP2 protocol implementation over TCP: SET/GET/DEL, sorted sets (ZADD/ZPOPMIN/ZCARD), PUBLISH, HSET/HGET, EVAL, KEYS, SETEX, EXPIRE
- **PostgreSQL backend** (`postgres_backend.cyr`) — wire protocol v3: startup, cleartext auth, simple query, row parsing, workflow table DDL/CRUD
- **WebSocket** (`ws.cyr`) — RFC 6455: SHA-1 implementation (RFC 3174), base64 encode/decode, WebSocket handshake (Sec-WebSocket-Accept), frame send/recv with masking, ping/pong
- **Encrypted IPC** (`ipc_encrypted.cyr`) — AES-256-GCM framing with nonce management, base64 wire encoding, key rotation. Crypto stubs ready for AES-NI (x86_64) and aarch64 intrinsics
- **295 test assertions** across 4 suites: core (144), expanded (92), backends (25), live (36)
- **17 benchmarks** covering all major operations
- **2 examples**: managed_queue, pubsub_tiers
- **Test runner**: `tests/test.sh` runs all suites + benchmarks

### Removed
- **QUIC transport** — deferred until sigil crypto port (TLS 1.3 dependency)
- **SQLite persistence** — no SQLite binding in Cyrius
- **Prometheus metrics** — replaced by generic function pointer vtable
- **Logging module** — `println` suffices

### Known Issues
- Cyrius compiler local variable clobbering across function calls — mitigated via globals
- Relay dedup and barrier `arrive_and_wait` affected by hashmap lookup issue in nested call contexts
- No `\r` escape in Cyrius string literals — RESP/HTTP/WebSocket use raw byte 13

## [1.0.4]

### Changed
- **License changed from AGPL-3.0-only to GPL-3.0-only** — updated `Cargo.toml`, `deny.toml`, `README.md`, `CONTRIBUTING.md`, `CLAUDE.md`, and `LICENSE` file
- **Dependencies updated** — 25 packages bumped to latest compatible versions (ICU 2.1→2.2, wasm-bindgen 0.2.115→0.2.117, libc 0.2.183→0.2.184, and others)

## [1.0.3]

### Fixed
- **`ws` feature missing `futures-util` dependency** — `ws` feature used `futures_util::{SinkExt, StreamExt}` but did not gate `dep:futures-util`, causing compilation failure when `ws` was enabled without `redis-backend` (which happened to bring `futures-util` in under `full`)

## [1.0.2]

### Changed
- **`redis` dependency upgraded from 0.27 to 1.x** — aligns with redis crate stable 1.0 release. No API changes required; `get_multiplexed_async_connection()`, `AsyncCommands`, `Script::invoke_async()` remain compatible. Consumers pinned to `redis 0.27` via majra can now use `redis 1.x` directly without version conflicts.

## [1.0.1]

### Added
- `EncryptedIpcConnection::rekey()` — key rotation API with nonce counter reset
- `EncryptedIpcConnection::needs_rekey()` / `messages_sent()` — nonce exhaustion tracking (warns at 2^31, errors at 2^32)
- `SlidingWindowLimiter` — approximate sliding-window rate limiter (~5% accuracy of exact, O(1) memory/time per key)
- `WorkflowEngine::resume()` — durable workflow execution: reload step results from storage, skip completed steps, resume from interruption point
- `ConnectionPool::with_circuit_breaker()` — per-endpoint circuit breaker (configurable failure threshold + cooldown)
- `CircuitBreakerConfig`, `CircuitState` — circuit breaker types (Closed/Open/HalfOpen)
- `ConnectionPool::circuit_state()` / `reset_circuit()` — circuit breaker introspection and manual reset
- `Relay::compact_dedup()` — DashMap shrink-to-fit to reclaim dead capacity after eviction
- `RateLimiter::compact()` / `SlidingWindowLimiter::compact()` — DashMap shrink-to-fit
- `NamespacedMetrics` — per-tenant metrics partitioning via prefix delegation
- Subscriber count warning at 40+ receivers per pattern (broadcast quadratic slowdown)
- Cached Redis Lua script SHA for `RedisRateLimiter` (EVALSHA optimization)
- `DirectChannel<T>` — zero-overhead broadcast channel, 73M msg/s, no topic routing
- `HashedChannel<T>` + `TopicHash` — hashed topic routing with coarse timestamp, 16M msg/s
- `TypedPubSub<T>` dual-pipe refactor — exact-topic subscribers use O(1) DashMap lookup (fast path), wildcard-only patterns iterate (slow path)
- 7 new dual-pipe + DirectChannel + HashedChannel benchmarks
- 4 new `SlidingWindowLimiter` tests

### Changed
- `TypedPubSub` internal storage split into `exact_subscriptions` + `pattern_subscriptions` for O(1) exact-topic publish
- `PostgresWorkflowStorage::connect_with_pool_size()` documents pool sizing formula (`cores * 2 + 1`, 10 MB/connection)
- Architecture overview documents three-tier pub/sub, circuit breaker, DashMap fragmentation mitigation

## [1.0.0] — 2026-03-26

**First stable release.** API freeze. Full feature coverage across pub/sub, queues, relay, IPC, heartbeat, rate limiting, barriers, DAG workflows, fleet scheduling, and distributed backends.

### Added

#### DAG workflow engine (`dag` feature)
- `WorkflowEngine<S, E>` — tier-based DAG executor with parallel step scheduling, retry with exponential backoff, and 4 error policies (Fail/Continue/Skip/Fallback)
- `TriggerMode` — `All` (AND) and `Any` (OR) join semantics for dependency resolution
- `WorkflowStorage` trait — db-agnostic async storage for definitions, runs, and step runs
- `StepExecutor` trait — consumer-defined step execution logic
- `InMemoryWorkflowStorage` — DashMap-backed default storage with retention policy (`evict_older_than`, `with_max_runs`)
- `SqliteWorkflowStorage` — SQLite-backed storage (behind `sqlite` feature)
- `topological_sort_tiers()` — modified Kahn's algorithm returning parallelizable tiers with trigger-mode-aware in-degree
- `WorkflowDefinition`, `WorkflowRun`, `StepRun` — full execution tracking types
- `WorkflowContext` — step output accumulation for downstream reference
- Validation: cycle detection, referential integrity for deps and fallbacks
- Cooperative cancellation via `AtomicBool` per run

#### Multi-tenant scoping (`namespace` module)
- `Namespace` — prefix-based tenant isolation for topics, keys, and node IDs
- `topic()`, `key()`, `node_id()`, `pattern()`, `wildcard()` — scoped identifier builders
- `strip_topic()`, `strip_key()` — reverse mapping to extract bare identifiers

#### PostgreSQL storage backend (`postgres` feature)
- `PostgresWorkflowStorage` — `WorkflowStorage` impl backed by `deadpool-postgres` connection pool
- `PostgresQueueBackend` — PostgreSQL persistence for `ManagedQueue` (mirrors `SqliteBackend` API)
- `ManagedQueue::with_postgres()` constructor
- Automatic table creation with `majra_` prefix
- `connect()`, `connect_with_pool_size()`, and `from_pool()` constructors

#### IPC encryption (`ipc-encrypted` feature)
- `EncryptedIpcConnection` — AES-256-GCM wrapper around `IpcConnection` using `ring`
- Pre-shared 256-bit key, monotonic nonce counter per direction
- `send()` / `recv()` encrypt/decrypt JSON payloads transparently

#### WebSocket bridge for pubsub (`ws` feature)
- `WsBridge` — bridges `PubSub` topics to WebSocket clients via `tokio-tungstenite`
- Clients subscribe via `{"subscribe": "pattern"}` JSON handshake
- `WsBridgeConfig` — configurable `max_connections` (default 1024)

#### Distributed rate limiting (`redis-backend` feature)
- `RedisRateLimiter` — distributed token-bucket rate limiter via atomic Redis Lua script
- Auto-expiring keys, compatible API style with in-process `RateLimiter`

#### Distributed heartbeat tracker (`redis-backend` feature)
- `RedisHeartbeatTracker` — cross-instance health coordination via Redis key TTLs
- `register()`, `heartbeat()`, `is_online()`, `get_metadata()`, `list_online()`, `deregister()`

#### Typed pub/sub (`TypedPubSub<T>`)
- `TypedPubSub<T>` — generic, type-safe pub/sub hub with backpressure, replay, and filters
- `BackpressurePolicy` — `DropOldest` (default) or `DropNewest`
- Automatic dead-subscriber cleanup on publish (configurable interval)
- `try_subscribe()` — capacity-checked subscription with `max_subscriptions` limit

#### Rate limiter enhancements
- `evict_stale(max_idle)` — periodic sweep of idle keys
- `RateLimitStats` — `total_allowed`, `total_rejected`, `active_keys`, `total_evicted`

#### Relay enhancements
- `send_request()` / `reply()` — request-response correlation via UUID and oneshot channels
- `evict_stale_dedup(max_idle)` — TTL-based dedup table eviction
- `evict_stale_requests(timeout)` — TTL-based pending request cleanup
- `set_max_dedup_entries()` — configurable dedup table cap with LRU eviction
- `RelayMessage::correlation_id` and `is_reply` fields

#### Observability & logging
- `metrics` module — `MajraMetrics` trait with no-op default and Prometheus implementation
- `NamespacedMetrics` — per-tenant metrics partitioning via prefix delegation
- `logging` feature — structured tracing via `MAJRA_LOG` env var
- Structured `#[instrument]` spans on ManagedQueue operations

#### Distributed primitives
- `AsyncBarrierSet` — async barrier with `arrive_and_wait()` and `AtomicBool` release flag
- `transport` module — `Transport` trait, `TransportFactory`, `ConnectionPool` with stale eviction
- `ConnectionPool::evict_stale(max_idle)` — TTL-based idle connection cleanup

#### Code quality
- `#[non_exhaustive]` on all public enums
- `#[must_use]` on all pure return types
- `#[inline]` on all hot-path accessors
- `///` doc comments on every public item
- `Counter` and `evict_from_dashmap` utilities

#### Repository infrastructure
- GitHub Actions CI (10-job pipeline) and release workflow
- LICENSE, CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md
- Makefile, `deny.toml`, `codecov.yml`, `rust-toolchain.toml`
- Fuzz targets (queue, pubsub, heartbeat)
- `supply-chain/` (cargo-vet), `scripts/version-bump.sh`
- `benchmarks.md` — 3-point trend tracking
- `docs/development/dependency-watch.md` — pinned versions and upgrade paths
- Live Redis integration test (`redis_live_full_lifecycle`) covering pub/sub, queue, rate limiter, heartbeat
- Live PostgreSQL integration test (`postgres_live_workflow_storage`) covering workflow CRUD
- 220 tests (unit + integration + doc-tests), 25+ benchmarks

### Changed
- `matches_pattern()` rewritten to iterative zero-allocation with inline depth tracking
- `ManagedQueue::dequeue()` releases tiers lock before DashMap mutation
- `ManagedQueue::cancel()` drops DashMap guard before awaiting tiers lock
- `RateLimiter` internals swapped from `Mutex<HashMap>` to `DashMap`
- `Relay` dedup map swapped to `DashMap`, stats to `AtomicU64`
- `ConnectionPool::acquire()` drops lock before async connect
- `PostgresWorkflowStorage::connect_with_pool_size()` — configurable pool size (was hardcoded to 16)
- Replay buffer fast-path for exact topic subscriptions (O(1) vs O(n) pattern scan)

### Fixed
- `AsyncBarrierSet::arrive_and_wait()` missed-wakeup race
- `TypedPubSub::publish()` delivered counter accuracy under `DropNewest`
- SQLite `persist()` no longer panics on serialisation failure
- IPC `write_frame` uses `u32::try_from` to prevent silent truncation

## [0.22.3] — 2026-03-22

### Changed
- Version bump for stiva 0.22.3 ecosystem release

## [0.21.3] - 2026-03-21

### Added

#### Thread safety
- `ConcurrentPriorityQueue<T>` — async-aware wrapper with `Notify`-based blocking dequeue
- `ConcurrentHeartbeatTracker` — `DashMap`-backed tracker with all `&self` methods
- `ConcurrentBarrierSet` — `DashMap`-backed barrier manager
- Compile-time `Send + Sync` assertions on all public types

#### Managed queue (`ManagedQueue<T>`)
- `ResourceReq` / `ResourcePool` — GPU-aware dequeue filtering
- `ManagedQueueConfig` — max concurrency enforcement
- `JobState` enum — `Queued → Running → Completed / Failed / Cancelled`
- `ManagedItem<T>` — lifecycle-tracked queue item
- `QueueEvent` — broadcast events on state transitions
- TTL-based eviction via `evict_expired()`
- `sqlite` feature — `SqliteBackend` persistence with WAL mode

#### Fleet & heartbeat
- `GpuTelemetry`, `FleetStats`, `EvictionPolicy`
- `register_with_telemetry()`, `heartbeat_with_telemetry()`, `fleet_stats()`

#### Error types
- `MajraError::InvalidStateTransition`, `ResourceUnavailable`, `Persistence`

### Changed
- `RateLimiter` and `Relay` internals to `DashMap` + `AtomicU64`

## [0.21.0] - 2026-03-21

### Added
- `envelope` — Universal message envelope with Target routing
- `pubsub` — Topic-based pub/sub with MQTT-style wildcard matching
- `queue` — Multi-tier priority queue with DAG dependency scheduling
- `relay` — Sequenced, deduplicated inter-node message relay
- `ipc` — Length-prefixed framing over Unix domain sockets
- `heartbeat` — TTL-based health tracking with Online → Suspect → Offline FSM
- `ratelimit` — Per-key token bucket rate limiter
- `barrier` — N-way barrier synchronisation with deadlock recovery
- `error` — Shared error types (MajraError, IpcError)
- Feature-gated modules: default = pubsub + queue + relay + heartbeat

[Unreleased]: https://github.com/MacCracken/majra/compare/v1.0.4...HEAD
[1.0.4]: https://github.com/MacCracken/majra/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/MacCracken/majra/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/MacCracken/majra/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/MacCracken/majra/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/MacCracken/majra/compare/v0.22.3...v1.0.0
[0.22.3]: https://github.com/MacCracken/majra/compare/v0.21.3...v0.22.3
[0.21.3]: https://github.com/MacCracken/majra/compare/v0.21.0...v0.21.3
[0.21.0]: https://github.com/MacCracken/majra/releases/tag/v0.21.0
