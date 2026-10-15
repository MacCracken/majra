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
| **2.10 line** | Larger capabilities, each taking the next MINOR as its trigger fires — or the next PATCH, where it adds no API. |
| **3.0.0** | The one thing 2.x has promised to break. |
| **Waiting on upstream** | Blocked outside this repo. Names the blocker. |
| **Non-goals** | Deliberately out of scope, recorded so the question stops recurring. |

> **Why these can't all be patch releases.** [`semver.md`](semver.md) reserves
> PATCH for bug fixes, performance and documentation with no API changes, so
> anything adding a public function takes the next MINOR. "The 2.10 line" below
> is therefore a *development line*, not a run of patch numbers — PostgreSQL
> SCRAM and QUIC cannot share one.

An item moves when its **trigger** fires — a consumer need, a dependency
landing, or a measurement crossing a threshold. Triggers are written down so
promotion is a decision rather than a mood.

---

## 2.10 line — larger capabilities

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

### agnos-portable `backends` and `patra-queue` suites

The four bundles cross-build clean for agnos (core since 2.5.0; `backends`
since 2.7.3 with one warning, and with none since the 6.6.6 pin closed the
`_agnos_getenv` gap upstream — 2.9.1). The **suites** are what cannot follow:
`tests/test_backends.tcyr` and `tests/test_patra_queue.tcyr` call `sys_unlink`
/ `sys_stat` with the Linux arity and name `SYS_SOCKETPAIR` / `SYS_RECVFROM` /
`SYS_GETSOCKNAME`, which the agnos peer does not declare (re-measured under
6.7.9 at 2.10.0, as at 6.6.20: core and expanded suites build `OK` with `--agnos`; the other
two fail at those sites). Until they build, "agnos-clean" is a claim about linking, not
about behaviour.

**Scope**: port the two suites' Linux-only call sites behind the per-target
wrappers or `#ifdef` arms (the shape `src/ipc.cyr` already uses), then add
`--agnos` to the cross-build gate next to `--aarch64`. No `src/` change.
**Aimed at**: the next PATCH — adds no API.
**Trigger**: a consumer builds the `backends` profile for agnos (the daemon
`--agnos` build that first exposed the profile), or the aarch64 run lane above
lands and the matrix grows a second non-x86 column anyway.

---

### Release functions for the blocks the API still hands out unpaired

The 2.10.1 P(-1) pass found the last returns a caller cannot release cleanly:

- **Bump vecs, which cannot be freed at all** — `hb_list_by_status`,
  `chb_list_by_status` (and `hb_online` / `chb_online`), `storage_list_defs`.
  A caller polling `chb_online` leaks a vec per call.
- **Freelist blocks with no documented release** — the barrier result from
  `barrier_arrive` / `_force` and `cbarrier_arrive` / `_force`, `relay_stats`,
  `fleet_stats`. `fl_free` works, but nothing says so.

**Scope**: build the four list returns on `_majra_vec_*` and add one release
for them (`majra_list_free`); add `barrier_result_free`, `relay_stats_free`,
`fleet_stats_free`, and document each pairing beside its constructor.
**Aimed at**: 2.11.0 — new public functions take a MINOR (`semver.md`).
**Trigger**: the next MINOR, or a consumer that polls one of the list calls.

---

### Retire `_majra_map_compact` at the ratelimit and heartbeat sites

cyrius 6.6.20's `lib/hashmap.cyr` counts `map_delete`'s tombstones and
rebuilds on a new-key insert, so under the pinned stdlib the sweep-time
compaction in `ratelimit_evict_stale`, `sliding_window_evict_stale` and both
heartbeat status sweeps never fires (the expanded suite has to starve those
maps by hand to reach it). It stays for now because a bundle compiles against
the *consumer's* stdlib, and on an older hashmap it is what keeps a churned
limiter from probing every slot under its lock. `relay.cyr`'s `seen` keeps it
regardless — its drops write tombstones the stdlib never counts.

**Scope**: delete the four call sites and the two hand-starve test halves that
exist only to reach them; keep the helper and relay's tests.
**Aimed at**: a PATCH — no API change.
**Trigger**: every consumer in [`state.md`](state.md)'s table pins cyrius
≥ 6.6.20, or 3.0.0, whichever is first.

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

Nothing at 2.10.0. The one item that sat here — the `_agnos_getenv` warning on
an agnos build of the `backends` profile — closed upstream: cyrius 6.6.6's
`lib/io.cyr` includes `lib/args_agnos.cyr` itself, and a clean-room probe of
`dist/majra-backends.cyr` plus its sidecar leaves cross-builds `OK` under
`--agnos` with **zero** undefined functions (2.9.1). The majra-side half moved
to the 2.10 line (agnos-portable backends + patra-queue suites).

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

- cyrius `lib/thread.cyr` `chan_new` multiplies `cap * 8` unchecked (found by
  the 2.8.1 sweep; the zero-allocation half is checked since 6.6.20 — the multiply still
  is not at 6.7.9).

- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` was
  fixed in cyrius 5.4.10 but never moved to `issues/archived/` with a
  `— RESOLVED` suffix. Per that repo's `issues/README.md` lifecycle someone on
  the Cyrius side should archive it. Recorded here only so it is not lost.
