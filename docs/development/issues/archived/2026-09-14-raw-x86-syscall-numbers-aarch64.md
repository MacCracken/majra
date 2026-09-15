# Raw x86_64 syscall numbers run as DIFFERENT syscalls on aarch64-Linux — `_SYS_FCHMOD` 91 (IPC access control), `SYS_GETRANDOM` 318 (envelope ids), `syscall(35)` (DAG backoff) — RESOLVED

**Status:** ✅ **RESOLVED in majra 2.7.3** (2026-09-14, pin 6.6.2 → 6.6.4). Was 🔴 **OPEN — MEASURED**
at majra **2.7.2** (the latest tag at filing; worktree clean), from daimon's `cyrius build --aarch64` of
the vendored `dist/majra.cyr` under cyrius 6.6.4. Every routing fact below was re-derived against
majra's own **6.6.2** pin as well — the two toolchains' aarch64 renumbering tables differ only in rows
(5, 6 = fstat/lstat) that majra does not touch, so nothing here depends on which pin builds it.
**Resolution:** steps 1–6 and 8 shipped as filed — `sys_fchmod`, `sys_getrandom` on every target,
`clock_now_ns` / `clock_epoch_ns`, a new internal `_majra_sleep_ns` (chrono `sleep_ms` to a monotonic
deadline; as shipped it also clamps each request to ≤ 1 s and terminates on a non-advancing clock,
which step 3's sketch did not) for the DAG backoff and the raw-35 test/soak sites, `lib/chrono.cyr` in
the thirteen `--no-deps` entry points that lacked it (all fourteen carry it now), raw 53 →
`SYS_SOCKETPAIR`, and — a site the §1 table missed — the `tests/test_backends.tcyr:410` mode check
(raw `syscall(4)` with a hardcoded x86_64 `st_mode` offset; 4 is a routed row, but 24 is `st_uid` on
aarch64) → `sys_stat` + `STAT_MODE`; the two step-7 assertions are in and mutation-verified; every
suite, fuzz, soak and example passes cross-built under `qemu-aarch64` (expanded suite 100×, the other
three 25×, 0 failures). **Not done:** the *run* half of the step-7 lane — CI gained a raw-syscall-literal
grep gate and a build-only `--aarch64` cross-build gate that fails on `duplicate symbol 'SYS_` (the
cheap belt), but the tests ran under qemu locally only; the run lane stays the roadmap item (now
§ "aarch64 CI lane — build **and run** under `qemu-aarch64`"). Full account in `CHANGELOG.md`
§ [2.7.3]. [Two filing errors, corrected here rather than in the body: (a) the Severity line's
"agnos is unaffected by the three" was wrong — the 318 `var` also overrode the agnos peer's
`SYS_GETRANDOM` (45), and the agnos arm's `sys_getrandom` spells that name, so it issued 318 and
2.7.2's envelope ids were 0/0 on agnos too; (b) §1's "neither declares … `socketpair`" is wrong — both
Linux peers declare `SYS_SOCKETPAIR` (53 / 199, as step 6 says); only the `nanosleep` half holds. Raw
53 still draws no literal diagnostic (re-probed at 6.6.4), just not for the reason §1 gives.] Body and
line numbers below are 2.7.2's, left as filed.
**Filed:** 2026-09-14, by a daimon consumer (daimon 2.1.4 toolchain bump). Claims were adversarially
verified against the source before filing; line numbers are 2.7.2's.
**Severity:** **P1 for any aarch64-Linux consumer**, three independent failures in shipped code:
the AF_UNIX IPC transport cannot bind (fail-closed — see §2), every envelope id generation fails
closed, and the DAG executor's retry backoff never sleeps. x86_64 is unaffected; agnos is unaffected
by the three (it has its own `#ifdef` arms) but see §4 for a clock note.
⚠ The roadmap's "aarch64 cross-build wiring" trigger — *"any non-x86_64 consumer"* — is already met:
daimon's `release.yml` ships a `daimon-aarch64` asset with this bundle inside it.
**Component (shipped, in all four dist bundles):** `src/ipc.cyr:20` → `:121`; `src/envelope.cyr:22`
→ `:88`; `src/dag.cyr:312, :316`. **Routed-today (correct by the emitter's table, not by the source):**
`src/ipc.cyr:13-17`, `src/envelope.cyr:21` → `:37, :64`, `src/postgres_backend.cyr:296, :299`,
`src/redis_backend.cyr:244, :247` (backends bundle only). **Test/soak code (majra's own aarch64 run,
not consumers):** `src/main.cyr:270, :274`, `tests/test_core.tcyr:1431, :1607`,
`tests/soak/soak_heartbeat.cyr:112`, `tests/test_backends.tcyr:334, :368, :570`.

## 1. What was measured, and why the compiler cannot save it

daimon's aarch64 cross-build log, from the vendored bundle (the only build-time signal any of this emits):

```
warning:lib/majra.cyr:205:19: duplicate symbol 'SYS_GETRANDOM' redefined with conflicting value (last definition wins)
```

`src/envelope.cyr:22` declares `var SYS_GETRANDOM = 318;` — the x86_64 number. The aarch64 peer declares
`SYS_GETRANDOM = 278` (`lib/syscalls_aarch64_linux.cyr:214`; x86_64 peer `:91` = 318; agnos
`lib/syscalls_x86_64_agnos.cyr:474` = 45). A dep bundle is prepended AFTER the stdlib leaves, so on
aarch64 majra's 318 wins for every `syscall(SYS_GETRANDOM, …)` in the translation unit — the consumer's
own calls included.

cyrius's aarch64 backend emits `ESYSXLAT`, a **runtime** renumbering chain (`cmp x8,#N` rows on the
syscall-number register, so it applies identically to a literal and to a `var SYS_X = N` global) that
rewrites 36 x86_64 numbers to their aarch64 equivalents at 6.6.2 — 38 at 6.6.4, which adds 5 and 6 —
(+ 6 cyrius-private ≥1000 aliases on both) and passes every other number through to `svc`
**verbatim** (`src/frontend/parse_expr.cyr`, v6.5.51 note:
*"a stray x86_64 number is indistinguishable from an intended native one, so the build succeeds and a
DIFFERENT, VALID syscall runs"*). The routed set, decoded from `src/backend/aarch64/emit.cyr` with the
awk pipeline `tests/gates/platform/raw_syscall_literals_routed.sh` uses, at tag **6.6.2** (majra's pin):

```
0 1 2 3 4 7 9 10 11 12 16 22 39 41 42 43 48 49 50 51 54 55 60 72 73 74 75 79 82 88 217 228 232 262 269 280  (+1022 1039 1049 1054 1073 1074)
```

(6.6.4 adds 5 and 6.) Against that, every raw number in majra's tree:

| site | number | x86_64 meaning | ESYSXLAT | on aarch64-Linux it issues |
|---|---:|---|---|---|
| `src/ipc.cyr:20` → `:121` | 91 | `fchmod` | **not routed** | **`capset`** (`asm-generic/unistd.h:254`). aarch64 `fchmod` is 52 (`syscalls_aarch64_linux.cyr:216`) |
| `src/envelope.cyr:22` → `:88` | 318 | `getrandom` | **not routed** | not a number the aarch64 peer declares; `-ENOSYS` (the shape the cyrius 6.6.4 CHANGELOG records for agnostik's identical raw 318) |
| `src/dag.cyr:312, :316` | 35 | `nanosleep` | **not routed** | **`unlinkat`** — `SYS_UNLINKAT = 35` (`syscalls_aarch64_linux.cyr:73`). aarch64 does have a `nanosleep` (101) — the peer just never declared it and the emitter never routed 35 to it |
| `src/main.cyr:270, :274` (test body), `tests/test_core.tcyr:1431, :1607`, `tests/soak/soak_heartbeat.cyr:112` | 35 | `nanosleep` | **not routed** | same |
| `tests/test_backends.tcyr:334, :368, :570` | 53 | `socketpair` | **not routed** | `fchmodat` (`SYS_FCHMODAT = 53`, `syscalls_aarch64_linux.cyr:76`) |
| `src/ipc.cyr:13-17`, `src/postgres_backend.cyr:296, :299`, `src/redis_backend.cyr:244, :247` | 41 42 43 49 50 | socket connect accept bind listen | routed → 198 203 202 200 201 | correct today |
| `src/envelope.cyr:21` → `:37, :64` | 228 | `clock_gettime` | routed → 113 | correct today |

⚠ **Only the 318 line produces a warning**, and only because the name collides with a peer-declared
symbol. The 91 site is a `var _SYS_FCHMOD` (leading underscore — a `^var SYS_` grep does not find it,
which is how the first draft of this filing missed it), and the raw-35 / raw-53 literals get nothing:
the v6.5.51 raw-literal diagnostic fires only for a literal that has a row in the generated
`syscall_xlat.cyr` table, and that table holds only syscalls *both* peers declare — neither declares
`nanosleep` or `socketpair`. So two of the three shipped defects are invisible at build time on every
toolchain, and the third is a warning in a consumer's log.

## 2. What the three unrouted shipped sites do on aarch64

**`ipc_bind` — the AF_UNIX access-control path (`src/ipc.cyr:107-125`).** The 2.6.9 → 2.6.10 fix sets
the socket to 0600 *before* `bind` precisely because bind copies the sockfs inode's mode to the
filesystem inode and `connect` checks that: *"a consumer trusting that promise got a world-connectable
endpoint where connecting IS authenticating"*. On aarch64 that `syscall(_SYS_FCHMOD, fd, 384)` is
`capset(header = fd, data = 384)`. The header argument is a small integer, not a mapped pointer, so the
kernel's copy of the header faults and the call returns `-EFAULT`; the `< 0` check at `:121` then
closes the fd and returns `Err(ERR_IPC)` — **every `ipc_bind` fails**, on every path, deterministically.
That is the fail-closed outcome. The fail-open one — a kernel that returned ≥ 0 — would bind the socket
world-connectable under `umask 022`, the exact regression the comment block documents closing. Either
way the Unix IPC transport is not usable on aarch64. Carried by all four bundles (`src/ipc.cyr` is in
`[lib]`, `[lib.backends]`, `[lib.admin]` and `[lib.signed]`).

**`envelope` id generation (`src/envelope.cyr:80-96`).** The fill loop calls
`syscall(SYS_GETRANDOM, &buf + got, 16 - got, 0)` up to 8 times and advances only on `n > 0`. `-ENOSYS`
never is, so after 8 attempts it takes the entropy-unavailable branch and returns `ret2(0, 0)`. That
branch is correctly fail-closed (*"a predictable id is worse than nothing"*) — which means **every
envelope id fails**. `test_envelope` (`src/main.cyr:76-97`, asserts `envelope_id_hi(e) != 0` at `:88`)
would catch this the first time majra's own tests run on aarch64.

**DAG executor backoff (`src/dag.cyr:296-319`).** The non-agnos arm issues
`syscall(35, &ts, &rem)` and re-enters only on `sr == -4` (EINTR). On aarch64 that is
`unlinkat(dirfd = low 32 bits of the stack address of ts, pathname = &rem, flags = whatever x2 holds)`.
In practice it returns an error — `EBADF` for the garbage dirfd, unless `rem`'s uninitialised bytes
happen to start with `/`, in which case the garbage path is resolved absolutely — and no error is
`-4`, so the loop exits at once: **the backoff never sleeps** and the retry becomes a hot spin, the
failure mode the comment at `:301-306` was written to prevent. ⚠ It is also a *file-removal* syscall
handed a pointer-derived path; "returns an error in practice" is an observation, not a guarantee.
`test_dag_retry` (`tests/test_core.tcyr:1543-1554`) traverses this path exactly once but asserts only
`RUN_COMPLETED` and the attempt count — no elapsed-time check — so a never-sleeping backoff passes it.

## 3. This is the class cyrius 6.6.4 swept from its own stdlib

- cyrius CHANGELOG [6.6.4]: *"Raw x86_64 syscall numbers in arch-neutral stdlib code ran as DIFFERENT
  syscalls on aarch64-Linux"* — its closing list names agnostik (raw 318), vidya, kavach, aegis, … for
  the consumer pin sweep. majra is absent only because nothing had cross-built it.
- The stdlib's own `sleep_ms` (`lib/chrono.cyr:156-167`) replaced its raw `syscall(35)` at cyrius
  **6.0.65** (issue `2026-06-04-macos-nanosleep-syscall-35-not-in-esysxlat`; the "no plain nanosleep"
  there is a *Darwin* fact — on aarch64-Linux the problem is that 35 is a different call). majra kept the
  pre-6.0.65 shape.
- The new `raw_syscall_literals_routed` gate scans cyrius's `lib/` + `cbt/` only; a package's own raw
  literals are outside it. majra's CI has no aarch64 lane (`ci.yml:105`: *"remains unwired … no
  consumer is blocked"* — see the daimon note above), so nothing majra owns can see any of this.

## 4. What would close it

⚠ **A constraint the first draft got wrong:** majra's CI builds every entry point with `--no-deps`
(`ci.yml:99, :119, :124, :133, :140, :149, :155, :157`; `release.yml:82`), so only explicit `include`
lines count and `[deps] stdlib` is irrelevant to those builds. `lib/chrono.cyr` is included by exactly
one of the fourteen entry points that reach `envelope.cyr` / `dag.cyr` (`tests/test_backends.tcyr:19`).
majra has already recorded this failure three times (`src/envelope.cyr:44-50`, `src/relay.cyr:36-39`,
`tests/test_core.tcyr:839-840`: *"reaching for it built fine locally and failed there with
`undefined function 'clock_epoch_ns'`"*). Steps 2 and 3 below therefore require adding
`include "lib/chrono.cyr"` (after `lib/syscalls.cyr`, before `src/envelope.cyr`) to the thirteen
`--no-deps` entry points — or rolling a local `poll(7)`-based wrapper next to `time_now_ns`. Consumers
are unaffected either way (all four `dist/*.deps` sidecars already list `chrono`).

1. **`_SYS_FCHMOD` — delete the `var`, call the wrapper.** `sys_fchmod(fd, mode)` is defined at
   `lib/syscalls_linux_common.cyr:152`, which both Linux peers and the macOS peer include, each
   spelling its own `SYS_FCHMOD` (x86_64 91, aarch64 52, macOS 91). The call sits inside the
   `#ifndef CYRIUS_TARGET_AGNOS` arm, so agnos/Windows do not see it. This is the security-relevant
   one; land it first.
2. **`SYS_GETRANDOM` — delete the `var`, call the wrapper.** `sys_getrandom(buf, len, flags)` exists on
   every target majra builds (`lib/syscalls_linux_common.cyr:475` for both Linux arches,
   `lib/syscalls_x86_64_agnos.cyr:1612`, `lib/syscalls_windows.cyr:203`), same arity, same
   `> 0 == bytes` convention, so `envelope.cyr:84-89`'s `#ifdef CYRIUS_TARGET_AGNOS` / `#ifndef` split
   collapses to the one line the agnos arm already has. Deleting the `var` also silences the
   consumer-side warning. (No chrono needed for this step.)
3. **`syscall(35, …)` in `dag.cyr` — `sleep_ms` + a monotonic deadline.** chrono's `sleep_ms` is
   `poll(NULL, 0, ms)` on Linux/macOS (7 → ppoll 73 is an ESYSXLAT row at 6.6.2 already), kernel32
   `Sleep` on PE, `#41` on agnos — so the `#ifdef CYRIUS_TARGET_AGNOS sys_sleep_ms` arm at
   `dag.cyr:297-300` goes away too. It discards `poll`'s return, so `-EINTR` ends a sleep early with no
   remainder; keep the EINTR-with-remainder intent as
   `deadline = clock_now_ns() + total_ns; while ((left = deadline - clock_now_ns()) > 0) { sleep_ms(max(1, ceil_ms(left))); }`.
   ⚠ Clamp to ≥ 1: `poll(…, 0)` returns immediately (a sub-ms tail becomes a spin) and a negative ms is
   an infinite wait on the aarch64 route. The backoff is `attempt * 10 ms` — whole milliseconds, so
   `poll`'s granularity loses nothing.
4. **`SYS_CLOCK_GETTIME` — delete the `var`; `time_now_ns` → `clock_now_ns()` AND `time_epoch_ns` →
   `clock_epoch_ns()`** (`envelope.cyr:37` and `:64` — the second caller, `CLOCK_REALTIME`, is easy to
   miss). Honest accounting: on Linux chrono's `clock_now_ns` issues the *same* raw `syscall(228, …)`
   (`lib/chrono.cyr:54`), so this does not remove the dependence on the 228 row — it moves it into
   stdlib code that cyrius's `raw_syscall_literals_routed` gate does scan, so a trimmed row would be
   caught upstream instead of here. On **agnos it is a behaviour change**: chrono reads `#95`
   (`sys_uptime_us`, rdtsc-backed) since cyrius 6.6.1, whose `:16-29` note explains that `#40`
   (`sys_uptime_ms` — what `envelope.cyr:29` uses today) is *frozen* for a foreground `run` program.
   Adopting chrono fixes that too; say so in the CHANGELOG rather than calling it equivalent.
5. **Raw 41/42 in the two backends — do NOT spell `SYS_SOCKET` / `SYS_CONNECT`.** Those names are
   declared on the Linux and macOS peers but **not** on agnos or Windows, and the backend files have no
   `#ifdef` guards, so that spelling turns a build that compiles today into an undefined-symbol error
   there. Reuse `ipc.cyr:13-14`'s `_SYS_SOCKET` / `_SYS_CONNECT` (target-neutral, already earlier in the
   `[lib.backends]` module order), or adopt `tcp_socket()` + `sock_connect(fd, addr, port)` from
   `lib/net.cyr` with `Result` unwrapping — noting that net.cyr's agnos `sock_connect` is a different
   primitive (`sys_sock_connect` #47, socket+connect in one, blocks ~8 s), so that path changes agnos
   semantics, not just numbering. (`lib/net.cyr` itself carries `NSYS_SOCKET = 41` etc., so it leans on
   the same rows; the benefit is one spelling, not a different one.)
6. **Test and soak sites**, or majra's own aarch64 run fails for reasons unrelated to the fixes:
   `src/main.cyr:270, :274` (`test_heartbeat_eviction`, no `#ifdef` split at all — it also issues raw 35
   on an agnos build of the tests), `tests/test_core.tcyr:1431, :1607`, `tests/soak/soak_heartbeat.cyr:112`
   → `sleep_ms`; `tests/test_backends.tcyr:334, :368, :570` raw 53 → `SYS_SOCKETPAIR` (declared on both
   Linux peers: x86_64 `:100`, aarch64 `:228`).
7. **Wire the aarch64 lane** the roadmap already scopes as *"a discrete, unblocked verification task"*,
   and run the tests under `qemu-aarch64` — a build-only lane proves nothing here (§1: two of three
   defects emit no warning). Fail it on any `duplicate symbol 'SYS_` warning as a cheap belt. Add the two
   assertions that would have caught this: a `time_now_ns()`-bracketed elapsed check around a retried
   step (≥ 10 ms at attempt 1) and an id-uniqueness check across two `envelope_new` calls.
8. Regenerate the four dist bundles (`cyrius distlib --all`, or the four per-profile invocations
   `ci.yml:172-175` runs today).

## Related

- daimon `docs/development/issues/2026-09-14-aarch64-binary-issues-x86-syscall-numbers.md` — the
  consumer filing: daimon's own two unrouted numbers (52 → `fchmod`, disabling its per-IP rate limiter;
  160 → `uname`, disabling agent rlimits) plus this bundle's 318.
- cyrius CHANGELOG [6.6.4] "Raw x86_64 syscall numbers in arch-neutral stdlib code";
  `tests/gates/platform/raw_syscall_literals_routed.sh` — the derivation of the routed set used above.
- cyrius `lib/chrono.cyr:16-29` (the agnos #40 → #95 clock note) and `:156-167` (the 6.0.65 raw-35 note).
- majra `docs/development/roadmap.md` § "aarch64 CI lane — build **and run** under `qemu-aarch64`"
  (§ "aarch64 cross-build wiring" at filing; re-scoped at 2.7.3) — the trigger this filing meets;
  `src/ipc.cyr:107-120` — the fchmod-before-bind reasoning that step 1 preserves.
