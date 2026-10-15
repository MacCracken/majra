# Majra Roadmap

**This file is forward-facing only.** What shipped lives in
[`CHANGELOG.md`](../../CHANGELOG.md); what is true *right now* — versions, pins,
test counts, bundle sizes — lives in [`state.md`](state.md). When an item here
ships, **delete it**; do not convert it into a changelog entry.

Every item below was re-checked against the 2.11.0 tree on 2026-10-15: still
open in the code, and — where it depends on upstream — still open at the latest
**tagged** cyrius (6.7.11) and sigil (3.13.12).

## How to read this

Work is scheduled against **release targets**, not priority labels. Every item
names the version it is aimed at and the **trigger** that moves it — a consumer
need, a dependency landing, or a measurement crossing a threshold — so
promotion is a decision rather than a mood.

| Target | What goes there |
|---|---|
| **2.12.0** | The next MINOR: work whose trigger has fired and that adds public API. |
| **2.12.x** | PATCHes after it: work that adds no API (CI, tests, toolchain, internal). |
| **Later MINORs (2.13+)** | Larger capabilities, each taking the next MINOR when its trigger fires. |
| **3.0.0** | What 2.x has promised to break, and the fallback for anything gated on consumers. |
| **Waiting on upstream** | Blocked outside this repo; names the blocker. |
| **Non-goals** | Deliberately out of scope, so the question stops recurring. |

> **Why the split.** [`semver.md`](semver.md) reserves PATCH for fixes,
> performance and documentation with no API change, so anything that adds a
> public function takes a MINOR. Two capabilities never share a MINOR by
> accident — each later-MINOR item takes the next one when it is ready.

---

## 2.12.0 — the next MINOR

### Rate limiter: a force-evict that can hold a hard key cap

Filed by agnosai on 2026-09-26:
[`issues/2026-09-26-agnosai-ratelimit-no-way-to-enforce-a-key-cap.md`](issues/2026-09-26-agnosai-ratelimit-no-way-to-enforce-a-key-cap.md).
Since 2.8.1, `ratelimit_evict_stale` evicts only buckets that have refilled to
burst. That is the right metering rule, but it means a sprayed key (one request,
one token spent) is unevictable for `1/rate` seconds, so a consumer that bounds
the map under key-spraying cannot hold its cap: 72,447 tracked keys against
agnosai's 4,096 at 1 req/s. **Still open at 2.11.0** — no force-evict, key-cap
or evict-to-count call exists in `src/ratelimit.cyr`.

**Scope**: the issue's option 1 (`ratelimit_evict_stale_force(rl, max_idle_ns)`,
today's walk without the refill gate) or option 2 (`ratelimit_evict_to(rl,
max_keys)`, least-recently-seen first). Leave `ratelimit_evict_stale` exactly as
it is. Carry the issue's §4 checks: the repro evicts all 20,000 through the new
call and 0 through the old one; a force-evicted drained key gets a fresh burst,
documented on the new call so nobody uses it on the idle path. Archive the issue
on ship.
**Trigger**: met — a consumer filed it and will adopt it.

### Metrics: pass hook arguments as arguments, not through globals

`metrics_queue_state_changed`, `metrics_heartbeat_transition`,
`metrics_barrier_arrived` and `metrics_workflow_step_finished` carry their
third and fourth arguments in `_metrics_arg3` / `_metrics_arg4` — unsynchronised
file-scope globals that two threads dispatching at once overwrite. Filed LOW by
the [2026-08-22 pass-2 audit](../audit/2026-08-22-audit-pass2.md) and never
scheduled; **still so at 2.11.0**. majra itself never dispatches those four
hooks, so the exposure is consumers calling them from more than one thread. The
globals predate `fncall3` / `fncall4`, which the pinned `lib/fnptr.cyr` ships.

**Scope**: dispatch through `fncall3` / `fncall4` so a callback can take the
extras as parameters; keep writing the globals for callbacks that read them, and
document them as deprecated. Removing them is a 3.0.0 item.
**Trigger**: met — an audit finding with nothing upstream in the way.

---

## 2.12.x — patches (no API)

### aarch64 CI lane — build **and run** under `qemu-aarch64`

CI cross-**builds** the four suites for aarch64 and fails on any
`duplicate symbol 'SYS_` or raw-syscall diagnostic. That is the cheap belt and
all a build can give: of the three raw-syscall defects 2.7.3 fixed, two emitted
no build-time warning on any toolchain, so a build-only lane cannot see the
class it exists to catch. Most releases since have run the suites under
`qemu-aarch64` by hand (2.11.0 last). **Still open at 2.11.0**: no CI job installs or runs
`qemu-user`.

