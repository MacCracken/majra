---
name: Cyrius compiler quirks
description: Toolchain-side gotchas that affect how majra code is written. Refresh as cyrius evolves; archive (don't delete) resolved entries.
---

# Cyrius compiler quirks

> **Toolchain floor**: cyrius 6.4.65 (the folded sigil needs `thread_local_alloc`) — and 6.5.19 for a thread-safe `fl_alloc`, see the archive. Current exact pin in [`state.md`](state.md) | **Refresh cadence**: when the pin moves or a new quirk surfaces. | **Last verified**: 6.5.35 (2.7.0) — quirk #8 re-checked against `lib/freelist.cyr` and **archived**: upstream locked it at 6.5.19 (`_fl_lock`, a `_threads_active`-gated CAS spinlock). Bare-`lib sync` file count re-measured at 49. **Prior**: 6.5.35 (2.6.8) — the undefined-fn `ud2` rule re-confirmed empirically against a clean-room consumer build (it is what made the missing-`sigil` sidecar a runtime SIGILL rather than a build failure); snapshot count and `lib sync --full` note refreshed. **Prior**: 6.4.83 (2.5.2) — quirks #4 and #6 re-checked; #6 rewritten.

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

**`--full` is load-bearing since 6.4.x**: a bare `cyrius lib sync` copies only the modules named in `[deps].stdlib` (49 files at the 6.5.35 pin) and omits exactly the toolchain-internal set above — which then hits quirk #6.

**The snapshot can also shadow a declared dep.** `lib sync --full` ships bundled copies of some git-resolvable deps (e.g. `sakshi`), and the subsequent `cyrius deps` overlay *overwrites* them from whatever tag is resolved — including a tag inherited from another dep's manifest. When that inherited tag is **older** than the snapshot's copy, the overlay silently downgrades and the only signal is `warning: ./lib/ shadows version-pinned … <dep> <old> (pinned: <new>)`. majra hit this with sakshi at 2.5.2 and countered it by declaring `[deps.sakshi]`
at the top level — a counter-move that was itself **retired at the 6.5.18 pin**,
because on a library that publishes bundles a git dep makes `distlib` drop the
module from the generated `.deps` sidecars (see the ⚠ at the top of
[`dependency-watch.md`](dependency-watch.md)). **The current remedy is to declare
nothing** and let the module ride the snapshot. Don't dismiss that warning as cosmetic.

A `./lib/` that exists fully **shadows** the version snapshot (no per-file fallback), so a partial `./lib/` — e.g. one `cyrius deps` populated without a preceding `lib sync --full` — is missing `slice.cyr` and friends, and the reaching call sites then hit quirk #6.

Build with **`cyrius build --no-deps`**: a plain `cyrius build` auto-runs `deps`, which re-resolves and perturbs the synced lib's include order enough to re-break the agnosys/slice resolution even when `slice.cyr` is present. Canonical sequence: `cyrius lib sync --full && cyrius deps && cyrius build --no-deps <src> <out>`.

