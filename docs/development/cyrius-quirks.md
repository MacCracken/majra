---
name: Cyrius compiler quirks
description: Toolchain-side gotchas that affect how majra code is written. Refresh as cyrius evolves; archive (don't delete) resolved entries.
---

# Cyrius compiler quirks

> **Toolchain floor**: cyrius 6.1.x (see [`state.md`](state.md) for the current exact pin) | **Refresh cadence**: when the pin moves or a new quirk surfaces.

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

### 4. `var buf[N]` inside fns is static, not stack

Function-scoped `var buf[256]` looks like a stack allocation but is emitted as static data — consecutive calls share the backing memory. Any `Str` or pointer returned from the fn that borrows into `buf` is invalidated by the next call.

**Pattern**: heap-allocate (`fl_alloc(N)` or `alloc(N)`) anything you intend to return out of the fn. Use `var buf[N]` only for true scratch consumed before the next call could happen.

Also why `var buf[256000]` bloats the binary by 256 KB — it lives in the data segment.

### 5. Inline-asm parameter loads were fragile pre-6.x — `param_load` pseudo fixes it

`mov rdi, [rbp-8]` -style byte-literal parameter loads inside `asm {}` blocks were tied to whatever stack-frame layout cc5 emitted; 5.10.x's expanded prologue shifted the slots and asm written against the old layout SIGILL'd. **cyrius 6.0.67+ exposes a `param_load(reg, idx)` asm pseudo** that resolves to the correct slot regardless of prologue shape, so this class is fixed at the toolchain.

**Implication for new majra code**: we still don't write inline asm, but if we ever need a hardware-acceleration hot path, use `param_load` rather than decoding `[rbp-N]` by hand.

**Implication for our deps**: this is why sigil was held at 2.9.0 through the 2.4.x line. Since 2.4.5 (cyrius 6.x) sigil's NI dispatch uses `param_load`, so we track **latest (3.7.10 as of 2.4.6)**. Full story in [`dependency-watch.md § sigil`](dependency-watch.md).

### 6. Undefined symbols compile to a runtime `ud2`, not a build error (cyrius 6.1.x)

**This reverses the cc5-era behavior** (an undefined function used to be a hard compile error — see the archived entry below). Under cyrius 6.1.x the compiler only emits a `warning: undefined function '<name>'` and lowers the call to a `ud2` instruction. The build *succeeds*; the program then **SIGILLs (exit 132) the instant that call executes**. Under gdb it looks like an asm fault until you notice the faulting instruction is `ud2`.

**Implication**: a missing `include` is now a latent runtime crash, not a caught-at-build mistake. After any toolchain/dep bump, **audit every entry point's reachable `undefined function` warnings** (`cyrius build … 2>&1 | grep 'undefined function' | grep -v 'may be unreachable'`) and add the providing module. This is how the 2.4.5 migration surfaced `ct_eq` (→ `lib/ct.cyr`), the `http_*`→`sandhi_server_*` rename, and the mutex/`metrics_queue_*` include gaps.

### 7. Cyrius 6.x splits stdlib (`lib sync`) from git deps (`deps`); build with `--no-deps`

`cyrius deps` no longer provisions the stdlib — it only resolves `[deps.*]` git deps. The version-pinned stdlib snapshot (97 `.cyr` files under 6.2.11 — was 88 under 6.1.35, 94 under 6.1.24; the count tracks the toolchain — including the toolchain-internal `slice`/`ct`/`chrono`/`async`/`dynlib`/`fdlopen`/`tls` that agnosys/sandhi reach into) is copied into `./lib/` by **`cyrius lib sync`**. Run `lib sync` *before* `deps`.

A `./lib/` that exists fully **shadows** the version snapshot (no per-file fallback), so a partial `./lib/` — e.g. one `cyrius deps` populated without a preceding `lib sync` — is missing `slice.cyr`, and agnosys 1.3.2's slice subscripts then hit quirk #6's `ud2`.

Build with **`cyrius build --no-deps`**: a plain `cyrius build` auto-runs `deps`, which re-resolves and perturbs the synced lib's include order enough to re-break the agnosys/slice resolution even when `slice.cyr` is present. Canonical sequence: `cyrius lib sync && cyrius deps && cyrius build --no-deps <src> <out>`.

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
