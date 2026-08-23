# Contributing to majra

Thank you for your interest in contributing to majra. This document covers the
development workflow, code standards, and project conventions.

## Development Workflow

1. **Fork** the repository on GitHub.
2. **Create a branch** from `main` for your work.
3. **Make your changes**, ensuring all checks pass.
4. **Open a pull request** against `main`.

## Prerequisites

- Cyrius compiler (`cyrius`) — [cyrius](https://github.com/MacCracken/cyrius)
- Redis (for live integration tests)
- PostgreSQL (for live integration tests)

## Building & Testing

```bash
# One-time per checkout, and after any toolchain bump.
# --full is load-bearing: a bare `lib sync` copies only the declared
# [deps].stdlib subset and omits the toolchain modules sigil/sandhi reach into.
cyrius lib sync --full && cyrius deps

# Compile. --no-deps stops the build re-resolving and perturbing ./lib/'s
# include order; every CI build passes it.
cyrius build --no-deps src/main.cyr build/majra && ./build/majra

# Run tests
cyrius test

# Run benchmarks (with history)
cyrius bench

# Full audit (self-host, test, fmt, lint, vet, deny, bench)
cyrius audit

# Policy enforcement
cyrius deny src/main.cyr

# Live integration tests. Requires Redis on :6379 and PostgreSQL on :5432
# CONFIGURED FOR CLEARTEXT AUTH — majra's PG client implements only
# AuthenticationCleartextPassword and fails closed on SCRAM, so a stock
# postgres:16 (scram-sha-256 by default) will refuse it. See
# docs/guides/testing.md for the docker + pg_hba recipe.
cyrius build --no-deps tests/test_live.tcyr build/test_live && ./build/test_live
```

Before opening a PR, run `cyrius audit` to verify everything passes.

## Adding a New Module

1. Create `src/module.cyr` with your implementation.
2. Add `include "src/module.cyr"` to `src/main.cyr` in dependency order.
3. Add unit tests to the appropriate test file (`tests/test_core.tcyr` or a new file).
4. If the module adds significant code, it may need its own test compilation unit
   (compiler fixup-table cap is 16384 forward references; `tests/test_patra_queue.tcyr` is its own entry point for exactly this reason).
5. Add the file to `[lib] modules` in `cyrius.cyml`, in the **same order** as
   the `include` in `src/main.cyr` — single-pass forward-reference resolution
   depends on it. Add it to `[lib.signed]` / `[lib.admin]` / `[lib.backends]`
   too if those profiles should carry it. CI's manifest-completeness gate
   fails otherwise.
6. Regenerate all four bundles and commit `dist/`:
   `cyrius distlib && cyrius distlib signed && cyrius distlib admin && cyrius distlib backends`
   CI's distribution-freshness gate fails on a non-empty `git diff dist/`.
7. Update `README.md` with the new module's entry.

## Code Style

- Functions: `snake_case` — `fn pubsub_new()`, `fn mq_enqueue()`
- Structs: document layout as comments — offsets, field sizes
- Internal functions: prefix with `_` — `fn _set_contains()`
- Globals for cross-call state: prefix with `_` — `var _pg_fd_g = 0` (`src/postgres_backend.cyr`)`
- Constants via enums: `enum Priority { PRIORITY_HIGH = 1; }`
- `\r` / `\n` escapes in string literals work (since cc4.x) — use them. Do **not** hand-emit byte 13 with `store8`; see `_sb_crlf` in `src/redis_backend.cyr`
- Use `fl_alloc` for structs that will be freed; `alloc` for long-lived collections

## Testing

- Unit tests go in `src/main.cyr` or `tests/test_core.tcyr`
- Backend protocol tests go in `tests/test_backends.tcyr`
- Durable / patra-backed queue tests go in `tests/test_patra_queue.tcyr` — a
  separate entry point on purpose; folding them into `test_backends.tcyr`
  blows the 16384 fixup cap (patra pulls sakshi + io + fs transitively)
- Live integration tests go in `tests/test_live.tcyr`
- All new features require tests before merge
- Concurrent types should have multi-thread tests where feasible

## Known Compiler Limitations

- **Local variable clobbering**: function calls may overwrite caller's local variables.
  Save values to globals before calling other functions when they must survive the call.
- **Fixup table cap**: programs with >16384 forward references fail to compile. Split the entry point — reordering will not help. See `docs/development/cyrius-quirks.md` §2.
  Split into multiple compilation units if needed.

## License

majra is licensed under **GPL-3.0-only**. All contributions must be compatible
with this license. By submitting a pull request, you agree that your
contribution is licensed under the same terms.
