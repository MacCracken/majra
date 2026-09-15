---
name: Cyrius compiler quirks
description: Toolchain-side gotchas that affect how majra code is written. Refresh as cyrius evolves; archive (don't delete) resolved entries.
---

# Cyrius compiler quirks

> **Toolchain floor**: cyrius 6.4.65 (the folded sigil needs `thread_local_alloc`) — and 6.5.19 for a thread-safe `fl_alloc`, see the archive. Current exact pin in [`state.md`](state.md) | **Refresh cadence**: when the pin moves or a new quirk surfaces. | **Last verified**: 6.6.4 (2.7.3, 2026-09-14) — three quirks added from the aarch64 filing and the first `qemu-aarch64` run of every suite: #9 (syscall numbers are x86_64-spelled and `ESYSXLAT` passes unrouted ones through as a *different* syscall), #10 (per-peer `struct stat` layout), #11 (cold-TCG latency under qemu). #6 re-confirmed at 6.6.4 (`sys_uname` dormant under `--no-deps` built `OK` and warned); #7 gained the *`--no-deps` prepends nothing* rule (chrono: thirteen entry points gained the include, `test_backends` already had it). Snapshot re-measured at 110, bare-`lib sync` at 52. The resolved archive this header points at had been dropped from the file by the post-2.7.0 doc sweep (2443b6a, 2026-08-22; first tagged in 2.7.1) — the same commit that archived #8 — restored, with #8 in it. **Prior**: 6.5.35 (2.7.0 — the sweep's own label; the 2.7.0 tag itself still lists #8 as active) — quirk #8 re-checked against `lib/freelist.cyr` and **archived**: upstream locked it at 6.5.19 (`_fl_lock`, a `_threads_active`-gated CAS spinlock). Bare-`lib sync` file count re-measured at 49. **Prior**: 6.5.35 (2.6.8) — the undefined-fn `ud2` rule re-confirmed empirically against a clean-room consumer build (it is what made the missing-`sigil` sidecar a runtime SIGILL rather than a build failure); snapshot count and `lib sync --full` note refreshed. **Prior**: 6.4.83 (2.5.2) — quirks #4 and #6 re-checked; #6 rewritten.

Things about the cyrius compiler that affect how majra code is written. None of these are bug reports — they're *load-bearing facts about the toolchain*. If a pattern in `src/` looks weird, the answer is probably here.

For *majra-side conventions* (allocator discipline, struct-field layout, fl_alloc-vs-alloc), see CLAUDE.md § Cyrius Conventions.
For *dep-version-tied gotchas* (sigil's asm-offset drift), see [`dependency-watch.md`](dependency-watch.md).
For *the dev workflow*, see CLAUDE.md § Process.

---

## Active

### 1. Local variable clobbering across deep call chains

A local's value can look wrong after returning from a nested call. Rare under cc5 at the 5.10.x line, much rarer than cc3 / early-cc5, but still real on deep chains.

**Workaround**: promote the local to a module-level global. The pattern is well-trodden — sigil uses it heavily for AES / SHA / ed25519 state; majra reaches for it sparingly because most call chains aren't deep enough to trip it.

**Diagnosis**: if you suspect a fresh instance, print the value before and after the suspect call. If they disagree, promote.

### 2. Single-pass compiler — fixup-table cap

Forward references across function boundaries work via a fixup table, capacity 1,048,576 entries at the 6.6.4 pin (8192 on cc3, 16384 across cc5 5.x). Module include order still matters for type / struct visibility — a struct must be declared before its first use even if the use site is in a fn called later.

**Practical implication**: very large test entry points can blow the cap. `tests/test_patra_queue.tcyr` lives in its own entry point because adding it to `tests/test_backends.tcyr` exceeded the then-16384 cap at 2.4.0.

**If a "fixup table full" error fires**: split the entry point. Don't try to reorder.

### 3. Hashmap keys — `map_new()` is cstr, `map_new_str()` is `Str`

cyrius 5.4.14+ added `map_new_str()` + content-derived `hash_str_v` to fix a `Str`-struct key collision majra originally surfaced via soak tests (~3% collision rate against the cstr-shaped `hash_str`).

- `map_new()` for cstr keys (legacy / default).
- `map_new_str()` for `Str`-struct keys.

`src/queue.cyr` is the canonical `map_new_str()` user (managed-queue job map keyed on `str_from_int(id)`). Other modules keyed on cstrs keep `map_new()`.

Picking the wrong one compiles cleanly but corrupts at runtime via silent collisions. Match the key type at the call site.

### 4. `var buf[N]` sizing: **locals are N bytes, globals are N×8 bytes**

Two different rules — this asymmetry bit the 2.5.0 + 2.5.1 buffer audits and misleads first-read reviewers (verified empirically at cyrius 6.4.62; unchanged at 6.4.83 and 6.5.35):

- **Function-local `var buf[N]` = N bytes.** A byte-sized scratch buffer. A 16-byte `struct timespec` needs `var ts[16]`, not `var ts[2]` (= 2 bytes → overflow). Confirmed: `soak_heartbeat` phase B silently corrupted its node count until `var ts[2]`→`var ts[16]` (CHANGELOG 2.5.1); likewise `key[32]` (AES-256), `nonce[12]` (GCM IV), `buf[4]` (be32).
- **Module-level / global `var buf[N]` = N × 8 bytes** (N `i64` slots in the data segment). So a global `var _resp_buf[512]` genuinely holds **4096** bytes and `var _err_msg_buf[64]` holds 512 — byte-indexed access (`store8(&buf + pos, …)`) up to those larger bounds is in-range. **Compute a global's real capacity as `N*8` before "fixing" it**: `redis_backend.cyr`'s `while (pos < 4088)` loop over global `var _resp_buf[512]` (= 4096 B) is correct, *not* an overflow — a naive N-bytes reading flags a false positive here.

**Location (since cyrius 6.3.13):** function-local `var buf[N]` now lives on the **guarded thread stack** — an overflow SIGSEGVs against a `PROT_NONE` guard page instead of silently scribbling adjacent globals (pre-6.3.13 locals were static data; that's why the undersized-buffer class went latent for so long, then turned into hard crashes at the 6.3.13 pin). Module-level `var buf[N]` is still static data-segment storage shared across the program.

**Pattern**: heap-allocate (`fl_alloc(N)` / `alloc(N)`) anything you return out of a fn; a `Str`/pointer borrowing into a **global** scratch buffer is invalidated by the next writer, so use `var buf[N]` only for scratch consumed before the next write.

### 5. Inline-asm parameter loads were fragile pre-6.x — `param_load` pseudo fixes it

`mov rdi, [rbp-8]` -style byte-literal parameter loads inside `asm {}` blocks were tied to whatever stack-frame layout cc5 emitted; 5.10.x's expanded prologue shifted the slots and asm written against the old layout SIGILL'd. **cyrius 6.0.67+ exposes a `param_load(reg, idx)` asm pseudo** that resolves to the correct slot regardless of prologue shape, so this class is fixed at the toolchain.

**Implication for new majra code**: we still don't write inline asm, but if we ever need a hardware-acceleration hot path, use `param_load` rather than decoding `[rbp-N]` by hand.

**Implication for our deps**: this is why sigil was held at 2.9.0 through the 2.4.x line. Since 2.4.5 (cyrius 6.x) sigil's NI dispatch uses `param_load`, so the constraint is gone — and since **2.6.8** sigil is a folded stdlib module that simply tracks the toolchain pin (3.12.18 under 6.6.4), so there is no separate sigil version to hold or advance. Full story in [`dependency-watch.md § sigil`](dependency-watch.md).

### 6. Undefined symbols: **reachable = hard build error, unreachable = warning + runtime `ud2`**

The behavior has moved twice. cc5 made an undefined function a hard compile error; **cyrius 6.1.x** downgraded it to a `warning: undefined function '<name>'` with the call lowered to a `ud2` — the build succeeded and the program **SIGILLed (exit 132) the instant that call executed**. The current toolchain splits the two cases on reachability (verified empirically at 6.4.62, 6.4.83, 6.5.35 and 6.6.4 — this is *not* a 6.4.83 change; the entry below was simply stale):

- **Reachable call site** → `error: refusing to emit binary with N reachable undefined function(s) (pass --allow-undef to downgrade)`. **No binary is written.** Caught at build time again.
- **Unreachable call site** → a bare `warning: undefined function '<name>'` and the build succeeds (re-checked at 6.6.4: `tests/test_backends.tcyr` minus its `lib/sys.cyr` include prints exactly one `undefined function` line — the bare form — then `OK`). The `… (call site may be unreachable)` suffixed variant is printed in the *reachable* case, alongside the refusal — so grep for the bare prefix, not the suffix. The `ud2` is still there, so anything that makes the site reachable later turns into a SIGILL.

**Implication**: a missing `include` on a live path now fails the build, but one on a dormant path is still a latent runtime crash. After any toolchain/dep bump, audit every entry point's `undefined function` warnings (`cyrius build … 2>&1 | grep 'undefined function'`) and add the providing module rather than leaning on "it built, so it's fine." This is how the 2.4.5 migration surfaced `ct_eq` (→ `lib/ct.cyr`), the `http_*`→`sandhi_server_*` rename, and the mutex/`metrics_queue_*` include gaps — and how 2.7.3 surfaced `sys_uname`: sigil 3.12.18 calls it from `lib/sys.cyr`, `tests/test_backends.tcyr` never reaches `agnosys_uname`, so without `include "lib/sys.cyr"` before `lib/sigil.cyr` the build reported `OK`, warned, and carried a dormant `ud2`.

**Watch the driver, not just CI.** Reachability is computed over whatever lands in the compilation unit, and `cyrius bench` / `cyrius audit` inject the manifest `[deps].stdlib` list while `cyrius build --no-deps` does not. That asymmetry is exactly what hid the 2.5.2 bench breakage: CI (`--no-deps`) was green while `cyrius bench` refused to emit, because the injected `tls`/`sandhi` dragged in reachable `fdlopen_*` / `async_*` calls the bench entry point never included. **A green CI does not mean `cyrius audit` compiles.** The asymmetry cuts the other way too — a `src/` file reaching a stdlib module the entry point never includes builds under `audit`/`bench` and fails only under `--no-deps`, which is what CI and every consumer hand-including a bundle run; see quirk #7's *`--no-deps` prepends nothing*.

### 7. Cyrius 6.x splits stdlib (`lib sync`) from git deps (`deps`); build with `--no-deps`

`cyrius deps` no longer provisions the stdlib — it only resolves `[deps.*]` git deps, and **majra has declared none since 2.6.8**, so the step exists purely to write/verify `cyrius.lock`. The version-pinned stdlib snapshot (**110 `.cyr` files under 6.6.2–6.6.4** — 2.7.2 pinned 6.6.2 but never re-synced — 103 top-level plus 7 under `lib/unicode/`; was 108 under 6.5.31–6.5.36, 99 under 6.4.62–6.4.83, 97 under 6.2.11, 88 under 6.1.35, 94 under 6.1.24; the count tracks the toolchain — including the toolchain-internal `slice`/`ct`/`chrono`/`async`/`dynlib`/`fdlopen`/`tls` that sigil/sandhi reach into, and `sigil` itself since the 6.5.x fold) is copied into `./lib/` by **`cyrius lib sync --full`**. Run `lib sync --full` *before* `deps`.

**`--full` is load-bearing since 6.4.x**: a bare `cyrius lib sync` copies only the modules named in `[deps].stdlib` plus their peers (52 files at the 6.6.4 pin; 49 at 6.5.35) and omits the other 58 — `slice`, `ct` and `keccak` among them, which sigil reaches into — so the reaching call sites hit quirk #6.

**The snapshot can also shadow a declared dep.** `lib sync --full` ships bundled copies of some git-resolvable deps (e.g. `sakshi`), and the subsequent `cyrius deps` overlay *overwrites* them from whatever tag is resolved — including a tag inherited from another dep's manifest. When that inherited tag is **older** than the snapshot's copy, the overlay silently downgrades and the only signal is `warning: ./lib/ shadows version-pinned … <dep> <old> (pinned: <new>)`. majra hit this with sakshi at 2.5.2 and countered it by declaring `[deps.sakshi]`
at the top level — a counter-move that was itself **retired at the 6.5.18 pin**,
because on a library that publishes bundles a git dep makes `distlib` drop the
module from the generated `.deps` sidecars (see the ⚠ at the top of
[`dependency-watch.md`](dependency-watch.md)). **The current remedy is to declare
nothing** and let the module ride the snapshot. Don't dismiss that warning as cosmetic.

A `./lib/` that exists fully **shadows** the version snapshot (no per-file fallback), so a partial `./lib/` — e.g. one `cyrius deps` populated without a preceding `lib sync --full` — is missing `slice.cyr` and friends, and the reaching call sites then hit quirk #6.

Build with **`cyrius build --no-deps`**: a plain `cyrius build` auto-runs `deps`, which re-resolves and perturbs the synced lib's include order enough to re-break the agnosys/slice resolution even when `slice.cyr` is present. Canonical sequence: `cyrius lib sync --full && cyrius deps && cyrius build --no-deps <src> <out>`.

**`--no-deps` prepends nothing.** A `--no-deps` build compiles exactly the entry point's `include` lines, in order. The manifest's `[deps].stdlib` list is **not consulted by that build** — it drives a bare `lib sync` (the 52-file subset above), a bare `deps` into an empty `./lib/`, a plain `cyrius build`, `cyrius audit` / `cyrius bench` (all of which prepend it) and the `distlib` sidecars; a `--no-deps` build alone ignores it — and there is no transitive pull: a stdlib module a `src/` file calls must be included by **every** entry point that reaches that file. 2.7.3's example: `src/envelope.cyr` moved its time sources onto `lib/chrono.cyr` (`clock_now_ns`, `clock_epoch_ns`, `sleep_ms`), which only one of the fourteen entry points reaching it (`tests/test_backends.tcyr`) included; the other thirteen — `src/main.cyr`, the three other `.tcyr` suites, the three fuzz harnesses, `benches/bench_all.bcyr`, `examples/managed_queue.cyr`, the four soaks — each gained `include "lib/chrono.cyr"` right after `lib/syscalls.cyr`. majra had already hit this wall at 2.6.5 (`undefined function 'clock_epoch_ns'` in CI only — a full local `lib sync` had `chrono.cyr` on disk — which is why `time_epoch_ns` was rolled locally then). The rule covers a folded module's own deps too: sigil 3.12.18 calls `sys_uname` from `lib/sys.cyr`, so `tests/test_backends.tcyr` includes `lib/sys.cyr` before `lib/sigil.cyr`, and `"sys"` joined `[deps].stdlib` so that `cyrius audit` / `cyrius bench` (which prepend the declared list) build clean; `distlib`'s compile-verify already infers `sys` into all four sidecars on its own. **Discipline when a `src/` file gains a stdlib dependency**: record it in the file header (`envelope.cyr`: *"Requires: lib/syscalls.cyr, lib/chrono.cyr"*), add the include to every entry point, and do not lean on quirk #6's reachable-undefined error to find the gaps — a dormant path builds and traps. A sidecar-provisioned consumer is unaffected (all four `dist/*.deps` have listed `chrono` since 2.7.1); one hand-including the bundle under `--no-deps` must add the include itself.

### 9. Syscall numbers are spelled x86_64 everywhere; aarch64 renumbers only a routed set (`ESYSXLAT`) and passes the rest through **as a different, valid syscall**

cyrius has one `syscall(n, …)` primitive and no per-arch number at the call site: every number in stdlib and package code is the **x86_64** one, and the aarch64 backend fixes them up at runtime with `ESYSXLAT` — a `cmp x8, #N` chain on the syscall-number register (so it applies identically to a literal and to a `var SYS_X = N` global) that rewrites the numbers it has rows for and passes **every other number through to `svc` verbatim**. 44 rows at 6.6.4 (38 Linux numbers plus six cyrius-private ≥ 1000 aliases; 6.6.4 added `fstat`/`lstat` 5 and 6). A number outside the set is not an error — it is whatever aarch64 calls that number: 91 (`fchmod`) is `capset`, 35 (`nanosleep`) is `unlinkat`, 53 (`socketpair`) is `fchmodat`, 63 (`uname`) is `read`, 318 (`getrandom`) is nothing (`-ENOSYS`). That is how 2.7.3's three shipped aarch64 defects happened — every `ipc_bind` failed, every envelope id was 0/0, the DAG backoff never slept — while x86_64 never noticed. Full account in `CHANGELOG.md` § [2.7.3] and [`issues/archived/2026-09-14-raw-x86-syscall-numbers-aarch64.md`](issues/archived/2026-09-14-raw-x86-syscall-numbers-aarch64.md); the same class is what cyrius 6.6.4 swept from its own stdlib.

**Almost nothing warns.** The only build-time signal is a *name* collision: `var SYS_GETRANDOM = 318` collided with the aarch64 peer's `SYS_GETRANDOM = 278` (`duplicate symbol … redefined with conflicting value (last definition wins)`) — and because a dep bundle is prepended *after* the stdlib leaves, majra's 318 also overrode the peer's for the consumer's whole translation unit. An underscore-prefixed `var _SYS_FCHMOD` collides with nothing, and a raw literal is diagnosed only when it has a row in the generated xlat table — which keeps a row only for a name *both* Linux peers declare **and** drops any x86_64 number that is itself a valid native aarch64 syscall (cyrius `programs/gen_syscall_xlat.cyr`, `_row_wanted`). `nanosleep` fails the first test (neither peer declares it) and 35 is the aarch64 peer's `SYS_UNLINKAT` regardless; `socketpair` is declared by both peers, but 53 is that peer's `SYS_FCHMODAT`, so it goes silent too. cyrius's own `raw_syscall_literals_routed` gate scans its `lib/` + `cbt/`, not packages. Two of the three defects were silent on every toolchain.

**How majra spells a syscall since 2.7.3** (no `syscall(<literal>` survives in `src/`, `tests/`, `fuzz/`, `benches/` or `examples/` outside comments):

- **Call the per-target wrapper.** `sys_fchmod`, `sys_getrandom`, `sys_stat`, chrono's `clock_now_ns` / `clock_epoch_ns` / `sleep_ms` — each peer spells its own number behind the same name and arity, so the agnos/non-agnos `#ifdef` splits that used to guard the raw numbers are gone too. majra's one sleep primitive is `_majra_sleep_ns` (`src/envelope.cyr`): chrono `sleep_ms` to a monotonic deadline, each request clamped to 1 ms … 1 s (`poll(…, 0)` returns at once, a negative timeout is an infinite wait, and x86_64 `poll` takes a C `int`), and it terminates even when the clock does not advance — its header comment says why.
- **A target-neutral `var _SYS_X = <x86_64 number>` is allowed only when no wrapper exists on every target *and* the number is a routed row.** `src/ipc.cyr`'s networking numbers — 41/42/43/49/50 → 198/203/202/200/201, plus `_SYS_SHUTDOWN` 48 → 210 since 2.8.1 — are the whole allowed set (an unrouted call such as sendto is spelled per arch under `#ifdef`, as `_IPC_SYS_SENDTO` 44/206; the CI gate allows exactly these): the agnos and Windows peers do not declare `SYS_SOCKET` etc., so spelling the peers' names would break those builds. `src/redis_backend.cyr` and `src/postgres_backend.cyr` reuse `_SYS_SOCKET` / `_SYS_CONNECT` from `ipc.cyr` (earlier in every include order) rather than redeclaring. Never name one `SYS_*` — that is the peers' namespace, and the collision overrides theirs.
- **Test code uses the peer-declared name** where both Linux peers have it (`SYS_SOCKETPAIR`).
- **Verify by running, not building.** `cyrius build --aarch64 --no-deps <entry> build/<name>_aarch64 && qemu-aarch64 ./build/<name>_aarch64` — recipe in [`testing.md`](../guides/testing.md). A build-only aarch64 lane (CI has one since 2.7.3, beside a raw-literal grep gate) sees only the diagnosed cases — the pass-through class above is invisible to it; the run-under-qemu lane is still a roadmap item. Grep belt (what CI's gate runs): `grep -rn 'syscall([0-9]' src tests fuzz benches examples` should return only comments; a `^var SYS_` grep misses underscore-prefixed names, which is how the filing's first draft missed 91.

### 10. `struct stat` is laid out per peer — `STAT_MODE` is 24 on x86_64-Linux and 16 on aarch64-Linux; use the `Stat` enum

The two Linux peers fill different `struct stat` shapes (x86_64's legacy layout against the aarch64 generic one — the aarch64 peer routes bare `stat` to `newfstatat`), and the macOS-arm64 arm is different again (`st_mode` at 4, `st_size` at 96). Each peer declares its offsets as `enum Stat` — `STAT_MODE`, `STAT_NLINK`, `STAT_UID`, `STAT_GID`, `STAT_SIZE`, `STAT_MTIME`, `STAT_MTIME_NSEC`, `STAT_BUFSZ` — and across the two Linux peers only `STAT_SIZE` (48), `STAT_MTIME` (88), `STAT_MTIME_NSEC` (96) and `STAT_BUFSZ` (144) agree. A hardcoded `load32(&st + 24)` reads **`st_uid`** on aarch64: `tests/test_backends.tcyr`'s `ipc_bind` mode check did exactly that through 2.7.2 (`syscall(4, path, &st)` + `+ 24`) and is `sys_stat(path, &st)` + `load32(&st + STAT_MODE)` since 2.7.3. Size the buffer as `var st[144]` (a local is N *bytes* — quirk #4). No `src/` module reads a stat buffer today; this is a test-and-consumer rule.

### 11. `qemu-aarch64` runs cold — the first call into a code path costs ~1 ms, so a 1 ms timing window in a test is not portable

qemu-user translates each basic block on first execution (TCG), so the first call into any non-trivial path — a hashmap probe, an allocator refill — pays a translation cost native never sees. Measured at 2.7.3: `test_ratelimit`'s four `ratelimit_check` calls take ~14 µs natively; under `qemu-aarch64` the **first** alone costs ~1.2 ms — wider than the 1 ms refill window the core ratelimit tests were built around, which is why they run on a one-second window now (`CHANGELOG.md` § [2.7.3]).

**Rule**: a test that asserts something did *not* happen inside a window budgets the window in whole seconds (or hundreds of milliseconds), never single-digit milliseconds; a test that asserts elapsed time asserts a **floor** (`>= 10 ms` for one DAG retry, `>= 5 ms` for `_majra_sleep_ns(5 ms)`), never a ceiling. Loop the suite under qemu before calling a timing assertion portable — and loop it anyway: seed-dependent crashes hide at ~1/16 (the 2.7.3 barrier defect, `CHANGELOG.md` § [2.7.3]). Recipes for both, including the `CYRIUS_SYMS=<file>` symbol map qemu's gdbstub needs (cyrius binaries carry no symbol table), are in [`testing.md`](../guides/testing.md).

---

## Resolved (archive — don't re-introduce workarounds)

These were live quirks in earlier majra cycles. Listed for archaeological context so a future agent reading old code or commit messages has the explanation.

- ~~`\r` escape sequence broken~~ — works since cc4.x. Don't hand-emit byte 13 with `store8(buf, 13)`.
- ~~Negative literals `-1`, `-N` broken~~ — work since 3.10.3. No need for `(0 - N)`.
- ~~Compound assignment `+=`, `-=`, `*=` broken~~ — work since 3.10.3.
- ~~Undefined functions silently produced NULL stubs~~ — became a compile-time error in cc5, **then reverted at cyrius 6.1.x to warn + runtime `ud2`**, now split on reachability (see active quirk #6). Net: still not a NULL stub, build-fatal only on a live path — audit the warnings.
- ~~256-initialized-global cap~~ — removed.
- ~~Fixup table cap at 8192~~ — raised to 16384 (cap still exists; see active quirk #2).
- ~~`map_get` after `map_set` corruption in deep call chains~~ — cc5 resolves.
- ~~`thread_create` + futex correctness bugs~~ — fixed via `_thread_spawn` clone trampoline in `lib/thread.cyr` (cyrius 5.4.10) + aarch64 SP-alignment (5.4.11). Multi-threaded `cbarrier_arrive_and_wait` works under 5.4.10+ (the 2.7.3 `cbarrier_arrive_and_wait` defect was majra's own wrong argument to `_cbarrier_do_arrive`, present since 2.0.0 and arch-independent — not the toolchain; `CHANGELOG.md` § [2.7.3]).
- ~~Str-keyed hashmaps colliding under `map_new()`~~ — use `map_new_str()`; see active quirk #3 for the working pattern.
- ~~**#8** `fl_alloc` is NOT thread-safe; `alloc` is~~ — **resolved upstream at cyrius 6.5.19**: `lib/freelist.cyr` gained `_fl_lock`, a `_threads_active`-gated process-wide CAS spinlock on the free-list splice, arena bump and refill (its header now says so and credits majra's 2026-08-10 filing). Through 6.5.18 two threads racing one size class could be handed the **same block**, and CLAUDE.md's "`fl_alloc` for structs" rule put every majra struct on that allocator: at 2.5.3 it cost `pubsub_subscribe` orphaned channels (1–7 per 800 concurrent subscribes — a `chan_recv` that blocks forever) and `mq_enqueue` 4–12 lost jobs per 800. majra keeps the lock-before-`fl_alloc` ordering regardless (`src/pubsub.cyr` says why: it also covers the `_mq_next_job_id` read-modify-write). Archived by the post-2.7.0 doc sweep (2443b6a, 2026-08-22; first tagged in 2.7.1 — the 2.7.0 tag itself still lists this as active #8), whose commit dropped the archive section along with it; restored at 2.7.3.

When the cyrius pin moves and an active quirk resolves, strikethrough-and-move it down here with the resolving version. Don't delete — the historical record is useful when the next consumer wonders "wait, doesn't X break?"
