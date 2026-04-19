# Majra — Claude Code Instructions

> **READ the distribution section before touching build or release.**
> majra ships to downstream Cyrius projects via a committed
> `dist/majra.cyr` (default engine) and `dist/majra-backends.cyr`
> (with Redis / PostgreSQL / encrypted-IPC / WebSocket) produced
> by `cyrius distlib`. That is the only distribution contract.
> Libro and patra are the references. Do not invent alternatives.

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

Cyrius stdlib only — vendored under `lib/`. The `[deps] stdlib = [...]`
list in `cyrius.cyml` mirrors the `include "lib/<name>.cyr"` lines at
the top of `src/main.cyr`. Keep them in sync.

No external first-party deps today. If one is added, wire it under
`[deps.<name>]` with `git` + `tag` + `modules` like libro wires sigil
and patra.

## Build & Test

```bash
# Compile (entry + output come from cyrius.cyml [build])
cyrius build src/main.cyr build/majra

# Run core tests (144 assertions)
./build/majra

# Expanded unit suite (92 assertions)
cyrius build tests/test_core.tcyr build/test_core && ./build/test_core

# Backend protocol suite (25 assertions, Redis/PostgreSQL/WS mocks)
cyrius build tests/test_backends.tcyr build/test_backends && ./build/test_backends

# Live integration tests (requires Redis + PostgreSQL)
cyrius build tests/test_live.tcyr build/test_live && ./build/test_live

# Benchmarks (with history tracking)
cyrius build benches/bench_all.bcyr build/bench_all && ./build/bench_all

# Regenerate distribution bundles (commit alongside src/ changes)
cyrius distlib          # → dist/majra.cyr           (core engine)
cyrius distlib backends # → dist/majra-backends.cyr  (+redis/pg/ws/encrypted-ipc)

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
11. Regenerate bundles: `cyrius distlib && cyrius distlib backends`
12. Full audit: `cyrius audit`
13. Documentation — update CHANGELOG, roadmap, docs
14. Version sync — `VERSION` is source of truth; `cyrius.cyml` uses `${file:VERSION}` (`scripts/version-bump.sh`)
15. Return to step 1

### Key Principles

- **Never skip benchmarks.** Numbers don't lie.
- **Tests + benchmarks are the way.** 256 assertions across 3 suites (144 core + 92 expanded + 20 backends), plus 36 live-integration.
- **Own the stack.** Zero external dependencies beyond the Cyrius toolchain.
- **No magic.** Every operation is measurable, auditable, traceable.
- **`fl_alloc` for structs, `alloc` for hashmaps.** Freelist supports individual free; bump allocator for long-lived collections.
- **Globals for cross-call state when cc5 clobbers locals.** cc5 is significantly better than cc3 at this, but if a local's value looks wrong after a nested call, promote it to a global.

## Project Structure

```
cyrius.cyml            Manifest ([package], [build], [lib], [lib.backends], [deps])
VERSION                Source of truth for the version string
src/main.cyr           Entry point + core tests (144 assertions)
src/*.cyr              15 core modules + 4 backend modules (redis, postgres, ipc_encrypted, ws)
tests/test_core.tcyr   Expanded unit tests (92 assertions)
tests/test_backends.tcyr  Backend protocol tests (20 assertions)
tests/test_live.tcyr   Live Redis + PostgreSQL tests (36 assertions)
tests/test.sh          Test runner script
benches/bench_all.bcyr 17 benchmarks
examples/              managed_queue.cyr, pubsub_tiers.cyr
fuzz/*.fcyr            Fuzz harnesses (heartbeat, pubsub, queue)
dist/majra.cyr         Consumer distribution artifact (default [lib] profile)
dist/majra-backends.cyr Consumer distribution artifact ([lib.backends] profile)
lib/                   Vendored Cyrius stdlib
build/                 Compiled binaries (gitignored)
scripts/version-bump.sh Syncs VERSION
docs/                  architecture, development, guides
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

- **Toolchain pin**: `cyrius` field inside `cyrius.cyml` (currently `cyrius = "5.4.8"`). CI and release extract it dynamically — no hardcoded version strings in YAML.
- **Manifest**: `cyrius.cyml` (migrated from `cyrius.toml` in 2.3.0 to match first-party convention). Uses `${file:VERSION}` so `VERSION` remains the single source of truth.
- **Manifest completeness gate**: CI asserts every `include "src/<file>.cyr"` in `src/main.cyr` is listed under `[lib] modules`. If not, `cyrius distlib` would silently ship a broken bundle.
- **Distribution freshness gate**: CI runs `cyrius distlib && cyrius distlib backends` and fails if `git diff dist/` is non-empty. Bundles must be regenerated and committed alongside any `src/` change.
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
modules = ["dist/majra.cyr"]          # core engine
# or
modules = ["dist/majra-backends.cyr"] # engine + Redis/PG/WS/encrypted IPC
```

`cyrius deps` clones at the tag and copies the chosen bundle into the
consumer's `lib/majra_majra.cyr`. Every tagged release MUST have both
`dist/majra.cyr` and `dist/majra-backends.cyr` committed, or consumer
`cyrius deps` breaks at pull time.

## DO NOT

- **Do not commit or push** — the user handles all git operations (commit, push, tag)
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not hardcode the Cyrius version in CI YAML — read the `cyrius = "..."` field from `cyrius.cyml`
- Do not forget to regenerate `dist/` after any `src/` change
- Do not add dependencies beyond the Cyrius toolchain
- Do not skip benchmarks before claiming performance improvements
- Do not commit `build/`

## Known Cyrius Compiler Quirks (5.4.8)

Most cc3-era workarounds documented in earlier majra versions are now
resolved under cc5. Quirks still worth knowing:

1. **Local variable clobbering** — still possible across deeply nested call chains in cc5, though rarer than cc3. Not a guaranteed bug. If a local's value looks wrong after a function call, promote it to a global as a workaround.
2. **Freelist vs bump allocator discipline** — `fl_alloc` + `fl_free` for individually-freed structs; `alloc()` for long-lived collections. Correct either way, but mix them thoughtfully.
3. **Single-pass compiler** — forward references across function boundaries work via fixups (cap **16384** in 5.4.x, up from 8192 in cc3). Module include order still matters for type/struct visibility.
4. **Backend modules gated behind `[lib.backends]` profile** — redis/postgres/ipc_encrypted/ws are not in the default `src/main.cyr` include list. They are compiled via separate test entry points. With the 16384 fixup cap they *could* be folded into main.cyr, but keeping them separate keeps the default engine binary lean and lets consumers choose the surface they pull in.

### Resolved under cc5 (stop treating as bugs)

- **`\r` escape sequence** — works since 4.x. Don't hand-emit byte 13 with `store8(buf, 13)`. The old `# --- Raw bytes for CR LF` workaround across network protocols is obsolete.
- **Negative literals `-1`, `-N`** — work since 3.10.3. No need for `(0 - N)`.
- **Compound assignment `+=`, `-=`, `*=`, etc.** — work since 3.10.3.
- **Undefined functions** — now a compile-time error, not a silent NULL stub.
- **256-initialized-global cap** — removed.
- **Fixup table cap** — raised to **16384** (was 8192). The "split across multiple compilation units" workaround is rarely needed now, though backend modules are still kept separate by design (see above).
- **`map_get` after `map_set` in deep call chains** — cc5 resolves this. The `relay` dedup table comment about this is historical.
