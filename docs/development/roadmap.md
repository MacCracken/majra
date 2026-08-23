# Majra Roadmap

**This file is forward-facing only.** What shipped lives in
[`CHANGELOG.md`](../../CHANGELOG.md); what is true *right now* — versions, pins,
test counts, bundle sizes — lives in [`state.md`](state.md). When an item here
ships, **delete it**; do not convert it into a changelog entry.

## How to read this

| Bucket | Meaning |
|---|---|
| **Now** | Committed for the next cut. Scope is settled and there is a definition of done. |
| **Next** | Agreed and scheduled behind *Now*. Scope understood; work not started. |
| **Backlog** | Real work with a real reason, but no trigger yet. Each entry names what would promote it. |
| **Waiting on upstream** | Blocked on cyrius, sigil, or patra. Names the blocker. |
| **Non-goals** | Deliberately out of scope, recorded so the question stops recurring. |

An item earns promotion when its **trigger** fires — a consumer need, a
dependency landing, or a measurement crossing a threshold. Triggers are written
down so promotion is a decision rather than a mood.

---

## Now

### Second P(-1) pass before the 2.7.0 cut

[`CLAUDE.md`](../../CLAUDE.md) P(-1) step 10 says *"repeat if heavy — keep
drilling until the pass is genuinely clean, not just no errors."* The
[2026-08-22 pass](../audit/2026-08-22-audit.md) returned **115 confirmed
findings**, which is heavy on any reading, and it rewrote enough code that the
rewrites themselves are now unaudited.

**Done when**: a second pass over the modules 2.6.9 changed most heavily
returns no new high-severity findings.

**Re-examine specifically** — all introduced by 2.6.9, none yet audited:

- the replay window in `encrypted_ipc_recv`: counter monotonicity across a
  rekey, and whether a peer can wedge it
- the new bounds arithmetic in the DataRow and RESP parsers — that is exactly
  where the previous bugs lived
- `relay_send` and `relay_receive_ex` now hold the relay mutex across the
  fan-out, which is safe *only* while every send path stays non-blocking
- `mq_dequeue`'s cancelled-job skip loop, which can now iterate

### Consumer migration guide for the 2.6.9 breaking changes

2.6.9 changed three public signatures plus two behaviours. Consumers pinning it
need a recipe, in the shape of the existing `docs/guides/migration-*.md` files.

**Done when** it covers: `encrypted_ipc_new` (new required role argument),
`majra_admin_serve` (address is now a parsed dotted-quad string),
`transport_send`/`transport_recv` (all three arguments now forwarded),
`namespace_new` (now rejects `/`, `:`, `#`, `+`, control bytes), and the
`patra_queue` `STR` → `TEXT` schema change — which needs an explicit "delete or
migrate the existing `.patra` file" step, since an old file silently keeps its
255-byte cap.

---

## Next

### pubsub unsubscribe + per-subscriber lag policy

The deferral 2.6.9 documented most carefully. Fan-out is a **blocking
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

**Trigger**: any consumer creating subscriptions dynamically. Today's consumers
subscribe at startup and hold for process life, which is why this has not bitten.

### PostgreSQL SCRAM-SHA-256 + `SSLRequest`

`postgres_backend.cyr` speaks **plaintext** and implements only
`AuthenticationCleartextPassword`, so the password and every query and result
row cross the wire in the clear. PostgreSQL has defaulted to `scram-sha-256`
since v14, so reaching a modern server means weakening `pg_hba.conf` to
`password` — which CI does today, and that is the tell.

2.6.9 made the connect **fail closed** on auth type 10 rather than downgrade,
so the current state is at least honest. It remains a real capability gap.

**Scope**: SCRAM-SHA-256 (SASL, RFC 5802) over sigil's HMAC-SHA256 + PBKDF2,
and `SSLRequest` (code 80877103) + TLS via `lib/tls.cyr`.

**Trigger**: any consumer needing PostgreSQL over something other than loopback
or an already-confidential channel.

---

## Backlog

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

**Scope**: thread-per-step within a tier, bounded pool, and a decision on how a
failing step affects its siblings.
**Trigger**: a workflow whose tier width and per-step latency make serial
execution the bottleneck. Needs a measurement, not an intuition.

### QUIC transport

Unblocked on the sigil side — X25519 has been available since 3.7.8.
**Trigger**: a consumer needing multiplexed streams or connection migration
that Unix-socket IPC and TCP do not cover. Scope it then: it is a large surface,
and the audit is a standing reminder that new wire parsers are where the bugs
are.

### aarch64 cross-build wiring

The historical `SYS_OPEN` blocker is long gone (agnosys 1.3.2). Wiring and
verifying a `cyrius build --aarch64` CI step is a discrete, unblocked
*verification* task — not a porting one.
**Trigger**: any non-x86_64 consumer. All current consumers are
x86_64-server-side, so this stays low priority.

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
