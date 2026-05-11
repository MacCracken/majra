# Majra — Claude Code Instructions

> **Core rule**: this file is **preferences, process, and procedures** — durable rules that change rarely. Volatile state (current version, dep versions, binary sizes, test counts, consumers, in-flight work) lives in [`docs/development/state.md`](docs/development/state.md), refreshed every release. Do not inline state here — inlined state rots within a minor. See [first-party-documentation § CLAUDE.md](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#claudemd).

---

## Project Identity

**Majra** (Arabic / Persian: مجرا — channel, conduit, waterway) — Distributed queue and multiplex engine for the AGNOS ecosystem: pub/sub, queues, relay, IPC, heartbeat, rate limiting, encrypted transport, signed envelopes.

- **Type**: Cyrius shared library (single-file dist bundles, `include`-driven consumption)
- **License**: GPL-3.0-only
- **Language**: Cyrius (toolchain pinned in `cyrius.cyml [package].cyrius`)
- **Version**: `VERSION` at repo root is the source of truth — do not inline the number anywhere
- **Genesis repo**: [agnosticos](https://github.com/MacCracken/agnosticos)
- **Standards**: [First-Party Standards](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-standards.md) · [First-Party Documentation](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md)
- **Shared crates**: [shared-crates.md](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/shared-crates.md)

## Goal

Own the messaging primitives so consumer projects (daimon, AgnosAI, hoosh, sutra, stiva, ifran, secureyeoman) don't each rebuild pub/sub, queues, relay, and heartbeat in subtly different ways. Zero external deps for the core profile; first-party sigil for the crypto profiles; nothing else. Every operation measurable, auditable, traceable. No magic.

## Current State

> Volatile state lives in [`docs/development/state.md`](docs/development/state.md) — current version, cyrius pin, resolved dep versions, test counts per suite, bundle sizes, consumer table, recent releases, in-flight blockers. Refreshed every release.
>
> Historical release narrative lives in [`CHANGELOG.md`](CHANGELOG.md).
>
> Doc-currency ledger lives in [`docs/doc-health.md`](docs/doc-health.md).

This file (`CLAUDE.md`) is durable rules.

## Quick Start

```bash
cyrius deps                                            # resolve lib/ from cyrius.cyml
cyrius build src/main.cyr build/majra && ./build/majra # build + core tests
cyrius build tests/test_backends.tcyr build/test_backends && ./build/test_backends
cyrius distlib && cyrius distlib signed && cyrius distlib admin && cyrius distlib backends  # regenerate 4 dist bundles
cyrius audit                                           # full: self-host, test, fmt, lint, vet, deny, bench
```

Full test matrix + soak + fuzz + bench commands in [`docs/guides/testing.md`](docs/guides/testing.md).

## Key Principles

- **Own the stack.** Zero external deps for the core profile. Richer profiles pull sigil (first-party). Nothing from outside the AGNOS tree.
- **No magic.** Every operation measurable, auditable, traceable. If you can't measure it, you can't ship it.
- **Numbers don't lie.** Benchmark before claiming perf. Test before claiming correctness. `0 failed` or it didn't pass.
- **`fl_alloc` for structs, `alloc` for hashmaps.** Freelist supports individual free; bump allocator is for long-lived collections. See [`docs/architecture/001-cyrius-compiler-quirks.md`](docs/architecture/001-cyrius-compiler-quirks.md) §2.
- **Reach for globals when cc5 clobbers locals.** Rare under the cc5 5.10.x line but real. See [`docs/architecture/001-cyrius-compiler-quirks.md`](docs/architecture/001-cyrius-compiler-quirks.md) §1.
- **The dist bundles ARE the distribution contract.** Four `cyrius distlib` profiles — every tagged release commits all four. CI's freshness gate fails on stale diff.

## Rules (Hard Constraints)

- **Read the genesis repo's CLAUDE.md first** — [agnosticos/CLAUDE.md](https://github.com/MacCracken/agnosticos/blob/main/CLAUDE.md)
- **Do not commit or push** — the user handles all git operations (commit, push, tag)
- **NEVER use `gh` CLI** — use `curl` against the GitHub API if needed
- **Do not hardcode the Cyrius version in CI YAML** — the `cyrius = "..."` field in `cyrius.cyml` is the single source of truth. CI reads it dynamically.
- **Do not forget to regenerate `dist/`** after any `src/` change — all four profile bundles must move together.
- **Do not add dependencies beyond the Cyrius toolchain + sigil.** Crypto goes through sigil; everything else uses stdlib.
- **Do not commit `build/` or `lib/`** — both are gitignored, repopulated by `cyrius build` / `cyrius deps`.
- **Do not skip benchmarks before claiming performance improvements.**
- **Do not skip the soak set** if a change could plausibly affect queue lifecycle, relay dedup, pubsub fan-out, barrier cycles, or heartbeat eviction.

## Process

### P(-1): Hardening (before any minor cut, and at v3.0 cut)

Run this pass before tagging `X.Y.0` or `X.0.0` — ship the result as the last patch of the current minor (e.g. `2.4.5` before `2.5.0`).

1. **Cleanliness** — `cyrius build`, `cyrius lint`, `cyrius fmt --check`, `cyrius vet`; all four test suites + fuzz + benches clean.
2. **Benchmark baseline** — `cyrius bench`; archive the CSV row in `bench-history.csv` for comparison.
3. **Internal deep review** — module-by-module walk for gaps, missed guards, ABI leaks, silently-ignored errors, off-by-ones, dead code.
4. **External research** — domain completeness against current state-of-the-art (RFC drift, new CVE classes for the protocol surfaces we ship: RESP, PostgreSQL wire, WebSocket, HTTP, AES-GCM, Ed25519).
5. **Security audit** — input handling, syscall usage, buffer sizes, pointer validation. File findings in `docs/audit/YYYY-MM-DD-audit.md` (directory created when first earned).
6. **Additional tests / benchmarks** from review + audit findings.
7. **Post-review benchmarks** — prove the wins against the step-2 baseline.
8. **Doc audit** — refresh `docs/development/state.md`, run the [`docs/doc-health.md`](docs/doc-health.md) sweep, add an architecture note for any quirk surfaced, file an ADR for any decision the cycle earned.
9. **Bundle freshness** — regenerate all four dist bundles; CI gate stays green.
10. **Repeat if heavy** — keep drilling until the pass is genuinely clean, not just "no errors."

### Development Loop (continuous, between hardening passes)

1. Work phase — new features, roadmap items, bug fixes.
2. Compile + test: all suites must report `0 failed`.
3. Lint + format: `cyrius fmt --check`, `cyrius lint`.
4. Benchmark additions for new code; `cyrius bench` for regression check.
5. Policy: `cyrius deny src/main.cyr` if touching syscall / network / fs surfaces.
6. Regenerate all four dist bundles if `src/` changed.
7. Docs: `CHANGELOG.md`, `docs/development/roadmap.md`, `docs/development/state.md`, [`docs/doc-health.md`](docs/doc-health.md) row touch.
8. Version sync: `VERSION` is the source of truth; `cyrius.cyml` reads it via `${file:VERSION}`.

### Closeout Pass (last patch of every minor, before tagging `X.Y+1.0`)

Subset of P(-1) — same shape, lighter touch:

1. Full test suite — `0 failed` across all 4 suites + fuzz.
2. Benchmark vs prior closeout — flag regressions > 10%.
3. Cleanup sweep — stale comments, dead `#ifdef` branches, unused includes.
4. Doc sync — CHANGELOG stanza, roadmap "Recently shipped" update, state.md refresh, doc-health.md sweep.
5. Bundle regen — all four `cyrius distlib` profiles.
6. Version verify — `VERSION`, `cyrius.cyml`, CHANGELOG header, intended git tag all match.
7. Clean build — `rm -rf build lib && cyrius deps && cyrius build src/main.cyr build/majra` passes from cold.

### Distribution Contract

majra is an **upstream Cyrius library**. Downstream projects wire it via `cyrius.cyml`:

```toml
[deps.majra]
git = "https://github.com/MacCracken/majra.git"
tag = "<majra version>"
modules = ["dist/majra.cyr"]           # core engine, no crypto
# or a richer profile:
# modules = ["dist/majra-signed.cyr"]    core + signed envelopes (pulls sigil)
# modules = ["dist/majra-admin.cyr"]     core + HTTP admin endpoint
# modules = ["dist/majra-backends.cyr"]  everything
```

Every tagged release MUST ship all four bundles. A consumer pinning a profile that's missing or stale breaks at `cyrius deps` time. CI's distribution-freshness gate enforces this on every push.

### CI / Release shape

- **Toolchain pin** in `cyrius.cyml [package].cyrius`. CI extracts dynamically; never hardcoded in YAML.
- **Versioned toolchain layout** in CI: `~/.cyrius/versions/<V>/{bin,lib}` with `~/.cyrius/{bin,lib}` symlinking active. Required by cc5 5.10.9+ for arch-peer include resolution.
- **`cyrius deps` runs before any build/test step.** With `cyrius.lock` committed, `cyrius deps --verify` enforces hash match.
- **Manifest completeness gate**: every `include "src/<file>.cyr"` in `src/main.cyr` must appear under `[lib] modules` in `cyrius.cyml`.
- **Distribution freshness gate**: all four `cyrius distlib` invocations run; CI fails on `git diff dist/` non-empty.
- **Release tag pattern**: `v[0-9]+.[0-9]+.[0-9]+` or `[0-9]+.[0-9]+.[0-9]+` (semver shape enforced post-trigger).
- **Version gate**: release asserts `VERSION == git tag` before building.
- **Docs gate**: CI verifies `VERSION` appears as `[x.y.z]` heading in `CHANGELOG.md`.

## Cyrius Conventions

- All struct fields are 8 bytes (`i64`), accessed via `load64` / `store64` with offset.
- Heap allocation: `fl_alloc()` / `fl_free()` (freelist) for structs with explicit lifetimes; `alloc()` for long-lived collections.
- `var buf[N]` inside a fn is **static data, not stack** — see [`docs/architecture/001-cyrius-compiler-quirks.md`](docs/architecture/001-cyrius-compiler-quirks.md) §6.
- `match` is reserved — don't use as a variable name.
- `return;` without value is invalid — always `return 0;`.
- All `var` declarations are function-scoped — no block scoping.

Full compiler-quirk list: [`docs/architecture/001-cyrius-compiler-quirks.md`](docs/architecture/001-cyrius-compiler-quirks.md).

## Docs

- [`README.md`](README.md) — what / why / quick start / module table / consumer ecosystem. The reader-landing page.
- [`CHANGELOG.md`](CHANGELOG.md) — source of truth for shipped work.
- [`docs/architecture/`](docs/architecture/) — non-obvious invariants and quirks. Numbered, never renumbered.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — system-level module map + data flow.
- [`docs/development/roadmap.md`](docs/development/roadmap.md) — recently shipped + open items + waiting-on-upstream.
- [`docs/development/state.md`](docs/development/state.md) — **live state, refreshed every release.**
- [`docs/development/dependency-watch.md`](docs/development/dependency-watch.md) — per-dep version tracking, upgrade rationale.
- [`docs/development/semver.md`](docs/development/semver.md) — semver promise for the 2.x line.
- [`docs/development/threat-model.md`](docs/development/threat-model.md) — trust boundaries, attack surface table.
- [`docs/guides/`](docs/guides/) — testing how-to, consumer migration recipes.
- [`docs/benchmarks/`](docs/benchmarks/) — point-in-time perf snapshots (v2.0.0 cutover headliner).
- [`docs/doc-health.md`](docs/doc-health.md) — whole-tree doc-currency ledger.
- [`tests/soak/README.md`](tests/soak/README.md) — soak-test conventions.

Full doc-tree convention: [first-party-documentation.md](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md).
