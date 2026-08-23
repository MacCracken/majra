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
| **2.7 line** | Larger capabilities, each taking the next MINOR as its trigger fires. |
| **Waiting on upstream** | Blocked outside this repo. Names the blocker. |
| **Non-goals** | Deliberately out of scope, recorded so the question stops recurring. |

> **Why these can't all be patch releases.** [`semver.md`](semver.md) reserves
> PATCH for "bug fixes, performance, documentation — no API changes", so
> anything adding a public function takes the next MINOR. "The 2.7 line" below
> is therefore a *development line*, not a run of patch numbers — PostgreSQL
> SCRAM and QUIC cannot share one.

An item moves when its **trigger** fires — a consumer need, a dependency
landing, or a measurement crossing a threshold. Triggers are written down so
promotion is a decision rather than a mood.

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
