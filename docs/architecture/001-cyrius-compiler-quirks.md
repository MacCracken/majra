# 001 — Cyrius compiler quirks (5.10.x floor)

> **Status**: Active | **Last refresh**: 2026-05-10 | **Affects**: all modules

Invariants and gotchas of the cyrius compiler that majra's code currently depends on or defends against. These are facts about the toolchain, not decisions we made. The list shortens over time as cc5 line improvements land; resolved entries get a strikethrough + version, not a deletion.

For *why* something is the way it is, see commit log or `CHANGELOG.md`. For *what* the convention is across the repo, see `CLAUDE.md`. This file is the *what's quietly true about the compiler* layer.

---

## 1. Local variable clobbering across deep call chains

Still possible under cc5 at the 5.10.x line, though much rarer than cc3 / early-cc5. Symptom: a local's value looks wrong after returning from a nested call.

**Workaround**: promote the local to a module-level global. The pattern is well-trodden — sigil uses it heavily for AES / SHA / ed25519 state; majra uses it sparingly because most call chains aren't deep enough to trip it.

If you suspect a fresh instance: print the value before and after the suspect call, then promote-to-global if the values disagree.

---

## 2. Freelist vs bump allocator discipline

Two allocators, different semantics:

- **`fl_alloc(n)` / `fl_free(p)`** — freelist, supports individual free. Use for structs with explicit lifetimes (envelopes, jobs, signed envelopes, queue entries).
- **`alloc(n)`** — bump allocator. No free. Use for long-lived collections (hashmaps, vec internals, anything that lives until process exit).

Mixing is allowed and correct either way, but mix thoughtfully. The conventional shape: `fl_alloc` for things you'd `fl_free` later; `alloc` for things you'd never free.

---

## 3. Single-pass compiler — fixup-table cap 16384

Forward references across function boundaries work via a fixup table, capacity 16384 entries (raised from 8192 in cc3). Module include order still matters for type / struct visibility — a struct must be declared before its first use, even if the use site is in a fn that's called later.

**Practical implication**: very large test entry points can blow the cap. `tests/test_patra_queue.tcyr` lives in its own entry point because adding it to `tests/test_backends.tcyr` exceeded 16384.

**If a fixup-cap error fires**: split the entry point, don't try to reorder.

---

## 4. Hashmap keys — `map_new()` is cstr, `map_new_str()` is Str

cyrius 5.4.14+ added `map_new_str()` + content-derived `hash_str_v` to fix a Str-struct key collision bug majra originally surfaced via soak tests (~3% collision rate against the cstr-shaped `hash_str`).

- **`map_new()`** for cstr keys (the legacy / default path).
- **`map_new_str()`** for Str-struct keys.

`src/queue.cyr` uses `map_new_str()` (its managed-queue job map is keyed on `str_from_int(id)`). Other modules keyed on cstrs keep `map_new()`.

Picking the wrong one compiles cleanly but corrupts at runtime — silent hash collisions in the wrong direction.

---

## 5. Stack-frame layout drifts between cyrius minor lines

Inline asm blocks that hardcode `[rbp-N]` parameter offsets break when cyrius bumps its prologue. Surfaced during the 2.4.2 toolchain bump (5.4.17 → 5.10.34): sigil 2.9.1+ AES-NI / SHA-NI / ed25519-NI dispatch fns SIGILL on 5.10.x because their `mov rdi, [rbp-8]` / `[rbp-16]` / `[rbp-24]` byte literals were written against the pre-5.5 frame.

**Implication for majra**: we pin sigil at 2.9.0 (asm-free reference paths) until upstream emits asm that survives a cyrius minor bump. Tracked at [sigil P1 in the v3.1 arc](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md).

**Implication for new majra code**: if you ever write inline asm (we don't today), don't decode parameters via `[rbp-N]` byte literals. Load via module-level globals before the asm block.

---

## 6. `var buf[N]` inside fns is static, not stack

Function-scoped `var buf[256]` looks like a stack allocation but is emitted as static data — consecutive calls share the backing memory. Any `Str` / pointer returned from the fn that borrows into `buf` is invalidated by the next call.

**Pattern**: heap-allocate (`fl_alloc(N)` or `alloc(N)`) anything you intend to return out of the fn. Use `var buf[N]` only for true scratch that's consumed before the next call could happen.

Subtle: this is also the reason `var buf[256000]` bloats the binary by 256 KB — it's static data, not a stack alloc, so it lives in the data segment.

---

## Resolved in cc5 (historical, kept here so the "wait, doesn't X break?" question has an answer)

These were live quirks in earlier majra cycles and aren't anymore. Listed for archaeological context; don't re-introduce workarounds for them.

- ~~`\r` escape sequence~~ — works since cc4.x. Don't hand-emit byte 13 with `store8(buf, 13)`.
- ~~Negative literals `-1`, `-N`~~ — work since 3.10.3. No need for `(0 - N)`.
- ~~Compound assignment `+=`, `-=`, `*=`~~ — work since 3.10.3.
- ~~Undefined functions silently produce NULL stubs~~ — now a compile-time error.
- ~~256-initialized-global cap~~ — removed.
- ~~Fixup table cap at 8192~~ — raised to 16384 (cap still exists; see quirk #3).
- ~~`map_get` after `map_set` corruption in deep call chains~~ — cc5 resolves.
- ~~`thread_create` + futex correctness~~ — fixed via `_thread_spawn` clone trampoline in `lib/thread.cyr` (cyrius 5.4.10) + aarch64 SP-alignment (5.4.11). Multi-threaded `cbarrier_arrive_and_wait` works under cc5 5.4.10+.
- ~~Str-keyed hashmaps colliding under `map_new()`~~ — see quirk #4 for the working pattern.

When the cyrius pin moves and a current quirk gets resolved, move it down here with a `~~strikethrough~~` and the resolving version. Don't delete — the historical record is useful when the next consumer wonders "wait, doesn't X break?"
