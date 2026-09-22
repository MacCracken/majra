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
| **2.9 line** | Larger capabilities, each taking the next MINOR as its trigger fires — or the next PATCH, where it adds no API. |
| **3.0.0** | The one thing 2.x has promised to break. |
| **Waiting on upstream** | Blocked outside this repo. Names the blocker. |
| **Non-goals** | Deliberately out of scope, recorded so the question stops recurring. |

> **Why these can't all be patch releases.** [`semver.md`](semver.md) reserves
> PATCH for bug fixes, performance and documentation with no API changes, so
> anything adding a public function takes the next MINOR. "The 2.9 line" below
> is therefore a *development line*, not a run of patch numbers — PostgreSQL
> SCRAM and QUIC cannot share one.

An item moves when its **trigger** fires — a consumer need, a dependency
landing, or a measurement crossing a threshold. Triggers are written down so
promotion is a decision rather than a mood.

---

## 2.9 line — larger capabilities

Each takes the next available MINOR when its trigger fires — or the next
PATCH, where it adds no public API. The item whose trigger has already fired
comes first; the rest are ordered by expected value, not by commitment.

### aarch64 CI lane — build **and run** under `qemu-aarch64`

CI cross-**builds** the four suites for aarch64 and fails on any
`duplicate symbol 'SYS_` or raw-syscall diagnostic. That is the cheap belt and
all a build can give: of the three raw-syscall defects 2.7.3 fixed, two emitted
no build-time warning on any toolchain, so a build-only lane cannot see the
class it exists to catch. Every release since has run the suites under
`qemu-aarch64` by hand instead.

**Scope**: `qemu-user` on the runner, then `cyrius build --aarch64 --no-deps
<entry> build/<name>_aarch64 && qemu-aarch64 ./build/<name>_aarch64` for the
four suites, looped — recipe and the sequential-loop caveat in
[`testing.md`](../guides/testing.md).
**Aimed at**: the next PATCH — adds no API.
**Trigger**: already met. daimon ships a `daimon-aarch64` asset with
`dist/majra.cyr` inside it.

### PostgreSQL SCRAM-SHA-256 + `SSLRequest`

`postgres_backend.cyr` speaks **plaintext** and implements only
`AuthenticationCleartextPassword`, so the password and every query and result
row cross the wire in the clear. PostgreSQL has defaulted to `scram-sha-256`
since v14, so reaching a modern server means weakening `pg_hba.conf` to
`password` — which CI does today, and that is the tell. The connect fails
closed on auth type 10 rather than downgrading, so the gap is at least honest.
It remains the largest capability gap majra ships.

**Scope**: SCRAM-SHA-256 (SASL, RFC 5802) over sigil's `hmac_sha256` — the
`Hi()` iteration (PBKDF2-HMAC-SHA-256) is still not in sigil (3.12.18 ships
HKDF and Argon2; its only `PBKDF2` is a LUKS enum name), so it is either
majra-side code or a sigil ask — and `SSLRequest` (code 80877103) + TLS via
`lib/tls.cyr`.
**Trigger**: any consumer needing PostgreSQL over something other than loopback
or an already-confidential channel.

### QUIC transport

Unblocked on the sigil side — `x25519` is there.
**Trigger**: a consumer needing multiplexed streams or connection migration
that Unix-socket IPC and TCP do not cover. Scope it then: it is a large surface,
and the audits are a standing reminder that new wire parsers are where the bugs
are.

### Shared-memory IPC transport (mmap-based)

Unix-socket IPC costs a syscall per message.
**Trigger**: a consumer hitting that ceiling, demonstrated with a benchmark.
Deferred since the roadmap's first draft (2026-03) on the reasonable grounds
that nobody has.

---

## 3.0.0

### Remove the deprecated bare `ERR_*` codes

2.8.0 prefixed the 20 error codes `MAJRA_ERR_*` and kept every bare spelling as
a same-value alias so existing code kept compiling. The aliases go at 3.0.0 —
or one of them sooner, if a stdlib module starts declaring that exact name,
which [`semver.md`](semver.md) § Documented exceptions already permits.
`src/main.cyr`'s `test_error` pins every alias to its replacement until then,
and `cyrius lint` keeps noting them while they live.