**Scope**: `qemu-user` on the runner, then `cyrius build --aarch64 --no-deps
<entry> build/<name>_aarch64 && qemu-aarch64 ./build/<name>_aarch64` for the
four suites, looped — recipe and the sequential-loop caveat in
[`testing.md`](../guides/testing.md).
**Trigger**: met — daimon ships a `daimon-aarch64` asset with `dist/majra.cyr`
inside it.

### agnos-portable `backends` and `patra-queue` suites

The four bundles cross-build clean for agnos. The **suites** cannot follow:
`tests/test_backends.tcyr` and `tests/test_patra_queue.tcyr` call `sys_unlink`
/ `sys_stat` with the Linux arity and name `SYS_SOCKETPAIR` / `SYS_RECVFROM` /
`SYS_GETSOCKNAME`, which the agnos peer does not declare. **Re-measured at
2.11.0 (cyrius 6.7.9)**: core and expanded build `OK` with `--agnos`; backends
stops on 27 errors and patra-queue on 6. Until they build, "agnos-clean" is a
claim about linking, not about behaviour.

**Scope**: port the two suites' Linux-only call sites behind the per-target
wrappers or `#ifdef` arms (the shape `src/ipc.cyr` already uses), then add
`--agnos` to the cross-build gate next to `--aarch64`. No `src/` change.
**Trigger**: a consumer builds the `backends` profile for agnos, or the aarch64
run lane above lands and the matrix grows a second non-x86 column anyway.

### Move the cyrius pin past 6.7.9

The one upstream warning every signed / admin / backends build prints — sigil's
over-budget `var buf[262144]` ("array local … gets STATIC storage") — is gone
from sigil **3.13.12**, which cyrius folds from **6.7.10**. 6.7.10 and 6.7.11
are tagged; 6.7.12 is still in progress.

**Scope**: the pin-bump procedure (baseline under the old pin, snapshot and fold
diff, `ESYSXLAT` re-derivation, the 6.7.x `ecosystem-migration.md` entries
grepped across the tree, loops natively and under qemu, interleaved A/B
benches), then drop the warning from `state.md`, `dependency-watch.md` and the
CHANGELOG's known issues.
**Trigger**: the next toolchain bump — take the newest *tagged* release at the
time.

### Retire `_majra_map_compact` at the ratelimit and heartbeat sites

cyrius 6.6.20's `lib/hashmap.cyr` counts `map_delete`'s tombstones and
rebuilds on a new-key insert, so under the pinned stdlib the sweep-time
compaction in `ratelimit_evict_stale`, `sliding_window_evict_stale` and both
heartbeat status sweeps never fires (the expanded suite starves those maps by
hand to reach it). It stays because a bundle compiles against the *consumer's*
stdlib, and on an older hashmap it is what keeps a churned limiter from probing
every slot under its lock. `relay.cyr`'s `seen` keeps it regardless — its drops
write tombstones the stdlib never counts.

**Scope**: delete the four call sites and the two hand-starve test halves that
exist only to reach them; keep the helper and relay's tests.
**Trigger**: every consumer in [`state.md`](state.md)'s table pins cyrius
≥ 6.6.20 — or 3.0.0, whichever is first.

---

## Later MINORs (2.13+) — trigger-gated capabilities

