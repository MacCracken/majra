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

## Documented exceptions

A symbol that collides with a Cyrius stdlib symbol of the same name **was never
covered by the promise above**, because its meaning was decided by the
consumer's include order rather than by majra. `last definition wins` is a
property of the consuming build, not of this library, so majra cannot promise
anything about such a name — and a name the promise never covered cannot be
broken by renaming it. Renaming out of a collision is therefore **not treated as
an API change** for the purposes of the tiers above, and may ship in whatever
cut is in flight, including a PATCH — with the rename, its rationale, and a
migration note recorded here and in `CHANGELOG.md`.

Two conditions gate this exception, and both must hold:

1. The colliding definitions **disagree** in return contract, parameter count,
   or semantics — so include order can silently change behaviour, not merely
   pick between two equivalent implementations.
2. The rename produces a **compile error** at every affected call site. A
   consumer must never be able to pick up the change silently.

Renames taken under this exception:

| Old name | New name | Cut | Collided with |
|----------|----------|-----|---------------|
| `base64_encode` | `majra_base64_encode` | 2.6.8 | `lib/bayan.cyr` (wrapper over `bayan_base64_encode`) |
| `base64_decode` | `majra_base64_decode` | 2.6.8 | `lib/bayan.cyr` — **return contracts disagreed**: majra returned a `{ptr, len}` struct, bayan returns a scalar `i64` |

New public symbols should carry a module prefix (`queue_`, `relay_`,
`majra_admin_`, …) precisely so this class of collision cannot recur. Check a
proposed public name against the `lib/` snapshot before adding it:

```bash
grep -rn "^fn <name>\b" lib/
```

## API stability

All public functions documented in `src/*.cyr` are stable, except where noted
under Documented exceptions above:

| Module | Stable since |
|--------|-------------|
| error, counter, envelope, namespace | 2.0.0 |
| metrics, ratelimit, heartbeat | 2.0.0 |
| queue, pubsub, relay, barrier | 2.0.0 |
| ipc, transport, fleet, dag | 2.0.0 |
| redis_backend, postgres_backend, ws | 2.0.0 |
| ipc_encrypted (framing only, crypto stub) | 2.0.0 |
