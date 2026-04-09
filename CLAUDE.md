# Majra — Claude Code Instructions

## Project Identity

**Majra** (Arabic: channel/waterway) — Distributed queue and multiplex engine — pub/sub, queues, relay, IPC, heartbeat, rate limiting

- **Type**: Cyrius library (single-file compilation via `include`)
- **License**: GPL-3.0-only
- **Version**: SemVer 2.0.0
- **Language**: [Cyrius](https://github.com/MacCracken/cyrius) (ported from Rust v1.0.4)
- **Genesis repo**: [agnosticos](https://github.com/MacCracken/agnosticos)
- **Philosophy**: [AGNOS Philosophy & Intention](https://github.com/MacCracken/agnosticos/blob/main/docs/philosophy.md)
- **Standards**: [First-Party Standards](https://github.com/MacCracken/agnosticos/blob/main/docs/development/applications/first-party-standards.md)
- **Recipes**: [zugot](https://github.com/MacCracken/zugot) — takumi build recipes

## Consumers

daimon (agent messaging), AgnosAI (crew coordination), hoosh (inference routing), sutra (fleet), stiva (container IPC)

## Development Process

### Build & Test

```bash
# Compile
cyrius build src/main.cyr build/majra

# Run tests
cyrius test

# Run benchmarks (with history tracking)
cyrius bench

# Full audit: self-host, test, fmt, lint, vet, deny, bench
cyrius audit

# Policy enforcement
cyrius deny src/main.cyr

# Run live integration tests (requires Redis + PostgreSQL)
cyrius build tests/test_live.cyr build/test_live && ./build/test_live
```

### Development Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Compile check: `cyrius build src/main.cyr build/majra`
3. Test: `cyrius test` — all suites must pass
4. Lint + format: `cyrius fmt --check`, `cyrius lint`
5. Policy check: `cyrius deny src/main.cyr`
6. Benchmark additions for new code
7. Run benchmarks: `cyrius bench` (tracks history automatically)
8. Audit phase — review performance, memory, security, correctness
9. Deeper tests/benchmarks from audit observations
10. Run benchmarks again — prove the wins
11. If audit heavy → return to step 8
12. Full audit: `cyrius audit` — self-host, test, fmt, lint, vet, deny, bench
13. Documentation — update CHANGELOG, roadmap, docs
14. Version check — VERSION and cyrius.toml in sync (`scripts/version-bump.sh`)
15. Return to step 1

### Key Principles

- **Never skip benchmarks.** Numbers don't lie.
- **Tests + benchmarks are the way.** 295 assertions across 4 test suites.
- **Own the stack.** Zero external dependencies — Cyrius stdlib only.
- **No magic.** Every operation is measurable, auditable, traceable.
- **Globals for cross-call state.** Cyrius single-pass compiler clobbers locals across function calls — use globals when values must survive nested calls.
- **Raw bytes for CR LF.** Cyrius does not support `\r` escape — use `store8(buf, 13); store8(buf+1, 10)` for network protocols.
- **`fl_alloc` for structs, `alloc` for hashmaps.** Freelist supports individual free; bump allocator for long-lived collections.
- **Compiler fixup limit: 8192.** Split large programs across multiple compilation units.

## Project Structure

```
src/main.cyr           Entry point + core tests (144 assertions)
src/*.cyr              19 library modules
tests/test_core.cyr    Expanded unit tests (92 assertions)
tests/test_backends.cyr Backend protocol tests (25 assertions)
tests/test_live.cyr    Live Redis + PostgreSQL tests (36 assertions)
tests/test.sh          Test runner script
benches/bench_all.cyr  17 benchmarks
examples/              managed_queue.cyr, pubsub_tiers.cyr
lib/                   Vendored Cyrius stdlib (28 modules)
rust-old/              Archived Rust source (reference)
build/                 Compiled binaries (gitignored)
```

## Documentation Structure

```
Root files (required):
  README.md, CHANGELOG.md, CLAUDE.md, CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md, LICENSE

docs/ (required):
  architecture/overview.md — module map, data flow, consumers
  development/roadmap.md — completed, backlog, future

docs/ (when earned):
  guides/ — usage guides, integration patterns
  development/ — semver, threat model
```

## DO NOT
- **Do not commit or push** — the user handles all git operations (commit, push, tag)
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies — Cyrius stdlib only
- Do not skip benchmarks before claiming performance improvements
- Do not commit `build/` or `rust-old/target/`

## Known Cyrius Compiler Issues

1. **Local variable clobbering** — function parameters and locals may be overwritten by nested function calls. Workaround: save critical values to globals before calling other functions.
2. **`map_get` after `map_set` in same call chain** — hashmap lookups may fail to find entries set in deeply nested call contexts. Workaround: restructure to minimize call depth between set and get.
3. **No `\r` escape sequence** — use raw byte 13 for carriage return in network protocols (RESP, HTTP, WebSocket).
4. **Fixup table limit (8192)** — programs with more than ~8192 forward references fail to compile. Split into multiple compilation units.
