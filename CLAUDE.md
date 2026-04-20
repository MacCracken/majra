# Majra — Claude Code Instructions

> **READ the distribution section before touching build or release.**
> majra ships to downstream Cyrius projects via four committed
> bundles produced by `cyrius distlib`:
>   - `dist/majra.cyr`          — core engine (15 modules)
>   - `dist/majra-signed.cyr`   — core + Ed25519-signed envelopes
>   - `dist/majra-admin.cyr`    — core + HTTP admin/metrics endpoint
>   - `dist/majra-backends.cyr` — everything (adds redis/pg/ws/encrypted-IPC/patra_queue)
> That is the only distribution contract. Libro and patra are the
> references. Do not invent alternatives.

## Project Identity

**Majra** (Arabic: channel/waterway) — Distributed queue and multiplex engine — pub/sub, queues, relay, IPC, heartbeat, rate limiting

- **Type**: Cyrius library (single-file compilation via `include`)
- **License**: GPL-3.0-only
- **Version**: See `VERSION` and `cyrius.cyml`
- **Language**: [Cyrius](https://github.com/MacCracken/cyrius) (pin in `cyrius.cyml` `cyrius = "..."` field)
- **Genesis repo**: [agnosticos](https://github.com/MacCracken/agnosticos)
- **Philosophy**: [AGNOS Philosophy & Intention](https://github.com/MacCracken/agnosticos/blob/main/docs/philosophy.md)
- **Standards**: [First-Party Standards](https://github.com/MacCracken/agnosticos/blob/main/docs/development/applications/first-party-standards.md)
- **Recipes**: [zugot](https://github.com/MacCracken/zugot) — takumi build recipes

## Consumers

daimon (agent messaging), AgnosAI (crew coordination), hoosh (inference routing), sutra (fleet), stiva (container IPC)

## Dependencies

- **Cyrius stdlib** — vendored under `lib/`. The `[deps] stdlib = [...]`
  list in `cyrius.cyml` mirrors the `include "lib/<name>.cyr"` lines at
  the top of `src/main.cyr`. Keep them in sync.
- **sigil ≥ 2.9.0** — first-party crypto library, vendored at `lib/sigil.cyr`
  via `[deps.sigil]`. Used by `src/ipc_encrypted.cyr` (AES-256-GCM) and
  `src/signed_envelope.cyr` (Ed25519). Consumer projects picking the
  core `dist/majra.cyr` profile don't need sigil; the `signed`, `admin`
  (no — admin doesn't), and `backends` profiles do.

If another external first-party dep is added, wire it under
`[deps.<name>]` with `git` + `tag` + `modules` like we wire sigil.

## Build & Test

```bash
# Compile (entry + output come from cyrius.cyml [build])
cyrius build src/main.cyr build/majra

# Full test matrix (150 + 96 + 42 + 17 = 305 assertions)
./build/majra                                                                      # core
cyrius build tests/test_core.tcyr        build/test_core        && ./build/test_core
cyrius build tests/test_backends.tcyr    build/test_backends    && ./build/test_backends
cyrius build tests/test_patra_queue.tcyr build/test_patra_queue && ./build/test_patra_queue

# Live integration (requires Redis + PostgreSQL running locally)
cyrius build tests/test_live.tcyr build/test_live && ./build/test_live

# Benchmarks (17 targets, history tracked)
cyrius build benches/bench_all.bcyr build/bench_all && ./build/bench_all

# Soak tests (on-demand, not in CI; slow)
cyrius build tests/soak/soak_queue.cyr build/soak_queue && ./build/soak_queue

# Fuzz harnesses
for f in fuzz/*.fcyr; do cyrius build "$f" "build/$(basename $f .fcyr)"; done

# Regenerate all four distribution bundles (commit alongside src/ changes)
cyrius distlib          # → dist/majra.cyr           (core engine)
cyrius distlib signed   # → dist/majra-signed.cyr    (+ signed_envelope)
cyrius distlib admin    # → dist/majra-admin.cyr     (+ admin endpoint)
cyrius distlib backends # → dist/majra-backends.cyr  (everything)

# Policy enforcement
cyrius deny src/main.cyr

# Full audit: self-host, test, fmt, lint, vet, deny, bench
cyrius audit
```

## Development Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Compile check: `cyrius build src/main.cyr build/majra`
3. Test: all suites must pass (`0 failed`)
4. Lint + format: `cyrius fmt --check`, `cyrius lint`
5. Policy check: `cyrius deny src/main.cyr`
6. Benchmark additions for new code
7. Run benchmarks: `cyrius bench` (tracks history automatically)
8. Audit phase — review performance, memory, security, correctness
9. Deeper tests/benchmarks from audit observations
10. Run benchmarks again — prove the wins
11. Regenerate all bundles: `cyrius distlib && cyrius distlib signed && cyrius distlib admin && cyrius distlib backends`
12. Full audit: `cyrius audit`
13. Documentation — update CHANGELOG, roadmap, docs
14. Version sync — `VERSION` is source of truth; `cyrius.cyml` uses `${file:VERSION}` (`scripts/version-bump.sh`)
15. Return to step 1

### Key Principles

- **Never skip benchmarks.** Numbers don't lie.
- **Tests + benchmarks are the way.** 305 assertions across 4 suites (150 core + 96 expanded + 42 backends + 17 patra_queue), plus 36 live-integration and on-demand soak targets under `tests/soak/`.
- **Own the stack.** Zero external dependencies for the core engine. The richer profiles (signed, admin, backends) pull sigil — first-party, vendored — as the only external surface.
- **No magic.** Every operation is measurable, auditable, traceable.
- **`fl_alloc` for structs, `alloc` for hashmaps.** Freelist supports individual free; bump allocator for long-lived collections.
- **Globals for cross-call state when cc5 clobbers locals.** cc5 is significantly better than cc3 at this, but if a local's value looks wrong after a nested call, promote it to a global.

## Project Structure

```
cyrius.cyml                Manifest ([package], [build], [lib], [lib.signed], [lib.admin], [lib.backends], [deps])
VERSION                    Source of truth for the version string
src/main.cyr               Entry point + core tests (150 assertions)
src/*.cyr                  22 modules across: core primitives, networking, composition,
                           backends (redis/pg/ipc_encrypted/ws/patra_queue), trust (signed_envelope),
                           operations (admin)
tests/test_core.tcyr       Expanded unit tests (96 assertions)
tests/test_backends.tcyr   Backend protocol tests (42 assertions — redis/pg/ws/aes-gcm/signed_envelope/admin)
tests/test_patra_queue.tcyr Persistent queue tests (17 assertions — separate entry to stay under 16384 fixup cap)
tests/test_live.tcyr       Live Redis + PostgreSQL tests (36 assertions)
tests/soak/*.cyr           On-demand soak tests (not in CI). soak_queue ships; soak_pubsub/relay/heartbeat tbd
benches/bench_all.bcyr     17 benchmarks
examples/                  managed_queue.cyr, pubsub_tiers.cyr
fuzz/*.fcyr                Fuzz harnesses (heartbeat, pubsub, queue)
dist/majra.cyr             Consumer dist — core engine (default [lib])
dist/majra-signed.cyr      Consumer dist — core + signed envelopes ([lib.signed])
dist/majra-admin.cyr       Consumer dist — core + admin endpoint ([lib.admin])
dist/majra-backends.cyr    Consumer dist — everything ([lib.backends])
lib/                       Vendored Cyrius stdlib + sigil (2.9.0+)
build/                     Compiled binaries (gitignored)
scripts/version-bump.sh    Syncs VERSION
docs/                      architecture, development, guides
```

## Documentation Structure

```
Root files (required):
  README.md, CHANGELOG.md, CLAUDE.md, CONTRIBUTING.md,
  SECURITY.md, CODE_OF_CONDUCT.md, LICENSE, VERSION, cyrius.cyml

docs/ (required):
  architecture/overview.md — module map, data flow, consumers
  development/roadmap.md — completed, backlog, future

docs/ (when earned):
  guides/ — usage guides, integration patterns
  development/ — semver, threat model
  adr/ — architecture decision records
```

## CI / Release

- **Toolchain pin**: `cyrius` field inside `cyrius.cyml` (currently `cyrius = "5.4.17"`). CI and release extract it dynamically — no hardcoded version strings in YAML.
- **Manifest**: `cyrius.cyml` (migrated from `cyrius.toml` in 2.3.0 to match first-party convention). Uses `${file:VERSION}` so `VERSION` remains the single source of truth.
- **Manifest completeness gate**: CI asserts every `include "src/<file>.cyr"` in `src/main.cyr` is listed under `[lib] modules`. If not, `cyrius distlib` would silently ship a broken bundle.
- **Distribution freshness gate**: CI runs all four `cyrius distlib` invocations and fails if `git diff dist/` is non-empty. All four bundles must be regenerated and committed alongside any `src/` change.
- **Tag filter**: release triggers on `tags: ['[0-9]*']` — semver-only.
- **Version gate**: release asserts `VERSION == git tag` before building.
- **Docs gate**: CI verifies `VERSION` appears as `[x.y.z]` heading in `CHANGELOG.md`.

## Distribution Contract

majra is an **upstream Cyrius library**. Downstream projects (daimon,
AgnosAI, hoosh, sutra, stiva) wire it into their `cyrius.cyml` like:

```toml
[deps.majra]
git = "https://github.com/MacCracken/majra.git"
tag = "<majra version>"
modules = ["dist/majra.cyr"]           # core engine — lean, no crypto
# or a richer profile:
modules = ["dist/majra-signed.cyr"]    # core + signed envelopes (pulls sigil)
modules = ["dist/majra-admin.cyr"]     # core + admin endpoint
modules = ["dist/majra-backends.cyr"]  # everything (signed + admin + network backends)
```

`cyrius deps` clones at the tag and copies the chosen bundle into the
consumer's `lib/majra_majra.cyr`. Every tagged release MUST have ALL FOUR
bundles (`dist/majra.cyr`, `dist/majra-signed.cyr`, `dist/majra-admin.cyr`,
`dist/majra-backends.cyr`) committed, or consumer `cyrius deps` breaks at
pull time for whichever profile they chose.

## DO NOT

- **Do not commit or push** — the user handles all git operations (commit, push, tag)
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not hardcode the Cyrius version in CI YAML — read the `cyrius = "..."` field from `cyrius.cyml`
- Do not forget to regenerate `dist/` after any `src/` change
- Do not add dependencies beyond the Cyrius toolchain
- Do not skip benchmarks before claiming performance improvements
- Do not commit `build/`

## Known Cyrius Compiler Quirks (5.4.17)

Most cc3-era workarounds documented in earlier majra versions are now
resolved under cc5. Quirks still worth knowing:

1. **Local variable clobbering** — still possible across deeply nested call chains in cc5, though rarer than cc3. Not a guaranteed bug. If a local's value looks wrong after a function call, promote it to a global as a workaround.
2. **Freelist vs bump allocator discipline** — `fl_alloc` + `fl_free` for individually-freed structs; `alloc()` for long-lived collections. Correct either way, but mix them thoughtfully.
3. **Single-pass compiler** — forward references across function boundaries work via fixups (cap **16384** in 5.4.x, up from 8192 in cc3). Module include order still matters for type/struct visibility. The patra_queue test lives in its own entry point because adding it to test_backends blew the 16384 cap.
4. **`map_new()` keys are cstrs; use `map_new_str()` for Str keys** — cyrius 5.4.14+ added `map_new_str` + content-derived `hash_str_v` to fix the Str-struct key collision bug (majra surfaced this via soak tests). `src/queue.cyr` uses `map_new_str()`; other modules keyed on cstrs keep `map_new()`.
5. **Inline asm stores through caller output pointers no-op across `include` boundaries** — confirmed via sigil 2.9.0's AES-NI work (scheduled for fix in cyrius 5.5.x; see `cyrius/docs/development/issues/inline-asm-stores-silently-drop-when-fn-included.md`). Avoid `asm {}` blocks that write to caller-supplied pointers from an include'd fn until the fix lands.

### Resolved under cc5 (stop treating as bugs)

- **`\r` escape sequence** — works since 4.x. Don't hand-emit byte 13 with `store8(buf, 13)`. The old `# --- Raw bytes for CR LF` workaround across network protocols is obsolete.
- **Negative literals `-1`, `-N`** — work since 3.10.3. No need for `(0 - N)`.
- **Compound assignment `+=`, `-=`, `*=`, etc.** — work since 3.10.3.
- **Undefined functions** — now a compile-time error, not a silent NULL stub.
- **256-initialized-global cap** — removed.
- **Fixup table cap** — raised to **16384** (was 8192). The "split across multiple compilation units" workaround is only needed for very large test entry points now (e.g. `tests/test_patra_queue.tcyr` vs `tests/test_backends.tcyr`).
- **`map_get` after `map_set` in deep call chains** — cc5 resolves this.
- **`thread_create` + futex correctness** — fixed via the `_thread_spawn` inline-asm clone trampoline in `lib/thread.cyr` (5.4.10) + aarch64 SP-alignment fix (5.4.11). Multi-threaded `cbarrier_arrive_and_wait` works under cc5 5.4.10+.
- **Str-keyed hashmaps** — use `map_new_str()` (5.4.14+) for Str-struct keys. Legacy `map_new()` continues to serve cstr keys correctly.
