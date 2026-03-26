# SemVer Guarantee — majra 1.x

## Promise

Starting with version 1.0.0, majra follows [Semantic Versioning 2.0.0](https://semver.org/):

- **PATCH** (1.0.x): Bug fixes, performance improvements, documentation. No API changes.
- **MINOR** (1.x.0): New features, new modules, new trait methods with defaults. All existing code compiles without changes.
- **MAJOR** (2.0.0): Reserved for breaking changes. Not planned.

## What counts as a breaking change

Any of the following in a PATCH or MINOR release would violate this guarantee:

- Removing or renaming a public type, function, method, constant, or module
- Changing the signature of a public function or method (parameters, return type, generics)
- Changing the meaning of an existing enum variant
- Adding a required method to a public trait (methods with defaults are fine)
- Changing a public struct's fields from public to private
- Raising the MSRV (Minimum Supported Rust Version) in a PATCH release
- Removing a feature flag or changing which modules a feature flag enables
- Changing the wire format of serialized types (`Envelope`, `RelayMessage`, etc.) in a way that breaks deserialization of previously serialized data

## What is NOT a breaking change

The following may happen in MINOR releases:

- Adding new public types, functions, methods, modules, or feature flags
- Adding new variants to `#[non_exhaustive]` enums
- Adding new fields to `#[non_exhaustive]` structs
- Adding new trait methods with default implementations
- Raising the MSRV in a MINOR release (with changelog notice)
- Changing internal implementation details (algorithms, data structures)
- Improving performance characteristics
- Adding new optional dependencies behind feature gates
- Deprecating items (with `#[deprecated]` and migration guidance)

## `#[non_exhaustive]` policy

All public enums use `#[non_exhaustive]`. Consumers must include a wildcard arm
when matching, which allows us to add variants without breaking downstream code:

```rust
match error {
    MajraError::Queue(msg) => { /* ... */ }
    MajraError::PubSub(msg) => { /* ... */ }
    // Required by #[non_exhaustive]:
    _ => { /* handle unknown variants */ }
}
```

## Feature flag stability

All feature flags documented in `Cargo.toml` are stable:

| Flag | Stable since |
|------|-------------|
| `pubsub` | 1.0.0 |
| `queue` | 1.0.0 |
| `relay` | 1.0.0 |
| `heartbeat` | 1.0.0 |
| `ipc` | 1.0.0 |
| `ratelimit` | 1.0.0 |
| `barrier` | 1.0.0 |
| `dag` | 1.0.0 |
| `sqlite` | 1.0.0 |
| `logging` | 1.0.0 |
| `full` | 1.0.0 |

New feature flags may be added in MINOR releases. Existing flags will not be
removed or have their module mappings changed in 1.x.

## MSRV policy

- The MSRV will not increase in PATCH releases.
- The MSRV may increase in MINOR releases, documented in the changelog.
- We target a 6-month trailing window: the MSRV will be no older than the Rust version released 6 months prior.

## CI enforcement

The `cargo-semver-checks` tool runs in CI on every pull request to detect
accidental breaking changes before merge.
