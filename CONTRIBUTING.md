# Contributing to majra

Thank you for your interest in contributing to majra. This document covers the
development workflow, code standards, and project conventions.

## Development Workflow

1. **Fork** the repository on GitHub.
2. **Create a branch** from `main` for your work.
3. **Make your changes**, ensuring all checks pass.
4. **Open a pull request** against `main`.

## Prerequisites

- Rust toolchain (MSRV: **1.89**)
- `cargo-deny` — supply chain checks
- `cargo-fuzz` — fuzz testing (requires nightly)
- `cargo-llvm-cov` — code coverage

## Makefile Targets

| Target          | Description                                      |
| --------------- | ------------------------------------------------ |
| `make check`    | Run fmt + clippy + test + audit (the full suite) |
| `make fmt`      | Format code with `cargo fmt`                     |
| `make clippy`   | Lint with `cargo clippy`                         |
| `make test`     | Run the test suite                               |
| `make deny`     | Audit dependencies with `cargo deny`             |
| `make fuzz`     | Run fuzz targets                                 |
| `make coverage` | Generate code coverage report                    |

Before opening a PR, run `make check` to verify everything passes.

## Adding a New Module

1. Create `src/module.rs` with your implementation.
2. Add the module to `src/lib.rs` (feature-gated if appropriate).
3. Add unit tests in the module file under `#[cfg(test)]`.
4. Add the feature flag to `Cargo.toml` if applicable.
5. Update `README.md` with the new module's feature table entry.

## Code Style

- Run `cargo fmt` before committing. All code must be formatted.
- `cargo clippy -D warnings` must pass with no warnings.
- All public items (functions, structs, enums, traits, type aliases) must have
  doc comments.
- Keep functions focused and testable.
- Use `#[non_exhaustive]` on public enums for forward compatibility.

## Testing

- Unit tests go in the module file under `#[cfg(test)]`.
- Concurrent types must have multi-thread tests.
- Property-based and fuzz tests are encouraged for serialisation paths.
- All new features require tests before merge.

## License

majra is licensed under **AGPL-3.0-only**. All contributions must be compatible
with this license. By submitting a pull request, you agree that your
contribution is licensed under the same terms.