**Trigger**: the 3.0.0 cut. Nothing else in 2.x is scheduled to break.

---

## Waiting on upstream

### agnos `--agnos` build for the non-core profiles

`src/patra_queue.cyr` pulls patra. The **core** profile (`dist/majra.cyr`) has
been agnos-clean since 2.5.0; only the `backends` profile and a daemon
`--agnos` build were ever affected, and `dist/majra-backends.cyr` plus its
sidecar leaves now cross-build `OK` under `--agnos`.

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
  around it with `_majra_map_compact`.

- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` was
  fixed in cyrius 5.4.10 but never moved to `issues/archived/` with a
  `— RESOLVED` suffix. Per that repo's `issues/README.md` lifecycle someone on
  the Cyrius side should archive it. Recorded here only so it is not lost.

## Moving the cyrius pin to 6.6.6

**Current pin: `cyrius = "6.6.4"` (cyrius.cyml:8).** Nothing has to change
first — bump the pin.

**⚠ majra is the one repo in this sibling slice that uses pair returns at
scale, and that is the 6.6.6 change to watch.** 6.6.6 turns "a pair-return fn
returning anything but a same-shaped pair" from a silent miscompile into a
compile error. `ret2` / `rethi` are used across `src/queue.cyr:46`,
`src/redis_backend.cyr` (155/159/162/166, read back at 239),
`src/postgres_backend.cyr` (612/614/618/625/632/643/660/671, read back at
719/802) and `src/admin.cyr:94`. Every one of those paths returns a two-value
pair on all branches today — **measured**: `cyrius build` under 6.6.4 and under
6.6.6 both exit 0 with the identical two diagnostics
(`lib/sigil.cyr:746 duplicate fn 'uname_release'`, and the 451,552-byte
static-data note). So this is a "a build that stops is the fix working" item
that does not stop majra's build — but it is now a hard error, so any future
`ret2` path that returns a single value on one branch will fail the build
instead of handing the caller a dropped tag.

**Windows: not exposed.** `src/ipc.cyr:157` is `#ifdef CYRIUS_TARGET_WIN
return -1;` — the IPC path declines on PE by design — and a grep for
`O_APPEND` / `O_TRUNC` across `src/`, `programs/` and `tests/` (excluding
vendored `lib/`) returns nothing. 6.6.6's PE append/truncate data-corruption fix
does not reach majra.

**Global redeclaration (6.6.6 makes "last definition wins" true from program
start, and a different type/size a compile error):** `_IPC_SYS_SENDTO` is
declared twice, `src/ipc.cyr:50` (x86, 44) and `:53` (aarch64, 206) — but inside
mutually exclusive `#ifdef CYRIUS_ARCH_X86` / `#ifdef CYRIUS_ARCH_AARCH64` arms,
so only one ever compiles. Not a redeclaration; neither rule fires. The
`sendto` / `fstatfs` collision notes at `src/ipc.cyr:45`, `src/ws.cyr:207`,
`src/postgres_backend.cyr:105` and `src/redis_backend.cyr:30` are comments about
the aarch64 remap and are unaffected — 6.6.6 names `SYS_STATFS` / `SYS_FSTATFS`
in the Linux peers, which does not touch majra's raw `sendto`.

**The rest of the 6.6.6 list is absent:** zero `struct` declarations, zero
`async` fns, zero `operator` fns, zero top-level `{ }` blocks, zero
`: cstring` parameters, no locally defined `vec_*` (so assert.cyr's new
transitive `include "lib/vec.cyr"` cannot collide — `vec` and `assert` are both
already in `[deps] stdlib`). `lib/regression.cyr` is vendored but majra calls no
`regression_*` helper, so the new exec deadline (`CYRIUS_CHECK_TIMEOUT`, 120 s)
changes nothing here.

**Verify after bumping:** re-run `cyrius deps` (6.6.6's `lib/io.cyr` is now
self-sufficient and `xrmdir` routes to `RemoveDirectoryW` on PE), then
`cyrius build` + `cyrius distlib` and confirm the diagnostic set is still the
same two lines.
