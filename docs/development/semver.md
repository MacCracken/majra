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
| `ws_send_text` | `majra_ws_send_text` | 2.8.0 | `lib/ws.cyr` — **arity and contract disagreed**: majra `(fd, data, len)` → 0 / -1, stdlib `(ws, msg)` → bytes written |
| `ws_recv_frame` | `majra_ws_recv_frame` | 2.8.0 | `lib/ws.cyr` — **arity and contract disagreed**: majra `(fd)` → frame struct, stdlib `(ws, opcode_out, len_out)` → payload pointer |

**Migration (2.8.0 `ws_*`)**: replace the two names at each call site; arguments
and return values are unchanged. A missed call site cannot slip through. Under
cyrius 6.6.4 a 3-argument `ws_send_text` against `lib/ws.cyr` is `expects 2
arguments, got 3`, and without `lib/ws.cyr` it is an undefined function. Before
the rename, a unit that included `lib/ws.cyr` next to `dist/majra-backends.cyr`
did not build at all under 6.6.4: the compiler rejects a duplicate fn whose arity
disagrees.

### Signature changes taken in 2.6.9 (a PATCH)

The first P(-1) audit found three public signatures that could not be made
correct in place. They shipped in **2.6.9**, a PATCH, because leaving them was
worse than breaking them — each old form was actively wrong rather than merely
inconvenient, and all three produce a **compile error** at the call site, so no
consumer can pick the change up silently.

| Symbol | Change | Why it could not stay |
|---|---|---|
| `encrypted_ipc_new` | gained a required `role` argument | Both directions derived nonces from independent counters under one key, so every message pair at the same counter reused `(key, nonce)` — keystream reuse plus GHASH subkey leakage. The 2-argument form cannot be made safe |
| `majra_admin_serve` | `addr` is now a parsed dotted-quad string | It was forwarded to `sockaddr_in`, which wants a packed integer, so the documented `"127.0.0.1"` call bound to the low 32 bits of a `char*` |
| `transport_send` / `transport_recv` | forward all 3 arguments | They dropped `len`, so an implementation written to the documented vtable contract read a garbage length |

Behavioural changes in the same release, without a signature change:
`namespace_new` rejects invalid prefixes (returns 0), `mq_job_count` counts
live rather than cumulative jobs, and `patra_queue`'s payload column moved
`STR` → `TEXT`. See [`../guides/migration-2.6.9.md`](../guides/migration-2.6.9.md).

New public symbols should carry a module prefix (`queue_`, `relay_`,
`majra_admin_`, …) precisely so this class of collision cannot recur. Check a
proposed public name against the `lib/` snapshot before adding it:

```bash
grep -rn "^fn <name>\b" lib/
```

### Security fixes that require a wire change (added 2.9.0)

A defect in a wire format cannot always be fixed compatibly: cross-connection
replay in encrypted IPC needs a handshake, and signature confusion needs a
domain prefix in the signed bytes. Both change what goes over the wire, which
the table above calls breaking, i.e. 3.0.0 — and holding a confirmed security
defect for an indefinite major was judged worse than breaking the format.

So: **a MINOR may change a wire format when the change fixes a confirmed
security defect that cannot be fixed compatibly.** Three conditions gate it,
and all three must hold:

1. The defect is **confirmed**, not theoretical, and named in an audit report
   or issue.
2. There is **no compatible fix** — an opt-in flag would leave the default
   vulnerable, which is the state the fix exists to end.
3. The release ships a **migration guide** naming what breaks, and the
   CHANGELOG entry leads with it.

⚠ The consequence is real and must be stated plainly every time: peers on the
two versions cannot talk to each other, so every end of a deployment upgrades
together. A rolling upgrade across the break needs a second channel, or a stop.

Wire breaks taken under this exception:

| Release | What changed | Defect it closes |
|---|---|---|
| 2.9.0 | Encrypted IPC: a 32-byte session hello per connection, and the AES key is HKDF-derived from the PSK plus both salts | Frames captured on one connection replayed into any later connection under the same PSK |
| 2.9.0 | Signed envelopes: the signing input is prefixed with `majra/signed-envelope/v1` | A majra envelope signature could be confused with another Ed25519 message signed under the same key |

## Deprecations

Deprecated names keep compiling, with unchanged values and behaviour, for the
rest of 2.x. They are removed at 3.0.0. The exception is a deprecated name that
starts colliding with a stdlib symbol first; that name may go earlier under
Documented exceptions above.

| Deprecated | Replacement | Since | Why |
|---|---|---|---|
| bare `ERR_*` error codes (all 20: `ERR_NONE` … `ERR_WORKFLOW_STORAGE`, `ERR_IPC_FRAME_TOO_LARGE` … `ERR_IPC_JSON`) | `MAJRA_ERR_*`, same values | 2.8.0 | Enum constants share one flat namespace across linked libraries, and bare `ERR_*` is reserved for the sakshi base logger (cyrius lint, proposal `2026-07-11-error-enum-namespace-lint-gate`). Nothing collided at 2.8.0; the prefix keeps it that way. `src/main.cyr`'s `test_error` asserts every alias still equals its replacement. |

## API stability

All public functions documented in `src/*.cyr` are stable, except where noted
under Documented exceptions above:

| Module | Stable since |
|--------|-------------|
| error, counter, envelope, namespace | 2.0.0 (error codes `MAJRA_ERR_*` since 2.8.0; bare `ERR_*` deprecated — see Deprecations) |
| metrics, ratelimit, heartbeat | 2.0.0 |
| queue, pubsub, relay, barrier | 2.0.0 |
| ipc, transport, fleet, dag | 2.0.0 |
| redis_backend, postgres_backend, ws | 2.0.0 (`ws_send_text` / `ws_recv_frame` renamed `majra_ws_*` at 2.8.0 — see Documented exceptions) |
| ipc_encrypted wire format | 2.0.0 framing; **session handshake at 2.9.0** — see Security fixes that require a wire change |
| signed_envelope signing input | 2.4.0; **domain prefix at 2.9.0** — same exception |
| ipc_encrypted (AES-256-GCM via sigil; framing, nonce/role separation, rekey) | 2.0.0 framing; role argument 2.6.9 — see Documented exceptions |
| signed_envelope | 2.4.0 |
| admin | 2.4.0 (`majra_admin_serve` signature changed at 2.6.9) |
| patra_queue | 2.4.0 (payload column `STR` → `TEXT` at 2.6.9) |
