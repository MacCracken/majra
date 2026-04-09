# SemVer Guarantee — majra 2.x

## Promise

Starting with version 2.0.0 (Cyrius port), majra follows [Semantic Versioning 2.0.0](https://semver.org/):

- **PATCH** (2.0.x): Bug fixes, performance improvements, documentation. No API changes.
- **MINOR** (2.x.0): New features, new modules, new functions. All existing code compiles without changes.
- **MAJOR** (3.0.0): Reserved for breaking changes. Not planned.

## What counts as a breaking change

Any of the following in a PATCH or MINOR release would violate this guarantee:

- Removing or renaming a public function
- Changing a function's parameter count or semantics
- Changing the meaning of an existing enum constant
- Changing struct layouts (field offsets) for public structs
- Changing the wire format of protocols (RESP, PG, WebSocket, IPC framing)

## What is NOT a breaking change

The following may happen in MINOR releases:

- Adding new public functions or modules
- Adding new enum constants
- Adding new fields to the end of structs (without changing existing offsets)
- Improving performance characteristics
- Adding new test suites or benchmarks

## API stability

All public functions documented in `src/*.cyr` are stable:

| Module | Stable since |
|--------|-------------|
| error, counter, envelope, namespace | 2.0.0 |
| metrics, ratelimit, heartbeat | 2.0.0 |
| queue, pubsub, relay, barrier | 2.0.0 |
| ipc, transport, fleet, dag | 2.0.0 |
| redis_backend, postgres_backend, ws | 2.0.0 |
| ipc_encrypted (framing only, crypto stub) | 2.0.0 |
