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
| **2.8.0** | Committed to this cut. |
| **2.7 line** | Larger capabilities, each taking the next MINOR as its trigger fires — or the next PATCH, where it adds no API. |
| **Waiting on upstream** | Blocked outside this repo. Names the blocker. |
| **Non-goals** | Deliberately out of scope, recorded so the question stops recurring. |

> **Why these can't all be patch releases.** [`semver.md`](semver.md) reserves
> PATCH for bug fixes, performance and documentation with no API changes, so
> anything adding a public function takes the next MINOR. "The 2.7 line" below
> is therefore a *development line*, not a run of patch numbers — PostgreSQL
> SCRAM and QUIC cannot share one.

An item moves when its **trigger** fires — a consumer need, a dependency
landing, or a measurement crossing a threshold. Triggers are written down so
promotion is a decision rather than a mood.

---

## 2.8.0

### Prefix the error enums — `ERR_*` → `MAJRA_ERR_*`

`src/error.cyr` declares its 20 error codes bare: `enum MajraErr` (`ERR_NONE`
… `ERR_WORKFLOW_STORAGE`) and `enum IpcErr` (`ERR_IPC_FRAME_TOO_LARGE` …
`ERR_IPC_JSON`). Under the 6.6.4 pin, `cyrius lint` notes every one of them:
bare `ERR_*` is reserved for the sakshi base logger, and a leaf library must
prefix its error enum, because enum constants share one flat namespace across
libraries (the linter cites cyrius proposal
`2026-07-11-error-enum-namespace-lint-gate`). The notes are advisory — lint
still reports 0 warnings — and nothing collides today: none of majra's 20 names
is among the 29 bare `ERR_*` names that `lib/sakshi.cyr` and `lib/sankoch.cyr`
declare in the 6.6.4 snapshot. This is the `_sub_new` / `sha1` class from 2.7.1,
caught before it collides rather than after.

**Scope**:
- Add `MAJRA_ERR_*` for all 20 codes, with **the same values**.
- Move every in-tree use to the new names: `src/barrier.cyr`, `src/ipc.cyr`,
  `src/ipc_encrypted.cyr`, `src/main.cyr` and `tests/test_core.tcyr`.
- Keep each bare `ERR_*` as a deprecated alias with the same value. One enum
  can hold both spellings; a probe under 6.6.4 builds and compares them equal.
- Regenerate the four bundles, and record the deprecation in `CHANGELOG.md` and
  [`semver.md`](semver.md).

⚠ **The aliases are what make this a MINOR.** [`semver.md`](semver.md) promises
that a MINOR compiles existing code unchanged. Adding constants is allowed;
renaming them is not. The collision exception there doesn't cover a straight
rename either, because its condition 1 needs the definitions to actually
disagree, and today they don't. So removing the bare names waits for 3.0.0,
unless a stdlib module first declares one of these exact names. At that point
the exception applies and that name may go in whatever cut is in flight. While
the aliases remain, lint keeps noting them. The notes go away only when the
aliases do.

**Aimed at**: 2.8.0.
**Trigger**: already met. The lint gate names this pattern, and every majra
consumer that also includes sakshi shares the namespace.

### Rename out of the `lib/ws.cyr` collision — `ws_send_text` / `ws_recv_frame`

Two public functions in `src/ws.cyr` share their names with `lib/ws.cyr` in the
6.6.4 snapshot. They disagree in parameter count and contract:

| name | majra (`src/ws.cyr`) | stdlib (`lib/ws.cyr`) |
|---|---|---|
| `ws_send_text` | `(fd, data, len)` → 0 / -1 | `(ws, msg)` → bytes written; length from `strlen` |
| `ws_recv_frame` | `(fd)` → frame struct, released with `ws_frame_free` | `(ws, opcode_out, len_out)` → payload pointer |

A consumer that links both gets whichever definition came last in include
order. This has been open since 2.7.1, and until now it was tracked only as a
CHANGELOG known issue and a [`state.md`](state.md) blocker. It only affects
the `backends` profile, the one bundle that carries `src/ws.cyr`.

**Scope**: rename to `majra_ws_send_text` / `majra_ws_recv_frame`, following the
2.6.8 `majra_base64_*` precedent. Update the in-tree references: the two calls
in `tests/test_backends.tcyr`, the header comments in `src/ws.cyr`, the README
module table, and the data-flow line in
[`overview.md`](../architecture/overview.md). Add both rows to the renames table
in [`semver.md`](semver.md) with a migration note, and regenerate the four
bundles. Leave the other `ws_*` names (`ws_frame_*`, `ws_send_close`,
`ws_send_pong`, `ws_bridge_*`) alone. None of them collides today, and renaming
them would be an ordinary break, not an exception.

⚠ **No aliases here, unlike the `ERR_*` item above.** Keeping the old name *is*
the collision. That's fine: this rename qualifies for the semver.md collision
exception, and a probe under 6.6.4 confirms both of its conditions:

1. The two definitions disagree, per the table.
2. A leftover call fails to compile. Against `lib/ws.cyr` it is
   `'ws_send_text' expects 2 arguments, got 3` and `'ws_recv_frame' expects 3
   arguments, got 1`. Without `lib/ws.cyr` it is a reachable undefined function,
   which cyrius refuses to emit.

So no consumer can pick up the change silently. The exception would allow this
in a PATCH; it rides 2.8.0 to ship alongside the `ERR_*` deprecation.

**Aimed at**: 2.8.0.
**Trigger**: already met. The collision exists in the pinned snapshot.

---

## 2.7 line — larger capabilities

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

- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` was
  fixed in cyrius 5.4.10 but never moved to `issues/archived/` with a
  `— RESOLVED` suffix. Per that repo's `issues/README.md` lifecycle someone on
  the Cyrius side should archive it. Recorded here only so it is not lost.
