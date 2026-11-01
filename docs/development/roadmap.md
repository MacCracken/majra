# Majra Roadmap

**This file is forward-facing only.** What shipped lives in
[`CHANGELOG.md`](../../CHANGELOG.md); what is true *right now* — versions, pins,
test counts, bundle sizes — lives in [`state.md`](state.md). When an item here
ships, **delete it**; do not convert it into a changelog entry.

Every item below was re-checked against the 2.12.4 tree on 2026-10-31, after its
P(-1) pass: still open in the code, and — where it depends on upstream — still
open at the latest **tagged** cyrius (6.7.18) and sigil (3.13.15).

## The road to 3.0.0

Every open item is pinned to the release that ships it, ending at 3.0.0. The
order is the expected-value order the trigger-gated list already had. Each item
keeps the trigger it used to wait on, as the reason it exists; the schedule no
longer waits for one to fire.

| Release | Ships | Kind |
|---|---|---|
| **2.13.0** | PostgreSQL SCRAM-SHA-256 + `SSLRequest` / TLS, and a release function for the namespace builders | MINOR |
| last 2.13.x | P(-1) hardening pass | PATCH |
| **2.14.0** | WebSocket pub/sub bridge | MINOR |
| last 2.14.x | P(-1) hardening pass | PATCH |
| **2.15.0** | QUIC transport | MINOR |
| last 2.15.x | P(-1) hardening pass | PATCH |
| **2.16.0** | Shared-memory IPC transport, if its benchmark gate passes | MINOR |
| last 2.16.x | P(-1) hardening pass at the v3.0 cut | PATCH |
| **3.0.0** | The removals: bare `ERR_*`, `_metrics_arg3` / `_metrics_arg4`, `_majra_map_compact` at the ratelimit and heartbeat sites | MAJOR |

- **One capability per MINOR.** [`semver.md`](semver.md) reserves PATCH for
  fixes, performance and documentation with no API change. Each item that adds
  public API therefore takes a MINOR of its own. A release function for
  something majra already hands out is hygiene, not a capability: it rides the
  next MINOR, as 2.11.0's did.
- **A P(-1) patch closes every line.** CLAUDE.md § Process runs the hardening
  pass before any MINOR cut and at the v3.0 cut, and ships it as the line's last
  PATCH. 2.12's shipped as 2.12.4; the later ones get their numbers when their
  line's patches are known.
- **Other patches do not move the sequence.** Pin bumps, fixes and CI work take
  the next free 2.N.x on whatever line is current.
- **Pulling forward.** `_majra_map_compact` retires in the next PATCH if its
  consumer-pin trigger fires before 3.0.0.

---

## 2.13.0 — PostgreSQL SCRAM-SHA-256 + `SSLRequest`

`postgres_backend.cyr` speaks **plaintext** and implements only
`AuthenticationCleartextPassword`, so the password and every query and result
row cross the wire in the clear. PostgreSQL has defaulted to `scram-sha-256`
since v14, so reaching a modern server means weakening `pg_hba.conf` to
`password` — which CI does today, and that is the tell. The connect fails closed
on auth type 10 rather than downgrading, so the gap is at least honest. It is
the largest capability gap majra ships.

**Scope**: SCRAM-SHA-256 (SASL, RFC 5802) and `SSLRequest` (code 80877103) +
TLS via `lib/tls.cyr`. SCRAM's `Hi()` is PBKDF2-HMAC-SHA-256, which sigil still
does not ship (re-checked at 3.13.15: `hmac_sha256` yes, no PBKDF2) — so it is
majra-side code over `hmac_sha256`, or a sigil ask first.
**Done when**: CI's live job authenticates against `scram-sha-256` in
`pg_hba.conf`, over TLS. Today the job weakens that line to `password`.
**Pinned**: 2.13.0, first, because it is the largest capability gap and the only
security one. It had waited for a consumer needing PostgreSQL over anything other
than loopback or an already-confidential channel.

### Also in 2.13.0: release the namespace builders' strings

`namespace_topic`, `namespace_key` / `namespace_node_id`, `namespace_pattern` and
`namespace_wildcard` return `Str`s built with `str_builder` on the bump heap,
which never frees. There is no release function, so a consumer that namespaces
on every publish leaks every call. The 2.12.4 P(-1) pass found it
([`../audit/2026-10-31-audit.md`](../audit/2026-10-31-audit.md)); it is the class
2.11.0 closed for the list and stats functions.

**Scope**: build the result on the freelist and add `namespace_str_free`, or an
`_a` variant taking the caller's allocator. Either is new public API, which is why
it is not a patch.

---

## 2.14.0 — WebSocket pub/sub bridge

