# Majra Roadmap

**This file is forward-facing only.** What shipped lives in
[`CHANGELOG.md`](../../CHANGELOG.md); what is true *right now* — versions, pins,
test counts, bundle sizes — lives in [`state.md`](state.md). When an item here
ships, **delete it**; do not convert it into a changelog entry.

## How to read this

Work is scheduled against **release targets**, not priority labels. Every item
names the version it is aimed at and the condition that would move it.

| Target | Theme |
|---|---|
| **2.6.10** | Hardening. Second P(-1) pass and its repairs. PATCH — no API changes. |
| **2.7.0** | Finish what the audit deferred — the additive APIs 2.6.9 declined to invent mid-repair. |
| **2.7 line** | Larger capabilities, each taking the next MINOR as its trigger fires. |
| **Waiting on upstream** | Blocked outside this repo. Names the blocker. |
| **Non-goals** | Deliberately out of scope, recorded so the question stops recurring. |

> **Why these can't all be patch releases.** [`semver.md`](semver.md) reserves
> PATCH for "bug fixes, performance, documentation — no API changes", so
> anything adding a public function takes the next MINOR. "The 2.7.x line"
> below is therefore a *development line*, not a run of patch numbers:
> `pubsub_unsubscribe` and PostgreSQL SCRAM cannot share one. 2.7.0 batches the
> three small additive items so they cost a single version between them.

An item moves when its **trigger** fires — a consumer need, a dependency
landing, or a measurement crossing a threshold. Triggers are written down so
promotion is a decision rather than a mood.

---

## 2.6.10 — hardening

### Second P(-1) pass and repairs

[`CLAUDE.md`](../../CLAUDE.md) P(-1) step 10 says *"repeat if heavy — keep
drilling until the pass is genuinely clean, not just no errors."* The
[2026-08-22 pass](../audit/2026-08-22-audit.md) returned **115 confirmed
findings**, which is heavy on any reading, and it rewrote enough code that the
rewrites themselves are unaudited.

**Done when**: a second pass over the modules 2.6.9 changed most heavily
returns no new high-severity findings, and its own write-up lands in
`docs/audit/`.

**Re-examine specifically** — all introduced by 2.6.9, none yet audited:

- **Ownership.** 2.6.9 added a large number of `fl_free` calls to code that
  previously just leaked. A leak is a far safer bug than a double-free, so every
  new free needs both of its sites traced.
- The replay window in `encrypted_ipc_recv`: counter monotonicity across a
  rekey, and whether a peer can wedge it.
- The new bounds arithmetic in the DataRow and RESP parsers — bounds checks are
  themselves a classic source of off-by-one and sign errors, and that is exactly
  where the previous bugs lived.
- `relay_send` and `relay_receive_ex` now hold the relay mutex across the
  fan-out, and `encrypted_ipc_send` now holds the connection mutex across
  `ipc_send_frame` — a socket write. Each is safe *only* if that path genuinely
  never blocks.
- Loops 2.6.9 added: `mq_dequeue`'s cancelled-job skip, `patra_queue_dequeue`'s
  claim-retry, `pg_query`'s ErrorResponse drain, `fleet_deregister_node`'s
  drain. Each needs a termination argument.

**Scope note**: repairs that require an API change do not belong in a PATCH.
If the pass surfaces one, it moves to 2.7.0 and the reason is recorded here.

### Consumer migration guide for the 2.6.9 breaking changes

Documentation, so it ships inside the PATCH without affecting its
classification.

**Done when** it covers, in the shape of the existing
`docs/guides/migration-*.md` files: `encrypted_ipc_new` (new required role
argument), `majra_admin_serve` (address is now a parsed dotted-quad string),
`transport_send`/`transport_recv` (all three arguments now forwarded),
`namespace_new` (now rejects `/`, `:`, `#`, `+`, control bytes), and the
`patra_queue` `STR` → `TEXT` schema change — which needs an explicit "delete or
migrate the existing `.patra` file" step, since an old file silently keeps its
255-byte cap.

---

## 2.7.0 — finish what the audit deferred

The additive APIs 2.6.9 declined to invent while it was repairing 115 findings.
Batched into one MINOR because each adds a public function and each is
individually small.

### pubsub unsubscribe + per-subscriber lag policy

The deferral the audit documented most carefully. Fan-out is a **blocking
backpressure contract**: 2.5.3 established it and
`test_pubsub_no_head_of_line_block` asserts it. The consequence is that a
subscriber abandoned without draining wedges publishes to its topic
permanently, because majra has no unsubscribe.

`chan_try_send` was tried during the audit and rejected — it trades a wedge for
silent message loss, the worse failure for a queue engine. The real fix is an
unsubscribe path plus an explicit lag policy (drop-oldest, drop-newest, or
disconnect) chosen by the caller rather than baked in.

**Scope**: `pubsub_unsubscribe`, a per-subscription lag policy, and a decision
on whether `relay` should share the mechanism — it already drops via
`chan_try_send`, matching `tokio::sync::broadcast`.

