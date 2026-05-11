---
name: Cyrius compiler quirks
description: Toolchain-side gotchas that affect how majra code is written. Refresh as cyrius evolves; archive (don't delete) resolved entries.
---

# Cyrius compiler quirks

> **Toolchain floor**: cyrius 5.10.x (see [`state.md`](state.md) for the current exact pin) | **Refresh cadence**: when the pin moves or a new quirk surfaces.

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

### 5. Inline-asm parameter loads are fragile across cyrius minor lines

`mov rdi, [rbp-8]` -style byte-literal parameter loads inside `asm {}` blocks are tied to whatever stack-frame layout cc5 emits at the time. cyrius 5.10.x's expanded prologue shifts the parameter slots relative to pre-5.5; asm written against the old layout reads garbage and the next instruction faults with SIGILL.

**Implication for new majra code**: we don't write inline asm today, and probably shouldn't until cyrius exposes a stable asm parameter ABI. If we ever need a hardware-acceleration hot path, load parameters into module-level globals *before* the asm block instead of decoding `[rbp-N]`.

**Implication for our deps**: this is why we hold sigil at 2.9.0 — 2.9.1+ has NI dispatch fns that hit this. Full story in [`dependency-watch.md § sigil`](dependency-watch.md).

---

## Resolved in cc5 (archive — don't re-introduce workarounds)

These were live quirks in earlier majra cycles. Listed for archaeological context so a future agent reading old code or commit messages has the explanation.

- ~~`\r` escape sequence broken~~ — works since cc4.x. Don't hand-emit byte 13 with `store8(buf, 13)`.
- ~~Negative literals `-1`, `-N` broken~~ — work since 3.10.3. No need for `(0 - N)`.
- ~~Compound assignment `+=`, `-=`, `*=` broken~~ — work since 3.10.3.
- ~~Undefined functions silently produced NULL stubs~~ — now a compile-time error.
- ~~256-initialized-global cap~~ — removed.
- ~~Fixup table cap at 8192~~ — raised to 16384 (cap still exists; see active quirk #2).
- ~~`map_get` after `map_set` corruption in deep call chains~~ — cc5 resolves.
- ~~`thread_create` + futex correctness bugs~~ — fixed via `_thread_spawn` clone trampoline in `lib/thread.cyr` (cyrius 5.4.10) + aarch64 SP-alignment (5.4.11). Multi-threaded `cbarrier_arrive_and_wait` works under 5.4.10+.
- ~~Str-keyed hashmaps colliding under `map_new()`~~ — use `map_new_str()`; see active quirk #3 for the working pattern.

When the cyrius pin moves and an active quirk resolves, strikethrough-and-move it down here with the resolving version. Don't delete — the historical record is useful when the next consumer wonders "wait, doesn't X break?"
