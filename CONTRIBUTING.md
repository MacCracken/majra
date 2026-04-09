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
# Compile
cyrius build src/main.cyr build/majra

# Run tests
cyrius test

# Run benchmarks (with history)
cyrius bench

# Full audit (self-host, test, fmt, lint, vet, deny, bench)
cyrius audit

# Policy enforcement
cyrius deny src/main.cyr

# Live integration tests (requires Redis on :6379, PostgreSQL on :5432)
cyrius build tests/test_live.tcyr build/test_live && ./build/test_live
```

Before opening a PR, run `cyrius audit` to verify everything passes.

## Adding a New Module

1. Create `src/module.cyr` with your implementation.
2. Add `include "src/module.cyr"` to `src/main.cyr` in dependency order.
3. Add unit tests to the appropriate test file (`tests/test_core.tcyr` or a new file).
4. If the module adds significant code, it may need its own test compilation unit
   (compiler fixup table limit is 8192 forward references).
5. Update `README.md` with the new module's entry.

## Code Style

- Functions: `snake_case` — `fn pubsub_new()`, `fn mq_enqueue()`
- Structs: document layout as comments — offsets, field sizes
- Internal functions: prefix with `_` — `fn _set_contains()`
- Globals for cross-call state: prefix with `_` — `var _recv_r = 0`
- Constants via enums: `enum Priority { PRIORITY_HIGH = 1; }`
- No `\r` in string literals — use raw byte 13 for CR
- Use `fl_alloc` for structs that will be freed; `alloc` for long-lived collections

## Testing

- Unit tests go in `src/main.cyr` or `tests/test_core.tcyr`
- Backend protocol tests go in `tests/test_backends.tcyr`
- Live integration tests go in `tests/test_live.tcyr`
- All new features require tests before merge
- Concurrent types should have multi-thread tests where feasible

## Known Compiler Limitations

- **Local variable clobbering**: function calls may overwrite caller's local variables.
  Save values to globals before calling other functions when they must survive the call.
- **Fixup table limit**: programs with >8192 forward references fail to compile.
  Split into multiple compilation units if needed.

## License

majra is licensed under **GPL-3.0-only**. All contributions must be compatible
with this license. By submitting a pull request, you agree that your
contribution is licensed under the same terms.
