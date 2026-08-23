---
name: Cyrius compiler quirks
description: Toolchain-side gotchas that affect how majra code is written. Refresh as cyrius evolves; archive (don't delete) resolved entries.
---

# Cyrius compiler quirks

> **Toolchain floor**: cyrius 6.1.x (see [`state.md`](state.md) for the current exact pin) | **Refresh cadence**: when the pin moves or a new quirk surfaces. | **Last verified**: 6.5.35 (2.6.8) — the undefined-fn `ud2` rule re-confirmed empirically against a clean-room consumer build (it is what made the missing-`sigil` sidecar a runtime SIGILL rather than a build failure); snapshot count and `lib sync --full` note refreshed. **Prior**: 6.4.83 (2.5.2) — quirks #4 and #6 re-checked; #6 rewritten.

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

### 2. Single-pass compiler — fixup-table cap 16384

Forward references across function boundaries work via a fixup table, capacity 16384 entries (up from 8192 in cc3). Module include order still matters for type / struct visibility — a struct must be declared before its first use even if the use site is in a fn called later.

**Practical implication**: very large test entry points can blow the cap. `tests/test_patra_queue.tcyr` lives in its own entry point because adding it to `tests/test_backends.tcyr` exceeded 16384.

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

**Implication for our deps**: this is why sigil was held at 2.9.0 through the 2.4.x line. Since 2.4.5 (cyrius 6.x) sigil's NI dispatch uses `param_load`, so the constraint is gone — and since **2.6.8** sigil is a folded stdlib module that simply tracks the toolchain pin (3.12.9 under 6.5.35), so there is no separate sigil version to hold or advance. Full story in [`dependency-watch.md § sigil`](dependency-watch.md).

### 6. Undefined symbols: **reachable = hard build error, unreachable = warning + runtime `ud2`**

The behavior has moved twice. cc5 made an undefined function a hard compile error; **cyrius 6.1.x** downgraded it to a `warning: undefined function '<name>'` with the call lowered to a `ud2` — the build succeeded and the program **SIGILLed (exit 132) the instant that call executed**. The current toolchain splits the two cases on reachability (verified empirically at 6.4.62, 6.4.83 and 6.5.35 — this is *not* a 6.4.83 change; the entry below was simply stale):

- **Reachable call site** → `error: refusing to emit binary with N reachable undefined function(s) (pass --allow-undef to downgrade)`. **No binary is written.** Caught at build time again.
- **Unreachable call site** → `warning: undefined function '<name>' (call site may be unreachable)` and the build succeeds. The `ud2` is still there, so anything that makes the site reachable later turns into a SIGILL.

**Implication**: a missing `include` on a live path now fails the build, but one on a dormant path is still a latent runtime crash. After any toolchain/dep bump, audit every entry point's `undefined function` warnings (`cyrius build … 2>&1 | grep 'undefined function'`) and add the providing module rather than leaning on "it built, so it's fine." This is how the 2.4.5 migration surfaced `ct_eq` (→ `lib/ct.cyr`), the `http_*`→`sandhi_server_*` rename, and the mutex/`metrics_queue_*` include gaps.

**Watch the driver, not just CI.** Reachability is computed over whatever lands in the compilation unit, and `cyrius bench` / `cyrius audit` inject the manifest `[deps].stdlib` list while `cyrius build --no-deps` does not. That asymmetry is exactly what hid the 2.5.2 bench breakage: CI (`--no-deps`) was green while `cyrius bench` refused to emit, because the injected `tls`/`sandhi` dragged in reachable `fdlopen_*` / `async_*` calls the bench entry point never included. **A green CI does not mean `cyrius audit` compiles.**

### 7. Cyrius 6.x splits stdlib (`lib sync`) from git deps (`deps`); build with `--no-deps`

`cyrius deps` no longer provisions the stdlib — it only resolves `[deps.*]` git deps, and **majra has declared none since 2.6.8**, so the step exists purely to write/verify `cyrius.lock`. The version-pinned stdlib snapshot (**108 `.cyr` files under 6.5.31 and 6.5.35** — was 99 under 6.4.62–6.4.83, 97 under 6.2.11, 88 under 6.1.35, 94 under 6.1.24; the count tracks the toolchain — including the toolchain-internal `slice`/`ct`/`chrono`/`async`/`dynlib`/`fdlopen`/`tls` that sigil/sandhi reach into, and `sigil` itself since the 6.5.x fold) is copied into `./lib/` by **`cyrius lib sync --full`**. Run `lib sync --full` *before* `deps`.

**`--full` is load-bearing since 6.4.x**: a bare `cyrius lib sync` copies only the modules named in `[deps].stdlib` (40 files) and omits exactly the toolchain-internal set above — which then hits quirk #6.

**The snapshot can also shadow a declared dep.** `lib sync --full` ships bundled copies of some git-resolvable deps (e.g. `sakshi`), and the subsequent `cyrius deps` overlay *overwrites* them from whatever tag is resolved — including a tag inherited from another dep's manifest. When that inherited tag is **older** than the snapshot's copy, the overlay silently downgrades and the only signal is `warning: ./lib/ shadows version-pinned … <dep> <old> (pinned: <new>)`. majra hit this with sakshi at 2.5.2 and fixed it by declaring `[deps.sakshi]` at the top level. Don't dismiss that warning as cosmetic.

A `./lib/` that exists fully **shadows** the version snapshot (no per-file fallback), so a partial `./lib/` — e.g. one `cyrius deps` populated without a preceding `lib sync --full` — is missing `slice.cyr` and friends, and the reaching call sites then hit quirk #6.

Build with **`cyrius build --no-deps`**: a plain `cyrius build` auto-runs `deps`, which re-resolves and perturbs the synced lib's include order enough to re-break the agnosys/slice resolution even when `slice.cyr` is present. Canonical sequence: `cyrius lib sync --full && cyrius deps && cyrius build --no-deps <src> <out>`.

### 8. `fl_alloc` is NOT thread-safe; `alloc` is

The two stdlib allocators have different concurrency contracts, and neither
says so in its header:

- **`alloc` (lib/alloc.cyr) is safe.** It carries a documented process-wide CAS
  spinlock plus a `_threads_active` single-threaded fast path (`:28-48`).
- **`fl_alloc` (lib/freelist.cyr) is not.** It pops the size-class free list
  with a plain load/store pair — `head = load64(&_fl_heads + cls*8);
  store64(&_fl_heads + cls*8, load64(head))` — with no lock, no CAS, no gate.
  Two threads racing the same size class can be handed **the same block**.

**This is load-bearing for majra**, because CLAUDE.md's own rule is "`fl_alloc`
for structs, `alloc` for hashmaps" — so every majra struct comes from the
*unsynchronized* allocator. At 2.5.3 this cost two silent data-loss bugs:
`pubsub_subscribe` handed callers channels that were never registered (their
`chan_recv` blocked forever), and `mq_enqueue` lost 4-12 jobs per 800 because
two enqueues got the same block *and* the same job key.

**The rule**: in any function reachable from more than one thread, take the
object's mutex **before** `fl_alloc`, not after. Allocating outside the lock to
"keep the critical section short" is exactly the bug. Note that `chan_new` and
`mutex_new` are safe (they use `alloc`), so a pre-lock `chan_new` is *not*
evidence of this bug — check which allocator actually runs.

**Diagnosis**: N threads × M operations, then assert the observable count
equals N×M and that no two returned pointers alias. The failure rate is low
(0.1-1.5%) and entirely silent, so single-run tests will pass. Loop it.

Upstream: `lib/freelist.cyr` gaining `lib/alloc.cyr`'s spinlock would dissolve
the class. Until then majra defends itself with lock placement. Re-check this
entry whenever the cyrius pin moves — verified present at **6.5.35**.

---

## Resolved in cc5 (archive — don't re-introduce workarounds)

These were live quirks in earlier majra cycles. Listed for archaeological context so a future agent reading old code or commit messages has the explanation.

- ~~`\r` escape sequence broken~~ — works since cc4.x. Don't hand-emit byte 13 with `store8(buf, 13)`.
- ~~Negative literals `-1`, `-N` broken~~ — work since 3.10.3. No need for `(0 - N)`.
- ~~Compound assignment `+=`, `-=`, `*=` broken~~ — work since 3.10.3.
- ~~Undefined functions silently produced NULL stubs~~ — became a compile-time error in cc5, **then reverted at cyrius 6.1.x to warn + runtime `ud2`** (see active quirk #6). Net: still not a NULL stub, but no longer build-fatal either — audit the warnings.
- ~~256-initialized-global cap~~ — removed.
- ~~Fixup table cap at 8192~~ — raised to 16384 (cap still exists; see active quirk #2).
- ~~`map_get` after `map_set` corruption in deep call chains~~ — cc5 resolves.
- ~~`thread_create` + futex correctness bugs~~ — fixed via `_thread_spawn` clone trampoline in `lib/thread.cyr` (cyrius 5.4.10) + aarch64 SP-alignment (5.4.11). Multi-threaded `cbarrier_arrive_and_wait` works under 5.4.10+.
- ~~Str-keyed hashmaps colliding under `map_new()`~~ — use `map_new_str()`; see active quirk #3 for the working pattern.

When the cyrius pin moves and an active quirk resolves, strikethrough-and-move it down here with the resolving version. Don't delete — the historical record is useful when the next consumer wonders "wait, doesn't X break?"