`src/ws.cyr` ships the RFC 6455 handshake and framing; there is no serve loop.
`ws_bridge_new` stores a hub and a connection cap that nothing reads, and a
consumer wanting fan-out drives `majra_ws_recv_frame` / `majra_ws_send_text`
from its own accept loop.

**Scope**: an accept / serve loop that subscribes each connection to the hub
and forwards matching messages as text frames, honouring `max_connections`,
with an idle timeout on the handshake read (today's byte-at-a-time read has
none) and UTF-8 validation on inbound text frames.
**Pinned**: 2.14.0. It had waited for a consumer that wants browser fan-out from
majra rather than its own loop.

---

## 2.15.0 — QUIC transport

Unblocked on the sigil side — `x25519` is there.

**Scope**: written first, as a note under [`../architecture/`](../architecture/)
and before any parser lands. The note settles which RFC 9000 / 9001 surface majra
carries and what it takes from sigil. It is a large surface, and the audits are a
standing reminder that new wire parsers are where the bugs are, so every parser
it adds ships with a fuzz harness.
**Pinned**: 2.15.0. It had waited for a consumer needing multiplexed streams or
connection migration that Unix-socket IPC and TCP do not cover.

---

## 2.16.0 — Shared-memory IPC transport (mmap-based)

Unix-socket IPC costs a syscall per message.

**Gate: benchmark first.** Bench Unix-socket IPC at the consumers' message sizes
against an mmap-ring prototype. If the syscall is not the bottleneck, the item
moves to Non-goals with the numbers, and the road goes from 2.15 straight to
3.0.0. Nobody has demonstrated that ceiling since the roadmap's first draft
(2026-03).
**Pinned**: 2.16.0, last, because it is the only item whose case is still
unmeasured.

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

Since 2.12.0 the four wider metrics hooks receive their third and fourth
arguments as parameters; the globals are still written only for callbacks that
read them, and are deprecated in [`semver.md`](semver.md). They go at 3.0.0.

### Retire `_majra_map_compact` at the ratelimit and heartbeat sites

cyrius 6.6.20's `lib/hashmap.cyr` counts `map_delete`'s tombstones and
rebuilds on a new-key insert, so under the pinned stdlib the sweep-time
compaction in `ratelimit_evict_stale`, `sliding_window_evict_stale` and both
heartbeat status sweeps never fires (2.12.0's `_ratelimit_evict_lru`, behind
both `*_evict_to` calls, carries the same call for the same reason) (the expanded suite starves those maps by
hand to reach it). It stays because a bundle compiles against the *consumer's*
stdlib, and on an older hashmap it is what keeps a churned limiter from probing
every slot under its lock. `relay.cyr`'s `seen` keeps it regardless — its drops
write tombstones the stdlib never counts.

**Scope**: delete the five call sites and the two hand-starve test halves that
exist only to reach them; keep the helper and relay's tests.
**Pinned**: 3.0.0 at the latest. **Pulled forward** to the next PATCH once every
consumer in [`state.md`](state.md)'s table pins cyrius ≥ 6.6.20. **Not met at
2.12.2** (local checkouts, 2026-10-30): daimon and hoosh pin 6.6.6, agnosai
6.6.14, stiva 6.6.10, ifran 6.6.2; sutra and secureyeoman carry no
`cyrius.cyml`.

**Gate**: the P(-1) pass at the v3.0 cut (the last 2.16.x). Nothing else in 2.x
is scheduled to break.

---

## Waiting on upstream

**Nothing blocks a pinned release.** Re-checked at 2.12.2 against cyrius 6.7.18
and sigil 3.13.15, and the patra item filed at 2.12.3:

- PBKDF2 for SCRAM is a *choice*, not a block: majra can build `Hi()` over
  `hmac_sha256` (see 2.13.0).
- **patra logs a failed before-image read as a zero page.** Filed 2026-10-31 as
  patra's `docs/development/issues/2026-10-31-wal-before-image-read-error-logged-as-zeros.md`.
  - An `EIO` on that read inside a transaction leaves an all-zero undo record.
    A rollback, or crash recovery, then writes zeros over the live page.
  - If that page is the header, the whole queue file is lost.
  - `patra_queue_enqueue`'s rollback path reaches it.
  - majra takes the fix with the first cyrius pin that folds it: a PATCH on the
    current line. No cyrius tag folds it yet (re-checked at 2.12.4).
  - If patra ships the read-fault seam the issue asks for, the patra-queue suite
    gains a rollback-after-read-fault test.

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
  the 2.8.1 sweep; still so at the 6.7.18 tag — the zero-allocation half has
  been checked since 6.6.20). majra is not exposed: `relay_with_capacity` caps a
  ring at 16,777,216 slots.
