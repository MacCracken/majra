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
| **2.9.0** | API and wire changes the 2.8.1 sweep deferred. |
| **2.8 line** | Larger capabilities, each taking the next MINOR as its trigger fires — or the next PATCH, where it adds no API. |
| **Waiting on upstream** | Blocked outside this repo. Names the blocker. |
| **Non-goals** | Deliberately out of scope, recorded so the question stops recurring. |

> **Why these can't all be patch releases.** [`semver.md`](semver.md) reserves
> PATCH for bug fixes, performance and documentation with no API changes, so
> anything adding a public function takes the next MINOR. "The 2.8 line" below
> is therefore a *development line*, not a run of patch numbers — PostgreSQL
> SCRAM and QUIC cannot share one.

An item moves when its **trigger** fires — a consumer need, a dependency
landing, or a measurement crossing a threshold. Triggers are written down so
promotion is a decision rather than a mood.

---

## 2.9.0

Fixes the 2.8.1 P(-1) sweep confirmed but could not take in a PATCH, because
each needs a new public symbol, a signature change or a wire change. Details
are in [`docs/audit/2026-09-15-audit.md`](../audit/2026-09-15-audit.md).

- **Encrypted IPC cross-connection replay.** The replay window is per handle,
  so a frame captured on one connection replays into a later one under the same
  PSK. The fix needs a handshake (per-connection key or session id), which is a
  wire change.
- **Signed envelopes: domain separation and anchored verify.** The signed bytes
  carry no version or domain prefix, and `signed_envelope_verify(se, 0)` returns
  "valid" for any re-signed envelope. Add the prefix, and make `expected_pk`
  mandatory or give the unanchored mode a distinct return code.
- **WebSocket Origin check.** `_ws_accept_upgrade` discards the request, so a
  consumer cannot reject a cross-site handshake. Expose the Origin header (or
  an allowlist) on the upgrade API.
- **Release functions for returned results**:
  - `pg_rows_free` / `redis_reply_free` for the result vecs that `pg_query`,
    `pg_exec` and Redis array replies return.
  - A way to free the heartbeat status-sweep transitions vec.
- **Relay subscribers**: `relay_unsubscribe`, and ownership rules for a
  `RelayMessage` delivered to several subscribers.
- **A distinct error code** for an encrypted-IPC frame that carries the
  receiver's own role (today it is `MAJRA_ERR_IPC`).

**Trigger**: already met. **Aimed at**: 2.9.0.

## 2.8 line — larger capabilities

Each takes the next available MINOR when its trigger fires — or the next
PATCH, where it adds no public API. An item whose trigger has already fired
comes first; the rest are ordered by expected value, not by commitment.

### aarch64 CI lane — build **and run** under `qemu-aarch64`

Porting is done and locally verified under `qemu-aarch64`
([CHANGELOG § 2.7.3](../../CHANGELOG.md)): 2.7.3 removed the last *unrouted*
raw x86_64 syscall numbers from arch-neutral code — the five networking
`var`s in `src/ipc.cyr` stay, all of them `ESYSXLAT` rows. CI now
cross-**builds** the four suites for aarch64 and fails on any
`duplicate symbol 'SYS_` or raw-syscall diagnostic; that is the cheap belt,
and it is all a build can give. What remains is the **run** half, and it is
the half that matters: two of the three raw-syscall sites 2.7.3 fixed emitted
no build-time warning on any toolchain, so a build-only lane proves nothing
about the class it exists to catch.

**Scope**: `qemu-user` on the runner, then `cyrius build --aarch64 --no-deps
<entry> build/<name>_aarch64 && qemu-aarch64 ./build/<name>_aarch64` for the
four suites; recipe in [`testing.md`](../guides/testing.md).
**Aimed at**: the next PATCH — adds no API.
**Trigger**: already met — daimon ships a `daimon-aarch64` asset with
`dist/majra.cyr` inside it, and that cross-build is what surfaced the
raw-syscall class.

### PostgreSQL SCRAM-SHA-256 + `SSLRequest`

`postgres_backend.cyr` speaks **plaintext** and implements only
`AuthenticationCleartextPassword`, so the password and every query and result
row cross the wire in the clear. PostgreSQL has defaulted to `scram-sha-256`
since v14, so reaching a modern server means weakening `pg_hba.conf` to
`password` — which CI does today, and that is the tell.

2.6.9 made the connect **fail closed** on auth type 10 rather than downgrade,
so the current state is at least honest. It remains the largest capability gap
majra ships.

**Scope**: SCRAM-SHA-256 (SASL, RFC 5802) over sigil's `hmac_sha256` — the
`Hi()` iteration (PBKDF2-HMAC-SHA-256) is not in sigil 3.12.18, which ships
HKDF and Argon2 but no PBKDF2, so it is either majra-side code or a sigil ask —
and `SSLRequest` (code 80877103) + TLS via `lib/tls.cyr`.
**Trigger**: any consumer needing PostgreSQL over something other than loopback
or an already-confidential channel.

### QUIC transport

Unblocked on the sigil side — X25519 has been available since 3.7.8.
**Trigger**: a consumer needing multiplexed streams or connection migration
that Unix-socket IPC and TCP do not cover. Scope it then: it is a large surface,
and the audit is a standing reminder that new wire parsers are where the bugs
are.

### Shared-memory IPC transport (mmap-based)

Unix-socket IPC costs a syscall per message.
**Trigger**: a consumer hitting that ceiling, demonstrated with a benchmark.
Deferred since the roadmap's first draft (2026-03) on the reasonable grounds
that nobody has.

---

## Waiting on upstream

### agnos `--agnos` build for the non-core profiles

`src/patra_queue.cyr` pulls patra. The **core** profile (`dist/majra.cyr`)
has been agnos-clean since 2.5.0; only the `backends` profile and a daemon
`--agnos` build were ever affected. The blocker this item was filed on at
2.5.0 — `lib/patra.cyr` referencing `SYS_LSEEK` unguarded on agnos — has
cleared: patra 1.14.3 (the 6.6.4 fold) takes `SYS_LSEEK` from the agnos
syscall peer, which supplies #58, and at 2.7.3 `dist/majra-backends.cyr` plus
its sidecar leaves cross-builds `OK` under `--agnos`.

**Blocked on**: what is left is a warning, not an error — that fold reports
`undefined function '_agnos_getenv'` (`lib/io.cyr`'s agnos `getenv` reaches
`lib/args_agnos.cyr`, which no sidecar names; a trapping `ud2` if reached).
Whether that is a `[deps].stdlib` declaration on majra's side, as `sys` was at
2.7.3, or an `io.cyr` include upstream is the question to settle before this
item is re-homed.
**majra-side work when it lands**: the `backends` and `patra-queue` suites do
not cross-build for agnos — they call `sys_unlink` / `sys_stat` with the Linux
arity and `syscall(SYS_SOCKETPAIR, …)`, which has no agnos row; port those,
then re-run the matrix.

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

- cyrius `lib/thread.cyr` `chan_new` multiplies `cap * 8` unchecked and does not
  check for a zero allocation (found by the 2.8.1 sweep). `lib/hashmap.cyr`
  never reclaims tombstones, so delete-heavy maps probe ever longer; majra works
  around it with compaction.

- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` was
  fixed in cyrius 5.4.10 but never moved to `issues/archived/` with a
  `— RESOLVED` suffix. Per that repo's `issues/README.md` lifecycle someone on
  the Cyrius side should archive it. Recorded here only so it is not lost.