Ordered by expected value, not by commitment. None has a met trigger.

### PostgreSQL SCRAM-SHA-256 + `SSLRequest`

`postgres_backend.cyr` speaks **plaintext** and implements only
`AuthenticationCleartextPassword`, so the password and every query and result
row cross the wire in the clear. PostgreSQL has defaulted to `scram-sha-256`
since v14, so reaching a modern server means weakening `pg_hba.conf` to
`password` — which CI does today, and that is the tell. The connect fails closed
on auth type 10 rather than downgrading, so the gap is at least honest. It is
the largest capability gap majra ships.

**Scope**: SCRAM-SHA-256 (SASL, RFC 5802) and `SSLRequest` (code 80877103) +
TLS via `lib/tls.cyr`. SCRAM's `Hi()` is PBKDF2-HMAC-SHA-256, which sigil still
does not ship (re-checked at 3.13.12: `hmac_sha256` yes, no PBKDF2) — so it is
majra-side code over `hmac_sha256`, or a sigil ask first.
**Trigger**: a consumer needing PostgreSQL over anything other than loopback or
an already-confidential channel.

### WebSocket pub/sub bridge

`src/ws.cyr` ships the RFC 6455 handshake and framing; there is no serve loop.
`ws_bridge_new` stores a hub and a connection cap that nothing reads, and a
consumer wanting fan-out drives `majra_ws_recv_frame` / `majra_ws_send_text`
from its own accept loop.

**Scope**: an accept / serve loop that subscribes each connection to the hub
and forwards matching messages as text frames, honouring `max_connections`,
with an idle timeout on the handshake read (today's byte-at-a-time read has
none) and UTF-8 validation on inbound text frames.
**Trigger**: a consumer that wants browser fan-out from majra rather than its
own loop.

### QUIC transport

Unblocked on the sigil side — `x25519` is there.
**Trigger**: a consumer needing multiplexed streams or connection migration
that Unix-socket IPC and TCP do not cover. Scope it then: it is a large surface,
and the audits are a standing reminder that new wire parsers are where the bugs
are.

### Shared-memory IPC transport (mmap-based)

Unix-socket IPC costs a syscall per message.
**Trigger**: a consumer hitting that ceiling, demonstrated with a benchmark.
Deferred since the roadmap's first draft (2026-03) because nobody has.

---

## 3.0.0

### Remove the deprecated bare `ERR_*` codes

2.8.0 prefixed the error codes `MAJRA_ERR_*` and kept every bare spelling (20 of
them at 2.11.0) as a same-value alias so existing code kept compiling. The
aliases go at 3.0.0 — or one of them sooner, if a stdlib module starts declaring
that exact name, which [`semver.md`](semver.md) § Documented exceptions already
permits. `src/main.cyr`'s `test_error` pins every alias to its replacement until
then, and `cyrius lint` notes them while they live.

### Remove `_metrics_arg3` / `_metrics_arg4`

Once 2.12.0 passes the extras as arguments, the globals exist only for
callbacks that still read them; they go at 3.0.0.

### Fallbacks that land here at the latest

- **Retire `_majra_map_compact`** at the ratelimit and heartbeat sites, if the
  consumer-pin trigger (2.12.x above) has not fired first.

**Trigger**: the 3.0.0 cut. Nothing else in 2.x is scheduled to break.

---

## Waiting on upstream

**Nothing blocks a majra item.** Re-checked at 2.11.0 against cyrius 6.7.11
and sigil 3.13.12:

- sigil's static-buffer warning is **fixed upstream** (sigil 3.13.12, cyrius
  6.7.10) — majra adopts it with the pin move under 2.12.x.
- PBKDF2 for SCRAM is a *choice*, not a block: majra can build `Hi()` over
  `hmac_sha256` (see the SCRAM item).

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
  the 2.8.1 sweep; still so at the 6.7.11 tag — the zero-allocation half has
  been checked since 6.6.20). majra is not exposed: `relay_with_capacity` caps a
  ring at 16,777,216 slots.