**Trigger**: already fired in principle — any consumer creating subscriptions
dynamically hits it. Today's consumers subscribe at startup and hold for process
life, which is the only reason it has not bitten.

### Per-key rate-limit statistics

`/ratelimit` returns the limiter's **global** counters regardless of the key
requested. 2.6.9 marked the response `"scope":"global"` so it is
self-describing, but an operator reading `/ratelimit?key=tenant-a` can still
mistake fleet-wide totals for that tenant's.

**Scope**: `ratelimit_stats_for_key(rl, key)` plus the admin route change.
**Trigger**: a multi-tenant consumer using the admin endpoint for per-tenant
observability.

### Parallel DAG tier execution

`dag.cyr` executes steps within a tier **serially**. The module header claimed
`thread_create`/`join` parallelism and always had; the code calls neither, and
2.6.9 corrected the header rather than the code. Tiers are a dependency-ordering
construct today — a coherent design, just not the advertised one.

**Scope**: thread-per-step within a tier, a bounded pool, and a decision on how
a failing step affects its siblings.
**Trigger**: a workflow whose tier width and per-step latency make serial
execution the bottleneck. Needs a measurement, not an intuition — and it should
be taken with the concurrency lessons of the 2.6.9 audit in hand.

---

## 2.7 line — larger capabilities

Each takes the next available MINOR when its trigger fires. Ordered by expected
value, not by commitment.

### PostgreSQL SCRAM-SHA-256 + `SSLRequest`

`postgres_backend.cyr` speaks **plaintext** and implements only
`AuthenticationCleartextPassword`, so the password and every query and result
row cross the wire in the clear. PostgreSQL has defaulted to `scram-sha-256`
since v14, so reaching a modern server means weakening `pg_hba.conf` to
`password` — which CI does today, and that is the tell.

2.6.9 made the connect **fail closed** on auth type 10 rather than downgrade,
so the current state is at least honest. It remains the largest capability gap
majra ships.

**Scope**: SCRAM-SHA-256 (SASL, RFC 5802) over sigil's HMAC-SHA256 + PBKDF2,
and `SSLRequest` (code 80877103) + TLS via `lib/tls.cyr`.
**Trigger**: any consumer needing PostgreSQL over something other than loopback
or an already-confidential channel.

### QUIC transport

Unblocked on the sigil side — X25519 has been available since 3.7.8.
**Trigger**: a consumer needing multiplexed streams or connection migration
that Unix-socket IPC and TCP do not cover. Scope it then: it is a large surface,
and the audit is a standing reminder that new wire parsers are where the bugs
are.

### aarch64 cross-build wiring

The historical `SYS_OPEN` blocker is long gone (agnosys 1.3.2). Wiring and
verifying a `cyrius build --aarch64` CI step is a discrete, unblocked
*verification* task — not a porting one, and arguably a PATCH since it adds no
API.
**Trigger**: any non-x86_64 consumer. All current consumers are
x86_64-server-side.

### Shared-memory IPC transport (mmap-based)

Unix-socket IPC costs a syscall per message.
**Trigger**: a consumer hitting that ceiling, demonstrated with a benchmark.
Deferred for years on the reasonable grounds that nobody has.

---

## Waiting on upstream

### agnos `--agnos` build for the non-core profiles

`src/patra_queue.cyr` pulls patra, and `lib/patra.cyr` still references
`SYS_LSEEK` unguarded on the agnos target. The **core** profile
(`dist/majra.cyr`) has been agnos-clean since 2.5.0; only the `backends` profile
and a daemon `--agnos` build are affected.

**Blocked on**: patra guarding or replacing `SYS_LSEEK` for agnos.
**majra-side work when it lands**: none expected beyond re-running the matrix.

> Everything else that sat here has cleared. The sigil asm-drift SIGILL was
> dissolved by cyrius 6.x's `param_load` pseudo, and the agnosys `SYS_OPEN`
> aarch64 blocker went away transitively — both at 2.4.5. Noted because
> "waiting on upstream is nearly empty" is itself worth knowing.

---

## Non-goals

- **Application-level business logic** — majra provides primitives; consumers
  define semantics.
- **Message broker replacement** — in-process library first; `redis_backend`
  covers cross-process.
- **LLVM / Cargo dependency** — Cyrius compiles directly to machine code.
- **Reimplementing crypto primitives** — crypto goes through sigil. That
  includes Ed25519 verification strictness (RFC 8032 §5.1.7's `S < L` check,
  small-order point rejection): majra delegates, and records the delegation in
  [`threat-model.md`](threat-model.md) rather than duplicating it.

---

## Upstream cleanup (not majra work)

- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` was
  fixed in cyrius 5.4.10 but never moved to `issues/archived/` with a
  `— RESOLVED` suffix. Per that repo's `issues/README.md` lifecycle someone on
  the Cyrius side should archive it. Recorded here only so it is not lost.
