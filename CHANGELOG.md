# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.6.7] — 2026-08-20 — the last folded-module pin that lagged the toolchain

**410** assertions green across four suites (core 150, expanded 200, backend 43,
patra-queue 17), 0 failed. Dep hashes 108 verified / 0 failed. Fuzz, benchmarks
and examples clean.

### Changed — `[deps.sigil]` 3.12.7 → 3.12.9

**This is the fix.** sigil is a *folded* stdlib module, and `cyrius deps`
applies a declared dep's copy on top of the `lib sync --full` snapshot on every
resolve. A `[deps.sigil]` behind the fold therefore downgrades `lib/sigil.cyr`
for every transitive consumer — and majra sits under both **agnosai** and
**bote**.

It was never actually observed downgrading anything: agnosai pins sigil 3.12.9
directly and libro pins 3.12.9, so in practice one of those won and
`lib/sigil.cyr` resolved to 3.12.9 everywhere checked. But that is resolution
*order* doing the work, not correctness — the identical shape, in patra, is what
broke agnosai's CI at 2.0.2 and took four repos to trace.

Found by sweeping the whole dependency closure — every `[deps.X]` where X is a
folded stdlib module, compared against what the pinned toolchain ships. This was
the only remaining mismatch of five such declarations.

### Changed — Cyrius pin 6.5.20 → 6.5.31

Eleven minors behind. Picks up the folds shipped since (sakshi 2.4.11, patra
1.13.9, yukti 2.3.8, niyama 1.0.7, mabda 4.1.0, ganita 1.1.4, yantra 1.0.3).

`src/ws.cyr` reformatted for 6.5.31's canonical continuation indent — 0/23 files
were unformatted under 6.5.20, 1/23 under 6.5.31, so this is the formatter
moving, not drift. `git diff -w -- src/` is empty. Only `dist/majra-backends.cyr`
changed as a result, since the base/signed/admin profiles deliberately exclude
the backend modules.

### Fixed — `version-bump.sh` told you to regenerate 2 of 4 bundles

Its next-steps line read `cyrius distlib && cyrius distlib backends`, omitting
`signed` and `admin`. The CI gate checks all four and fails on any stale bundle,
so following the script's own instructions produced a red build. Corrected to
list every profile.

## [2.6.6] — 2026-08-13 — the relay's fan-out could wedge its sender

### Fixed

- **relay** — **a full subscriber ring blocked the SENDER, forever.** Both
  fan-out paths pushed with `chan_send`, which futex-waits for space; Rust's
  `Relay` is a `tokio::sync::broadcast` and `Relay::send` is
  `let _ = self.tx.send(..)` (`fleet/relay.rs:133`) — it **never blocks**. A full
  ring overwrites its oldest slot and the lagging receiver observes
  `RecvError::Lagged`. Now `chan_try_send`, so a full ring drops rather than
  waits.

  ⚠ **2.6.5 is what made this reachable.** Before it, every subscriber channel
  was 256 deep regardless of the capacity asked for, so wedging needed 256
  undrained messages; honouring the capacity made it reachable at whatever depth
  the caller names — `relay_with_capacity(id, 2)` deadlocked on the **third**
  send. Reproduced directly: three sends against a depth-2 relay never returned,
  killed by `timeout`. majra has no unsubscribe, so the wedge is permanent
  rather than transient, and an agnosai caller wedges a pooled worker thread.

  Found by an adversarial review of the 2.6.5 change set, not by the suite —
  the 2.6.5 capacity test filled the ring with `chan_try_send` **directly on the
  channel**, routing around `relay_send` and so around the blocking call.

### Added

- A deadlock guard: a depth-2 relay takes a third and fourth `relay_send` and
  both return. Mutation-verified — restoring `chan_send` hangs the suite
  (`timeout` rc 124) rather than failing an assertion.

## [2.6.5] — 2026-08-13 — the relay's capacity was discarded and its timestamp was unportable

### Fixed

- **relay** — **`capacity` was accepted and thrown away.** `relay_subscribe`
  hardcoded `chan_new(256)`, so a relay built asking for 4 and one asking for
  4096 behaved identically; Rust's `Relay::new` sizes its `broadcast::channel`
  from the argument. New `relay_with_capacity(node_id, capacity)` and
  `relay_capacity(r)`; `relay_new` delegates with the 256 default, so nothing
  that does not ask for a depth changes. A non-positive capacity falls back
  rather than building a channel nothing could be sent through.

- **relay** — **the message timestamp was CLOCK_MONOTONIC.** A RelayMessage is
  serialised and sent to another node, and Rust stamps it `DateTime<Utc>` — a
  portable instant. `time_now_ns()` measures from an arbitrary per-boot zero, so
  the number carried on the wire was meaningless outside the emitting process:
  two nodes could not order each other's messages by it, and a persisted one
  could not be read back after a reboot. Now `clock_epoch_ns()`.

  ⚠ **This changes what the field MEANS, not any decision majra makes.** Dedup
  and ordering are by `seq`, and nothing in the module compares timestamps.

  ⚠ **The wall-clock reader is majra's own `time_epoch_ns` (`src/envelope.cyr`),
  NOT the stdlib's `clock_epoch_ns`.** `chrono` is not among the modules a
  `--no-deps` build prepends, and `--no-deps` is exactly what CI passes — so the
  stdlib call compiled locally, where a full `lib sync` had put `chrono.cyr` on
  disk, and failed in CI with `undefined function 'clock_epoch_ns'`. Adding
  `chrono` to `[deps] stdlib` does **not** fix it; that list controls
  provisioning, not what a `--no-deps` build has in scope. `time_epoch_ns` sits
  beside the `time_now_ns` that already does this dance and differs only in the
  clock id (`CLOCK_REALTIME` rather than `CLOCK_MONOTONIC`), so majra's time
  sources stay in one place and no dependency is added.

  Both reported by agnosai 2026-08-13, which had carried them in
  `src/fleet/relay.cyr` as "owed to majra".

### Added

- `relay_msg_timestamp(m)` — there was no accessor for offset 40 at all.
- `RELAY_DEFAULT_CAPACITY` (256), named rather than repeated.
- 9 relay regression tests (`test_core` **189 -> 198**). Mutation-verified:
  restoring the hardcoded `chan_new(256)` and reverting to `time_now_ns()` each
  fail loudly. A depth of 2 is the capacity discriminator — the third send with
  nobody draining must be refused, where 256 swallows it.

### Changed

- The `Relay` struct grows **96 -> 104 bytes**; `capacity` is **appended**, so no
  existing field offset moves.

## [2.6.4] — 2026-08-13 — the rate limiter never refused anything

### Fixed

- **ratelimit** — **a key built per request got a fresh full-burst bucket every
  time, so neither limiter refused anything.** `ratelimit_check` and
  `sliding_window_check` store the caller's key pointer directly
  (`map_set` is `store64(ep, key)` — a borrow, not a copy), and the entry lives
  until eviction. Every real caller derives its key per request — a header, a
  peer address, a token — so the map was holding a pointer the caller was free
  to reuse or release, and once it did, the bucket became unreachable under its
  own key. Both limiters now own their key (`_ratelimit_key_own`, an `fl_alloc`
  copy on the same freelist as the bucket).

  ⚠ **It passed the obvious test.** A string literal is one pooled address
  reused at every call site, so `ratelimit_check(rl, "k1")` in a loop shares a
  bucket and looks correct — which is how this survived since the module was
  written. The regression tests build every key at a fresh address.

  Reported by agnosai on 2026-08-13, measured through its HTTP handler: three
  identical requests produced **3 active keys and 0 rejections**.

- **ratelimit** — **the eviction sweep leaked its own scratch on every run.**
  `map_keys` builds a vec and `to_remove` was a second one; both come from
  `vec_new_a(default_alloc())` — the global **no-free** bump — and `vec_push`
  growth abandons each old buffer there as well. A 100k-key sweep burned well
  over a megabyte that never came back, so the routine whose entire purpose is
  bounding a limiter's memory consumed more of it the better it worked. The
  sweep now **allocates nothing**: it walks the entries array through the map's
  own public accessors (`map_entries` / `map_cap`), the same iteration
  `map_keys` and `map_iter` do internally. Deleting during the walk is safe
  because `map_delete` only tombstones the slot it finds — it never moves,
  reallocs or rehashes.

- **ratelimit** — **eviction leaked both halves of every bucket it removed.**
  `ratelimit_evict_stale` called `map_delete` and dropped the key and the
  16-byte bucket on the floor, so the one mechanism built to bound a limiter's
  memory grew it instead — unbounded under churning keys. Both are now returned
  with `fl_free`, after the `map_delete` that needs the key to find the entry.
  Safe because `_map_find` never key-compares a tombstone and `_map_grow_a`
  rehashes only live entries, so the stale pointer left in the slot is never
  dereferenced.

### Notes

- ⚠ **`ratelimit_check`/`sliding_window_check` take a cstr, not a `Str`.** The
  bucket map is `KeyTypeCstr` — it hashes with `hash_str` and compares with
  `streq`, both reading bytes from the pointer. A `Str` VALUE passed instead is
  read as its header, whose first eight bytes are the data pointer, so identical
  content at two addresses hashes two ways. That mismatch is what agnosai hit.
  Callers holding a `Str` must pass `str_cstr(s)`. Documented at both entry
  points; the signatures are unchanged.
- ⚠ `sliding_window_*` still has **no eviction sweep**, so its entries live for
  the process. That predates this release and is unchanged by it; only
  `ratelimit_*` is bounded in time.

### Added

- **ratelimit** — `total_evicted` in `ratelimit_stats` (offset 24) is now wired
  to a real counter, cumulative across sweeps, and surfaced by
  `_admin_ratelimit_json`. It had been hardcoded `0` with an "unused for now"
  note; with eviction now reclaiming memory, the churn it measures is worth
  seeing. `RateLimiter` grows 48 -> 56 bytes, which is the same freelist class.
- 6 ratelimit/sliding-window regression tests — `test_core` **159 → 189
  assertions**, plus one in `test_backends` for the new `evicted` field (**242 →
  249** across the three runnable suites). Every one keys on bytes at a fresh
  address, because a string literal is one pooled address reused at every call
  site and a pointer-keyed limiter passes happily under literals.
  Mutation-verified, 7 probes / 7 kills: borrowing the key in either limiter,
  dropping either `fl_free` in the sweep, swapping the map to `map_new_str`, and
  a sweep that ignores its idle threshold are all caught.

## [2.6.3] — 2026-08-12 — the `fl_alloc` stopgap is retired; upstream fixed it properly

### Changed

- **Toolchain pinned to cyrius 6.5.20** (was 6.5.18), which makes **`fl_alloc`
  thread-safe** — the defect majra reported from `test_relay_receive_is_reentrant`
  on 2026-08-10. The fix landed in 6.5.19; 6.5.20 is taken because it re-folds
  **patra 1.13.0**, and patra reaches majra as a stdlib module. patra 1.12.12 —
  what every earlier snapshot folded — carried its own `[deps.sakshi]` at
  **2.4.2** against the 2.4.10 the same snapshot shipped, and `cyrius deps`
  overlays a git dep's resolution on top of the `lib sync --full` snapshot on
  **every `cyrius build`**, recursing through sibling manifests. majra was never
  hit (it consults no patra manifest — patra arrives from the fold, and majra's
  `src/` calls no sakshi symbol), but the pin should name a snapshot without the
  hazard in it.

  The filing described **one** race: two threads popping the same block off
  `_fl_heads[cls]`. Upstream found **five** and locked all of them behind a
  process-wide CAS spinlock — `fl_init`'s check-then-set, the pop, the push, the
  arena bump, and an arena refill whose `mmap` left a **~2 µs unlocked window**
  (roughly 1,000× wider than the pop) that could return a block **running off the
  end of its mapping**. The large (>4096) path stays lock-free; it touches no
  shared state. The lock costs nothing until threads exist (a `_threads_active`
  gate).

- **`relay_receive_ex` allocates its result struct AFTER the unlock again.**

  2.6.1 pulled the 16-byte `fl_alloc` *inside* the critical section as a
  stopgap, because an unsynchronised `fl_alloc` could hand two concurrent
  `relay_receive` callers the same block — which corrupted the dedup table (a
  message arriving under another sender's sequence, then rejected as a
  duplicate) and, under real contention, faulted.

  ⚠ **That stopgap was never a complete fix**, and the header said so: it
  serialised majra against *majra* while any other thread in the process calling
  `fl_alloc` still raced it. With the allocator fixed upstream, the block goes
  back outside the lock, which is where it belongs — it is private to the call
  and nothing under the lock reads it. **The lock is held across a
  subscriber-list walk on a hot pub/sub path**, so shortening it is the point of
  undoing the workaround, not a cosmetic tidy.

  No API change; `relay_receive` / `relay_receive_ex` return the same struct.

## [2.6.2] — 2026-08-11 — the priority queue: O(n²) drain, and an unguarded negative index

### Performance — a pop is now O(1) amortised, and 6,000x faster at depth

`pq_dequeue` took the front of a tier with `vec_get(tier, 0)` followed by
`vec_remove(tier, 0)`. `vec_remove` shifts every remaining element down a slot, so
**one pop cost O(n) in the tier depth and draining a queue of n cost O(n²)**.

Measured, mean cost of a single `pq_dequeue` while draining a tier of that depth:

| depth | before | after |
|---|---|---|
| 2,000 | 2.00 µs | **34 ns** |
| 4,000 | 4.02 µs | **34 ns** |
| 8,000 | 7.92 µs | **33 ns** |
| 16,000 | 15.56 µs | **34 ns** |
| 200,000 | 198.70 µs | **33 ns** |

Before, doubling the depth doubled the per-pop cost — the signature of a linear pop
rather than cache noise; at 200,000 a full drain was roughly **40 seconds of memmove**.
After, the cost is flat across a 100x range, which is what O(1) looks like.

⚠ **This was never the pathological case — it is the DESIGN case.** A priority queue
exists so that low-priority work is *allowed* to accumulate behind high-priority work,
so the deep-tier path is precisely the one that has to stay cheap. Reported by a
consumer (agnosai) whose `llm/inference_queue` puts background summarisation behind
interactive crew tasks.

**How.** Each tier gains a read index; a pop advances it instead of moving the
survivors, and the consumed prefix is reclaimed once the head passes the midpoint —
so each surviving item is copied at most once per doubling of the head, amortised
O(1). A fully drained tier costs nothing to reclaim: length and head both reset with
no copying, which is the common shape for a queue that empties between bursts.

⚠ **Swap-with-last was NOT used**, though it is the other obvious O(1) trick. It
destroys FIFO within a priority level, which `queue_item_new`'s monotonic id and
`ManagedQueue`'s semantics both depend on.

### Changed

- `PriorityQueue` is **88 bytes**, was 48 — five read heads at offsets 48-87. The
  tier pointers (0-39) and the total (40) keep their offsets, so `pq_len` and
  `pq_is_empty` are untouched. Nothing outside `src/queue.cyr` reads the struct
  directly; verified by grep before the layout change.

### Fixed — `pq_enqueue` clamped only ONE end, and the missing end was memory-unsafe

The over-range clamp (`pri >= NUM_PRIORITIES`) had always been there. A **negative**
priority went straight through to `load64(pq + pri * 8)`, which for `pri = -1` reads
eight bytes **before** the struct and then `vec_push`es onto whatever that decoded to.
An out-of-bounds read followed by a write through the result — not a wrong answer, a
memory-safety bug.

Reachable from anything that computes a priority rather than naming one:
`queue_item_new(priority, payload)` stores the caller's value verbatim, so a consumer
mapping its own enum onto this one is a single arithmetic slip from `-1`.

Both ends now clamp to `PRIORITY_NORMAL`.

⚠ **Without the guard the suite does not fail an assertion — it dies mid-test.** The
three preceding queue tests print `ok`, then the process is gone: no assertion message,
no summary line. Verified by removing the guard and running.

`test_queue_priority_clamping` covers `-1`, a far-negative, and an over-range value;
asserts all three land in NORMAL **in arrival order** (clamping must not reorder), and
that a real `PRIORITY_CRITICAL` still preempts a clamped item.

### Changed — lint is clean across `src/`, was 13 warnings in two files

All 13 were `line exceeds 120 characters`, pre-existing and untouched by the
queue work: 8 in `src/ws.cyr`, 5 in `src/postgres_backend.cyr`.

⚠ **`src/postgres_backend.cyr`'s were long SQL literals, and the emitted SQL is
byte-identical** — verified by reassembling every `pg_exec` / `str_builder`
string from `git show HEAD:` and from the new file and comparing the two sets:
**8 strings, identical**. The three `CREATE TABLE` statements now build through
`str_builder`, which is the idiom `pg_save_workflow_def` in the same file already
used; the splits fall on `, ` boundaries inside the column list.

⚠ **A `\`-continued string literal was NOT used**, in either file.
`cyrius fmt` reindents inside multi-line literals and the leading spaces land
**in the string** — which for a SQL statement means silently corrupted DDL. The
`ws.cyr` HTTP/101 response is split into three `add_cstr` calls at header
boundaries for the same reason.

`ws.cyr`'s other seven were the SHA-1 big-endian word assembly and the 20-byte
digest spill; both are now one operation per line and read better for it.

### Added

- `test_queue_deep_fifo_and_compaction` — the existing FIFO test used **three**
  items, too shallow to reach the interesting states. This drains 500 at one
  priority across ~9 compactions, refills after a full drain (the head-reset
  branch), and runs 400 interleaved push/pops so the head advances while the tier
  never empties.

  ⚠ It asserts the **backing vec length**, not just order. Deleting `_pq_compact`
  outright leaves every ordering assertion passing — order is correct either way —
  while the tier grows to hold all 400 pushes forever. That is a memory defect an
  order test structurally cannot see. Both mutants (compaction removed; head not
  reset after compacting) are verified to fail this suite.

**Gate:** 4 suites, **159 + 42 + 36 assertions, 0 failures**, with live Redis and
PostgreSQL. `cyrius lint` clean across every file in `src/`, `cyrius fmt` clean
across `src/`, `tests/` and `benches/`, `vet` 27 deps / 0 untrusted, `deny` 0
violations, `deps --verify` 107/107, all four `dist/` bundles regenerated and
idempotent. Toolchain and deps were already current: cyrius **6.5.18**, sigil
**3.12.7**; `lib/` diffs clean against the pinned snapshot.

## [2.6.1] — 2026-08-10 — `relay_receive` raced the ALLOCATOR, not the relay

### Fixed — a concurrent `relay_receive` could be handed another sender's message

`relay_receive_ex` allocated its result struct with `fl_alloc(16)` **after** releasing its
mutex. **`fl_alloc` is not thread-safe** — `lib/freelist.cyr` manipulates the global
`_fl_heads` free lists with plain loads and stores, no mutex and no atomics — so two
threads in that window could be handed the **same block**.

The symptom was not a crash at the allocation. It was wrong data in the dedup table: a
message arriving under another sender's sequence number and correctly rejected as a
duplicate, surfacing as an intermittent

```
FAIL: every strictly-increasing message from sender-a is accepted
FAIL: no message was wrongly dropped as a duplicate
```

— which reads like a defect in the dedup logic, the one part of this path 2.6.0 had just
audited. Under heavier contention it **faulted** instead of failing an assertion.

⚠ **2.6.0's reentrancy fix was correct and is untouched.** It made the dedup table's
mutation safe. This is a second, independent race in the same function, in an allocation
that happens *after* that mutex is dropped — which is why auditing the locking again
would never have found it. The fix allocates the 16 bytes inside the critical section, a
section that already walks the subscriber list.

Measured, 40 concurrent instances of `tests/test_core.tcyr`:

| build | failures |
|---|---|
| before | **4 / 40**, one a core dump |
| after | **0 / 40** |

A single run passes either way; only contention separates them, which is why this reached
CI rather than a local test sweep.

### Fixed — the reentrancy test was measuring the allocator, not the relay

`test_relay_receive_is_reentrant` built its 400 messages per worker **inside** the worker
threads, with `fl_alloc`. The two workers therefore raced the allocator, and the test
could fail for a reason unrelated to `relay_receive`'s reentrancy. Messages are now built
up front on the calling thread and the workers only read them, so the test measures the
invariant it names.

Filed upstream as
`cyrius/docs/development/issues/2026-08-10-fl-alloc-is-not-thread-safe-and-says-nothing.md`:
`lib/freelist.cyr` documents neither the constraint nor a safe variant, and it ships
beside `thread.cyr`.

### Changed

- **Toolchain pin 6.5.14 → 6.5.18.**
- **`[deps.sigil]` 3.12.6 → 3.12.7.**
- **`sakshi` moved from `[deps.sakshi]` into `[deps].stdlib`, at 2.4.10.**

  That git pin was **defensive**, and the thing it defended against is gone. It existed
  because sigil's own manifest declared `[deps.sakshi]`, and `cyrius deps` overlays a git
  dep's resolution on top of the `lib sync --full` snapshot — so left implicit, sigil
  silently downgraded `lib/sakshi.cyr` on every build behind an unnamed "1 bundled lib(s)
  differ" warning. **sigil 3.12.7 dropped that dep**, so there is nothing to counteract:
  `patra` reaches majra as a *stdlib* module from the snapshot, not as a git dep, so its
  manifest is never consulted. majra's `src/` calls no sakshi symbol and none of the four
  bundles reference one.

  ⚠ **Do not re-add a `[deps.sakshi]` here to "pin" it.** On a library that publishes
  bundles, a git dep makes `distlib` reclassify the module out of the **stdlib leaves**,
  dropping it from the `.deps` sidecars and breaking clean-room consumers — kavach hit
  exactly that and had to revert it.

### Verified

- **241 assertions, 0 failures** across all four suites, including `tests/test_live.tcyr`
  (36) run against **real Redis and PostgreSQL** in Docker rather than skipped — the
  containers, and the `pg_hba.conf` rewrite the CI job performs, were stood up locally so
  the live path was actually exercised.
- `cyrius bench` clean.
- All four `dist/` bundles regenerated — umbrella **and** the `backends` / `admin` /
  `signed` profiles. ⚠ A bare `cyrius distlib` writes only `dist/majra.cyr`; each profile
  needs its own `cyrius distlib <name>`, and skipping them ships a bundle stamped with the
  previous version.
- **40 concurrent instances of `test_core`, 0 failures**, against 4/40 before the fix.
- `lib/sakshi.cyr` holds at **2.4.10 through a build**, with no shadow warning.

## [2.6.0] — 2026-08-08

**`relay_receive` was not reentrant, and three smaller relay defects alongside
it.** All four were reported by agnosai, which drives majra's relay from a
100-worker `sandhi_server_run_pooled` pool. Minor bump rather than patch: two
new public functions and one appended stats field.

### Fixed — `relay_receive` raced itself (Critical for threaded consumers)

Two independent problems, either one sufficient to corrupt a dedup decision:

- **It stashed its arguments and dedup state in FILE-SCOPE globals** —
  `_recv_r`, `_recv_msg`, `_dedup_map`, `_dedup_key`, `_dedup_seq`. Their own
  comment said they "predate cc5", were a cc3 local-clobbering workaround, and
  were kept because removing them "would require a structural refactor". That
  refactor is this release: they are parameters, and the globals are gone.
- **It took no lock at all**, while every other mutating entry point
  (`relay_send`, `relay_subscribe`, `relay_evict_stale_dedup`) locked `r + 40`.
  So the `seen` map was mutated unsynchronised even setting the globals aside.

`relay_receive` now holds the mutex across the dedup decision, the eviction
sweep and the counter updates, then **releases it before the subscriber
fan-out** — the same shape 2.5.3 gave `pubsub_publish`, so a slow or full
subscriber channel cannot block an unrelated receive.

**Verified by a threaded test, not by inspection.** `test_relay_receive_is_reentrant`
runs two workers with distinct sender ids, 400 messages each, strictly
increasing sequences — so with correct per-call state there is no interaction
and every receive must be accepted. Against the pre-fix code it fails
reproducibly (3/3 runs) on `no message was wrongly dropped as a duplicate`:
the race made real messages look like replays.

Also removed: `_relay_check_dedup`, dead since `relay_receive` inlined its
logic — referenced only by the comment that justified the globals.

### Fixed — `is_broadcast` was computed and thrown away

`relay_receive` derived whether a message was a broadcast and then returned the
bare message, so a consumer could not tell a broadcast from a direct message
without re-inspecting the envelope.

**Added `relay_receive_ex`**, returning an `IncomingMessage` (16 B:
`incoming_is_broadcast`, `incoming_msg`). `relay_receive` is unchanged in
behaviour and return type — it now delegates and drops the flag — so existing
consumers keep working.

### Added — sequence-gap detection

A gap means at least one message from a sender was lost in transit. The message
is still delivered; the gap is what a caller needs to request a resend.

- **`relay_last_seq(r, from)`** — the last sequence seen from a sender, or `-1`
  if unknown. Taken under the lock.
- **`sequence_gaps`** in `relay_stats`, counted when `seq != last + 1`, and also
  when a *first* message from a new sender arrives at `seq != 1`.
- A duplicate is **not** counted as a gap — it increments `duplicates_dropped`,
  as before.

⚠ **`relay_stats` grew from 40 to 48 bytes, with `sequence_gaps` APPENDED at
offset 40.** Offsets 0–32 are unchanged, so a reader built against 2.5.x still
reads the right fields; `test_relay_stats_layout_unchanged` pins each one.

### Documented — bounded dedup opens a replay window

`relay_set_max_dedup` evicts the least-recently-seen sender when the table
overflows, and evicting a sender forgets its last-seen sequence — so that
sender's already-delivered messages become acceptable again and are fanned out a
second time. This is **off by default** (`relay_new` sets `0` = unbounded) and
that default is correct for a strict deliver-once contract; the behaviour is now
stated at the call site rather than left to be discovered.

### Changed — toolchain and dependencies

- `cyrius` **6.5.10 → 6.5.14**. Three-step resolve, 107 files.
- `[deps.sigil]` **3.12.1 → 3.12.6** — five releases of RSA fixes, including the
  PKCS#1 v1.5 and PSS **authentication bypasses** (a forged signature verifying)
  that 3.12.3–3.12.6 closed. majra reaches sigil through `ipc_encrypted` and the
  TLS path, so this is not optional.
- `[deps.sakshi]` stays 2.4.8 — already what the 6.5.14 fold carries
  (hash-verified identical).

### Verification

`test_core` **142 passed / 0 failed** (was 112 — 30 new assertions), plus
`test_backends` 42/42 and `test_patra_queue` 17/17. The **full soak set** was run
because this touches relay dedup: `soak_relay` (dedup correct, eviction
bounded), `soak_queue`, `soak_pubsub`, `soak_heartbeat` — all OK. All four dist
profiles regenerated and verified to carry the new API.

⚠ **Pre-existing and NOT fixed here:** `cyrius distlib`'s self-check reports
`undefined variable 'CLOCK_MONOTONIC'` in the generated bundles
(`src/envelope.cyr:37` uses it; `lib/syscalls.cyr:40` includes the platform file
that defines it). This is **not** a consumer-visible defect — a program that
includes `dist/majra.cyr` builds and links fine, verified — and it reproduces on
2.5.3 with 8 errors, so it predates this release. Worth its own investigation.

## [2.5.3] — 2026-07-28

**Two silent data-loss races and a namespace-isolation bypass in the routing
core.** This is the first release in the 2.5 line to change `src/` logic — all
four dist bundle bodies move. Every fix below was reproduced with a probe
before the change and re-run after: the probes are the acceptance criteria, and
the durable ones are now in-tree as regression assertions. **321/321 CI**
(150 core + **112** expanded, up from 96 + 42 backends + 17 patra_queue) +
3/3 fuzz + 4/4 soak + **36/36 live**.

The trigger was a third-party report that "majra's pubsub subscribe path is
broken", filed with no repro. It was right, for a reason the reporter didn't
identify — and the same root cause was also eating queue jobs.

### Fixed
- **`pubsub_subscribe` could hand a caller a channel that was never registered
  (silent, ~0.1-0.9% under contention).** All three subscribe entry points ran
  `chan_new` + `_sub_new` **before** taking the hub mutex. `chan_new` is safe —
  it uses `alloc`, which carries a CAS spinlock — but `_sub_new` uses
  `fl_alloc`, and `lib/freelist.cyr` pops its size-class free list with a plain
  load/store pair and **no lock at all**. Two concurrent subscribers could
  therefore be handed the *same* 16-byte subscriber block; both wrote it, the
  vec received the same pointer twice, and one caller walked away with a live
  channel that no subscriber entry pointed at. That caller's `chan_recv` blocks
  **forever**, and `pubsub_publish` counted the aliased subscriber twice so the
  delivered count concealed it. Measured: **1-7 orphaned channels per 800**
  concurrent subscribes, 5 of 8 runs affected; **0 across 10 runs** after
  moving the lock above the allocations. Regression test:
  `test_pubsub_concurrent_subscribe`.
- **`mq_enqueue` silently lost jobs under concurrency — the same class, worse
  blast radius.** It performed `queue_item_new` (an `fl_alloc`), `_next_job_key()`,
  and `fl_alloc(72)` all before locking, and `_next_job_key()` increments the
  `_mq_next_job_id` global with a plain read-modify-write. Two enqueues could
  take the **same job key**, and the second `map_set` overwrote the first job
  with no error anywhere. Measured: **788-796 of 800** jobs surviving, 3 of 6
  runs affected; **800/800 across 6 runs** after extending the existing mutex to
  cover key generation and both allocations. Regression test:
  `test_managed_queue_concurrent_enqueue`.
- **A lagging subscriber froze the entire hub, including unrelated topics and
  `pubsub_subscribe` itself.** `pubsub_publish` held the hub mutex across
  `chan_send`, which **blocks** when a subscriber's 64-slot channel is full — so
  the publisher parked inside the critical section. Publishes to other topics
  with empty channels blocked; `pubsub_subscribe` on a brand-new topic blocked.
  Measured behind a single slow-but-well-behaved consumer: publish latency on an
  unrelated, always-empty topic went **4.7us → 10.7ms avg / 213ms worst**, versus
  the ~1us/op in `docs/architecture/overview.md`. Publish now snapshots each
  subscriber list as a `(data, len)` pair under one short lock and walks it
  unlocked — safe because `vec_push` grows into a *new* bump-allocated buffer
  and never frees the old one, and pushes only append. `hashed_channel_send` had
  the identical shape and got the same treatment. Regression test:
  `test_pubsub_no_head_of_line_block` (it hangs rather than fails if this
  regresses — the honest signal for a liveness bug).
- **`pubsub_publish` over-reported `delivered` on closed channels.**
  `chan_send` returns −1 for a closed channel; the count incremented anyway.
  Now only a `chan_send` returning 0 counts.

### Changed — pattern matching (behavior change, read this)
- **`#` and `+` are now honored only when they occupy a whole level.**
  Previously both were matched anywhere in a token, so a pattern could escape
  its namespace prefix: **`"tenant-a#"` matched `"tenant-a-evil/secret"`** and
  `"sensors/temp+"` matched `"sensors/temperature"`. `namespace_wildcard(ns)`
  builds exactly `"<prefix>/#"`, so a single dropped `/` turned tenant isolation
  into a prefix scan over other tenants. `#` must additionally be the final
  character (MQTT-3.1.1 §4.7.1.2); anywhere else it is now a literal byte.
- **`"<prefix>/#"` now also matches the bare topic `"<prefix>"`**, per
  MQTT-3.1.1 §4.7.1.2 ("`sport/#` also matches the singular `sport`, since `#`
  includes the parent level"). Previously it did not.
- These two items **change matching semantics**, which `docs/development/semver.md`
  treats as off-limits for a PATCH. They ship in a patch anyway because the old
  behavior is a defect against the module's own documented "MQTT-style" contract
  and the first item is a security bug — but a consumer that (deliberately or
  accidentally) relied on mid-token wildcards will see subscriptions stop
  matching. 8 new assertions in `test_pubsub_pattern_matching_extended` pin the
  new semantics.

### Performance
- **Net neutral after two optimization passes; no regression >10%.** 17 targets
  × 7 trials, pre-fix vs post-fix, compared on min *and* median. The naive
  correct version cost **+110% on `pattern_exact`** and **+40% on
  `pubsub_publish_nosub`** — the former from computing wildcard alignment on
  every character, the latter from a `map_get` (string hash) per loop iteration.
  Both were restructured: alignment is checked inside each wildcard branch so a
  literal byte costs what it always did, and publish snapshots once instead of
  re-resolving the map per subscriber. Final: `pattern_exact` **−2.4%**,
  `pubsub_publish_nosub` **+0.3%**, `pubsub_1sub_publish` **+0.3%**. The one
  residual is `pattern_wildcard_#` at **+4ns** (+8.9% min / +11.1% median) —
  that is the level-alignment check itself, and it is the price of the
  isolation fix.

### Distribution
- All four bundle **bodies** change (first `src/` logic change in the 2.5 line):
  `majra.cyr` 3,187 → **3,284** lines, `majra-signed.cyr` 3,333 → **3,430**,
  `majra-admin.cyr` 3,320 → **3,417**, `majra-backends.cyr` 4,788 → **4,885**.
  The `signed` and `admin` `.deps` sidecars gain `alloc`, which those profiles
  now reference directly.

## [2.5.2] — 2026-07-28

**Toolchain pin `6.4.62` → `6.4.83` + sigil `3.11.1` → `3.12.1` (latest) +
sakshi pinned forward `2.4.3` → `2.4.6`, and a `cyrius bench` repair.** No
majra source-logic change: the four dist bundle **bodies stay byte-identical**
(the whole `git diff dist/` is four banner lines, 2.5.1 → 2.5.2; the `.deps`
sidecars regenerate identically). Full matrix re-ran clean under the new pin —
**305/305 CI** (150 core + 96 expanded + 42 backends + 17 patra_queue) +
**3/3 fuzz** + **4/4 soak** + **17/17 bench** targets.

The 6.4.62 → 6.4.83 span leaves majra's surface alone: the `lib sync --full`
snapshot holds at **99 files**, `cyrius.lock` holds at **99 hashes + 1
commit-pin**, and `cyrius lint` output is byte-identical across all 23 `src/`
files under the two toolchains (both linters run against the same tree, 104
lines of output, zero diff). One toolchain change is visible in-tree, via the
stdlib snapshot's bundled sakshi — see below.

### Changed
- Toolchain pin `6.4.62` → `6.4.83`; `[deps.sigil]` tag `3.11.1` → `3.12.1`.
  sigil's `dist/sigil.cyr` grows 25,391 → **26,254 lines**. majra's sigil
  footprint is unchanged at six symbols — `ed25519_{init,sign,verify}`
  (`src/signed_envelope.cyr`) + `aes_gcm_{global_init,encrypt,decrypt}`
  (`src/ipc_encrypted.cyr`); the constant-time pk compare is still stdlib
  `ct_eq_bytes_lens`. `core` and `admin` pull no sigil.
- **The two pins are an atomic move: the crypto profiles now floor at cyrius
  ≥ 6.4.64.** sigil 3.12.1 stopped hardcoding its crypto-bank thread-local slot
  (`_SIGIL_CBANK_SLOT = 8` at 3.11.1) and now allocates it dynamically —
  `_SIGIL_CBANK_SLOT = -1` plus a CAS-gated `thread_local_alloc()`. That symbol
  does not exist in the 6.4.62 stdlib snapshot (`TLOCAL_MAX_SLOTS = 16`, no
  allocator); it first appears in **6.4.64** (`TLOCAL_MAX_SLOTS = 128`).
  Verified by building the same consumer against both sigil bundles on both
  snapshots. So **sigil 3.12.1 cannot be paired with cyrius 6.4.62** — the build
  fails with `refusing to emit binary with N reachable undefined function(s)`.
  sigil's own comment says "requires cyrius >= 6.4.65"; 6.4.64 is where the
  symbol lands, so 6.4.65 is conservative-safe. majra's builds were never at
  risk (`tests/test_backends.tcyr` pulls `lib/tls.cyr` → `lib/thread_local.cyr`
  ahead of sigil), but **downstream consumers of `majra-signed` /
  `majra-backends` are** — see the README note below.
- **README's consumer include contract was incomplete, and is now verified
  rather than asserted.** The dist bundles are pure `src/` concatenation with
  **zero `include "lib/…"` lines**, so the consumer supplies every stdlib module
  both majra *and sigil* reach into — but the README only ever named `lib/ct.cyr`
  and (for admin/backends) `lib/sandhi.cyr`. Building a clean consumer against
  each shipped bundle shows `signed` additionally needs `thread_local`, `io`,
  `fs`, `chrono`, `bayan`, `keccak`, `random`; `admin` needs `net`, `io`,
  `chrono`, `async`, `dynlib`, `fdlopen`, `sakshi`, `random` + `tls` before
  `sandhi`; `backends` needs the union. README now carries the per-profile table.
  **`lib/thread_local.cyr` was already required at sigil 3.11.1** (for
  `thread_local_{init,get,set}`) — that gap is pre-existing, not new here.
- **New explicit `[deps.sakshi]` block, pinned to the published latest
  (`2.4.6`).** sakshi reaches majra's compilation unit only transitively (patra
  for the durable queue, sigil for its logging floor) — but sigil's own manifest
  declares `[deps.sakshi] tag = "2.4.3"`, and `cyrius deps` overlays that
  resolution *on top of* the `lib sync --full` snapshot, which ships 2.4.6 under
  the 6.4.83 pin. Left implicit, every build **downgraded** `lib/sakshi.cyr` and
  printed `./lib/ shadows version-pinned … sakshi 2.4.3 (pinned: 2.4.6)`.
  Declaring it at the top level pins the resolution forward and the warning is
  gone. The span is backward-compatible for majra: 2.4.4 is purely additive
  (128-bit W3C trace-id), 2.4.5 fixes the agnos `_sk_open` `O_RDWR`→`AO_WRONLY`
  fold (a real read-path bug on the agnos target), 2.4.6 is a pin catch-up.
  `cyrius.lock`'s commit-pin moves `2.4.3` → `2.4.6`; the hash count is unchanged.

### Fixed
- **`cyrius bench` / `cyrius audit` could not compile `benches/bench_all.bcyr`.**
  Both resolve the manifest `[deps].stdlib` list into the compilation unit —
  which includes `tls` and `sandhi` — while the bench entry point included
  neither `fdlopen` (called by `lib/tls.cyr`) nor `async` (called by sandhi). The
  driver stopped at `error: refusing to emit binary with 4 reachable undefined
  function(s)`, so `cyrius bench` reported `0 passed, 1 failed` without running a
  single benchmark. The entry point now carries the same explicit toolchain
  includes the test entry points already had (`async`, `dynlib`, `fdlopen` — see
  `tests/test_backends.tcyr`). **This was pre-existing, not a 6.4.83
  regression** — reproduced identically at the 2.5.1 state with
  `CYRIUS_HOME` pinned to 6.4.62. CI was unaffected because it builds benches
  via `cyrius build --no-deps benches/*.bcyr`, which never injects the manifest
  stdlib set; the breakage was confined to the `bench`/`audit` convenience path
  the release process actually leans on.

### Docs
- **[`cyrius-quirks.md`](docs/development/cyrius-quirks.md) §6 rewritten — it was
  describing 6.1.x behavior that no longer holds.** The entry claimed an
  undefined symbol is *always* a warning + runtime `ud2`. The toolchain now
  splits on reachability: a **reachable** call site is a hard `error: refusing
  to emit binary with N reachable undefined function(s)` and **no binary is
  written**, while an **unreachable** one still warns and emits the `ud2`.
  Verified empirically at both 6.4.62 and 6.4.83 — this is a stale-doc fix, not
  a 6.4.83 change. The entry also now records that reachability is computed over
  whatever the *driver* injects, so a green CI (`--no-deps`) does not imply
  `cyrius audit` compiles — the trap this release's bench fix walked into.
- §7 gained the snapshot-shadowing rule (a `lib sync --full` copy can be
  silently downgraded by a `cyrius deps` overlay carrying an inherited tag) and
  its stale `cyrius lib sync` invocations were corrected to `--full`; same
  correction in [`testing.md`](docs/guides/testing.md), which still had the
  pre-6.4.x form. §5's "latest sigil" reference caught up 3.7.10 → 3.12.1.
- [`threat-model.md`](docs/development/threat-model.md) had two pre-existing
  stale claims: the Crypto-trust-boundary line still cited the **sigil 3.7.8**
  pin (four bumps behind) and Supply Chain still said "one first-party dep".
  Both corrected; the latter now also notes that a transitive dep whose version
  is inherited from another manifest is not a version majra controls.
- `state.md` / `dependency-watch.md` / `roadmap.md` / `doc-health.md` /
  `README.md` / `CLAUDE.md` refreshed for the new pins.

### Verified
- 150 core + 96 expanded + 42 backends + 17 patra_queue = **305/305, 0 failed**.
- 3/3 fuzz harnesses (500 iters × 10s timeout), 4/4 soak suites.
- **Live integration: 36/36, 0 failed** — 7 Redis + 4 PostgreSQL categories
  against `redis:7-alpine` + `postgres:16-alpine`. (`state.md` had recorded 32
  for this suite; `docs/guides/testing.md` had the correct 36. Corrected.)
- Clean-consumer builds against all four shipped bundles (`core`, `signed`,
  `admin`, `backends`) — each compiles and runs from an entry point that has
  only the documented include set, which is how the README table above was
  derived rather than asserted.
- `cyrius vet src/main.cyr` → 27 deps, 0 untrusted, 0 missing;
  `cyrius deny src/main.cyr` → 27 deps, 0 violations; `cyrius fmt --check`
  clean across `src/` + `tests/` + `fuzz/` + `benches/`.
- Cold rebuild from scratch (`rm -rf build lib && lib sync --full && deps &&
  build --no-deps`) passes, and `cyrius deps --verify` reports **99 verified,
  0 failed** against the committed lockfile. The sakshi commit-pin
  (`bfc127f8…`) matches GitHub's `2.4.6` tag object, and the resolved
  `lib/sigil.cyr` hashes identical to `dist/sigil.cyr` at sigil's `3.12.1` tag
  — so the local `path = "../sigil"` resolution and CI's git resolution agree.
- **Benchmarks: no regression.** 17 targets × 7 trials on each pin (6.4.62
  baseline vs 6.4.83), compared on both min and median: every delta lands
  within **±3.4%**, none over the 10% flag threshold. The eye-catching
  single-run swings (`pattern_wildcard_+` "+24%", `barrier_cycle` "+13%") were
  run-to-run noise — the same 6.4.83 binary produced both 85 ns and 118 ns for
  `pattern_exact` — which is why the comparison is distribution-based.

## [2.5.1] — 2026-07-13

**Toolchain pin `6.3.15` → `6.4.62` + sigil `3.9.8` → `3.11.1` (latest) + a
sigil-footprint review.** No majra source-logic change: the four dist bundle
**bodies stay byte-identical** (only the version banner moves 2.5.0 → 2.5.1).
Full matrix re-ran clean under the new pin — **305/305 CI** (150 core + 96
expanded + 42 backends + 17 patra_queue) + **3/3 fuzz** + **4/4 soak**.

The cyrius 6.3.15 → 6.4.62 span is almost entirely agnos syscall wrappers,
the async-runtime target split, and DX diagnostics — nothing touching majra's
surface. Two toolchain changes are visible in-tree:
- **`cyrius lib sync` default is now the declared `[deps].stdlib` *subset*
  (40 files); the whole snapshot is `--full` (99 files).** CI + release
  already invoke `cyrius lib sync --full`; the CLAUDE.md quick-start was
  corrected to match. `cyrius.lock` carries **99** resolved-file hashes.
- **Per-profile `distlib` `.deps` sidecars re-subsetted** (cyrius 6.4.48 fix +
  the 6.3.32 built-in `std` group). The three profile sidecars drop `fmt`/
  `syscalls` (now implied by the always-resolved `std` group) and `assert`
  (test-only, unreferenced by the bundles); `signed`/`admin` additionally drop
  `alloc`, while `backends` keeps `alloc` (referenced directly by its extra
  modules). The `.cyr` bodies are unchanged; regeneration is idempotent, so
  the CI freshness gate stays green.

### Sigil-footprint review (`[lib.<type>]` per-primitive profiles)
sigil 3.11.0 added twelve per-primitive `[lib.<type>]` distlib profiles
("pull only the crypto you need"). majra's **entire** sigil surface is six
symbols — `ed25519_{init,sign,verify}` (`src/signed_envelope.cyr`) and
`aes_gcm_{global_init,encrypt,decrypt}` (`src/ipc_encrypted.cyr`); the
constant-time pk compare is stdlib `ct_eq_bytes_lens`, **not** sigil. The
`core` and `admin` profiles correctly pull **no** sigil.
- **majra keeps the full `dist/sigil.cyr`.** Its only local sigil consumer
  (`tests/test_backends.tcyr`) exercises *both* primitives, and the two narrow
  closures `sigil-ed25519.cyr` + `sigil-aes.cyr` (~2k lines each) **overlap on
  121 functions** (Ed25519 uses SHA-512 internally; both share sigil's u256
  field arithmetic + crypto-scratch + random floor). Combined they emit 121
  "last-definition-wins" duplicate-fn warnings — noisier and more fragile than
  the full bundle's single *deduplicated* closure (which resolves clean). sigil
  publishes no `dist/sigil/index.cyml`, so the clean `modular = [...]` dedup
  path is unavailable. The per-primitive win is real only for a **single**-
  primitive consumer — a `signed`-only downstream (e.g. secureyeoman) should
  pull `dist/sigil-ed25519.cyr` (~2k lines) instead of the full 25,391-line
  bundle. Recorded in [`dependency-watch.md`](docs/development/dependency-watch.md).
- The bump also banks sigil **3.9.9**'s crypto-bank thread-local-slot fix
  (`_SIGIL_CBANK_SLOT` moved 0 → 8): slot 0 collided with **patra**'s SQL
  scratch slot, corrupting state in a process that links *both* — exactly the
  `backends` profile (sigil crypto + `patra_queue`).

### Fixed
- **Undersized `var X[N]` buffers in the test/soak harnesses (same class the
  2.5.0 audit fixed in `src/`, missed in the non-CI files).** A function-local
  `var X[N]` is **N bytes**, so a 16-byte `struct timespec` written into
  `var ts[2]` (2 bytes) overflows. Since cyrius 6.3.13 moved `var X[N]` locals
  onto the guarded thread stack, `tests/soak/soak_heartbeat.cyr`'s phase-B
  offline-eviction sleep was silently corrupting its own node count →
  **`FAIL B: nodes prematurely evicted after cycle 1`** (the soak set is not in
  the default CI pass, so it went uncaught at 2.5.0 despite already being live
  at 6.3.15). Every over-written buffer is now sized to the bytes it holds:
  `soak_heartbeat.cyr` + `test_core.tcyr` `ts[2]→ts[16]`; `test_backends.tcyr`
  `key[4]→key[32]` (×2), `nonce[2]→nonce[12]`, `buf[2]→buf[4]`. All four soak
  suites now pass (was 3/4). An adversarial review pass empirically pinned the
  underlying rule and corrected [`cyrius-quirks.md`](docs/development/cyrius-quirks.md) §4:
  a **function-local** `var buf[N]` is **N bytes**, but a **module-level/global**
  `var buf[N]` is **N × 8 bytes** (N `i64` slots) — so the shipped globals
  (`redis_backend.cyr` `_resp_buf[512]` = 4096 B, `error.cyr` `_err_msg_buf[64]`
  = 512 B) are correctly sized and were *not* touched.

### Changed
- Toolchain pin `6.3.15` → `6.4.62`; `[deps.sigil]` tag `3.9.8` → `3.11.1`.

## [2.5.0] — 2026-06-30

**agnos target support for the core pub/sub engine** (base-stack agnos-readiness
migration, tier 1 — the shared blocker under bote/t-ron/hoosh). majra had **zero**
`CYRIUS_TARGET_AGNOS` guards, so its core engine failed to compile on `--agnos`
(`undefined variable SYS_FUTEX`). The core (`dist/majra.cyr`) is now agnos-clean:

- **`barrier.cyr` / `queue.cyr`** — the `futex(FUTEX_WAIT/WAKE)` fast-path (no such
  syscall on agnos, and `SYS_FUTEX`/`FUTEX_*` are Linux-only stdlib constants) is
  now `#ifndef CYRIUS_TARGET_AGNOS`-guarded. On agnos the wait becomes a
  `sys_sched_yield()` spin-yield (cooperative scheduler) and the wake is a no-op —
  correct producer/consumer + barrier semantics on the single-core model.
- **`envelope.cyr`** — `time_now_ns` uses `sys_uptime_ms()` (#40, monotonic) on
  agnos instead of `clock_gettime` (#228, out of the frozen 0-63 range); UUID
  generation uses `sys_getrandom` (#45) instead of the Linux `getrandom` (#318).
- **`dag.cyr`** — retry backoff uses `sys_sleep_ms()` (#41) on agnos instead of
  `nanosleep` (#35, = `sysinfo` on agnos → mis-dispatch).
- **`ipc.cyr`** — AF_UNIX domain-socket transport (in the core bundle) fail-closes
  on agnos (`ipc_bind`/`ipc_accept`/`ipc_connect` → `Err(ERR_IPC)`), keeping the raw
  Linux socket numbers (41/42/43/49/50) off the agnos target.

Toolchain pin `6.2.11` → `6.3.15`. Host build byte-identical (all changes are
additive `#ifdef` branches). Core dist verified agnos-clean (`SYS_FUTEX`
references are all `#ifndef`-guarded; no raw `228`/`318`/`35`).

**Known residual (non-core):** `patra_queue.cyr` — the optional persistent-queue
backend — pulls `patra`, whose `lib/patra.cyr` still references `SYS_LSEEK`
unguarded on agnos. `patra_queue` is **excluded from the default `dist/majra.cyr`**
(core) profile, so no consumer that pulls the core engine (bote/t-ron/hoosh) is
affected; it only blocks a full `--agnos` build of the majra daemon + the
`backends` profile. Tracked for the patra migration.

### Fixed
- **Undersized array-local buffer overflows (host crash under cyrius ≥ 6.3.13).**
  Several `var X[N]` locals were too small for the bytes written into them:
  `var ts[2]` (2 bytes) for a 16-byte `struct timespec` (`envelope.cyr`
  `time_now_ns`, `dag.cyr` backoff, `main.cyr`), `var buf[2]` for 16 random bytes
  (`envelope.cyr` `uuid_generate`), and 1-byte frame headers written 2–4 bytes
  (`ipc.cyr`, `ws.cyr`, `postgres_backend.cyr`). A function-local `var X[N]` is
  **N bytes**, so these overflowed. It was *latent* before cyrius **6.3.13**, when
  local arrays lived in a shared global/BSS buffer (the overflow scribbled adjacent
  globals harmlessly); 6.3.13 moved `var X[N]` locals onto the **thread stack**
  (with a `PROT_NONE` guard page), so the same overflow now smashes the stack →
  `SIGSEGV`. Surfaced as a `test_core` segfault in `test_relay_skip_routing` (it
  calls `time_now_ns`) the moment the pin moved to 6.3.15. Every buffer is now
  sized to the bytes actually written. `test_core` **96/96**, `test_patra_queue`
  **17/17** green.

### Added
- agnos (`CYRIUS_TARGET_AGNOS`) support for the core engine (barrier/queue/
  envelope/dag/ipc): futex→sched_yield, clock→uptime_ms, getrandom→#45,
  nanosleep→sleep_ms, AF_UNIX IPC fail-closes.

### Changed
- Toolchain pin `6.2.11` → `6.3.15`.

## [2.4.7] — 2026-06-15

Cyrius toolchain minor bump **6.1.35 → 6.2.11** (first move onto the
6.2.x line) plus a routine dependency bump **sigil 3.7.10 → 3.7.14**
(latest). No majra source-logic change; the four distribution bundle
bodies are byte-identical to 2.4.6 (only the version banner moves). The
6.2.x stdlib snapshot grew the lib-sync set **88 → 97 files**, and
sigil 3.7.14 rolls transitive **agnosys 1.3.2 → 1.4.3**. All 305 CI
assertions + 3 fuzz harnesses + 4 soak suites pass under the new
toolchain.

### Changed

- **Cyrius toolchain pin 6.1.35 → 6.2.11** (`cyrius.cyml [package].cyrius`).
  First step onto the 6.2.x line; stdlib / codegen fixes pulled in via
  `cyrius lib sync` + `cyrius deps`. The lib-sync snapshot is now 97
  `.cyr` files (was 88 under 6.1.35).
- **sigil 3.7.10 → 3.7.14** (`[deps.sigil]`). Routine patch bump tracking
  latest. Transitive **agnosys 1.3.2 → 1.4.3**. The four bundle bodies
  stay byte-identical — sigil's `signed`/`backends` surface is unchanged
  across 3.7.10 → 3.7.14.
- **`cyrius.lock`** now carries SHA-256 over **97** resolved files (was
  88), reflecting the larger 6.2.x stdlib snapshot. CI's
  `cyrius deps --verify` enforces the match.

## [2.4.6] — 2026-06-11

Cyrius toolchain refresh within the 6.1.x line. Pin **6.1.24 → 6.1.35**,
and a routine dependency bump **sigil 3.7.8 → 3.7.10** (latest). No
majra source-logic change; the four distribution bundle bodies are
byte-identical to 2.4.5 (only the version banner moves). The one
mechanical adjustment the toolchain forced: `bigint` was dropped from
the cyrius 6.1.35 stdlib snapshot (94 → 88 files), and majra never
called it — sigil 3.x bundles its own `u256_*` field arithmetic — so
its lone stale `include` and the `[deps] stdlib` hint entry were
removed. All 305 CI assertions + 3 fuzz harnesses + 4 soak suites pass
under the new toolchain.

### Changed

- **Cyrius toolchain pin 6.1.24 → 6.1.35** (`cyrius.cyml [package].cyrius`).
  Eleven patch-level cyrius releases of stdlib / codegen fixes pulled in
  via `cyrius lib sync` + `cyrius deps`.
- **sigil 3.7.8 → 3.7.10** (`[deps.sigil]`). Routine patch bump now that
  sigil tracks latest under the cyrius 6.x toolchain (the 2.9.0 asm-NI
  pin was retired at 2.4.5). Transitive **agnosys holds at 1.3.2**. The
  four bundle bodies stay byte-identical — sigil's `signed`/`backends`
  surface is unchanged across 3.7.8 → 3.7.10.
- **`bigint` removed from the stdlib surface.** The cyrius 6.1.35 stdlib
  snapshot dropped `lib/bigint.cyr` (snapshot 94 → 88 files). majra had
  no `big_*` call sites — `tests/test_backends.tcyr` carried a stale
  `include "lib/bigint.cyr"` (a leftover from before sigil 3.x bundled
  its own `u256_*` ops) and `cyrius.cyml [deps] stdlib` listed `bigint`
  as a hint. Both removed; `cyrius deps` no longer errors on the missing
  module. `cyrius.lock` now carries **88** hashes (was 94).

### Verified

- Core (main.cyr smoke): **150/150**.
- `tests/test_core.tcyr`: **96/96**.
- `tests/test_backends.tcyr`: **42/42** — `aes_gcm_roundtrip`,
  `encrypted_ipc`, `signed_envelope`, `admin` all green on the sigil
  3.7.10 surface under cyrius 6.1.35.
- `tests/test_patra_queue.tcyr`: **17/17**.
- Fuzz (heartbeat/pubsub/queue): clean. Soak (queue/pubsub/relay/
  heartbeat): clean. All four dist bundles regenerated at v2.4.6
  (bodies byte-identical to 2.4.5; only the version banner moved).

## [2.4.5] — 2026-06-10

Cyrius 6.x migration. Toolchain pin **5.10.44 → 6.1.24**, and with the
6.x compiler the long-standing sigil crypto-NI blocker finally clears:
sigil moves **2.9.0 → 3.7.8** (latest), the first sigil bump since the
2.4.0 line. No majra API, ABI, or wire-format drift; the four
distribution profiles keep their public surface. All 305 CI assertions
+ 3 fuzz harnesses + 4 soak suites pass under the new toolchain.

### Changed

- **Cyrius toolchain pin 5.10.44 → 6.1.24** (`cyrius.cyml [package].cyrius`).
- **sigil 2.9.0 → 3.7.8** (`[deps.sigil]`). The 2.9.0 pin existed solely
  to dodge the AES-NI / Ed25519-NI `[rbp-N]` asm-offset SIGILL on cyrius
  5.10.x. Under cyrius 6.x that whole failure class is gone — sigil's NI
  asm migrated to the `param_load` pseudo (cyrius 6.0.67+), so the latest
  release rides the toolchain cleanly. Transitively rolls **agnosys 1.0.4
  → 1.3.2** (zero `SYS_OPEN` refs — the dormant aarch64 cross-build
  blocker is resolved as a side effect).
- **Build workflow: `cyrius lib sync` now precedes `cyrius deps`, and
  builds pass `--no-deps`.** Cyrius 6.x split stdlib provisioning
  (`cyrius lib sync` copies the version-pinned 94-file snapshot into
  `./lib/`) from git-dep resolution (`cyrius deps`). A bare `cyrius deps`
  leaves a partial `./lib/` that omits the toolchain modules
  agnosys/sandhi reach into (`slice`, `tls`), and cyrius 6.1.x compiles
  an unresolved call to a runtime-trapping `ud2` rather than failing the
  build — so the omission surfaces as a SIGILL, not a link error.
  `cyrius.lock` now carries 94 hashes (was 3). CI + release workflows
  updated.

### Migrated

- **`src/admin.cyr` → sandhi server API.** The HTTP server surface was
  renamed `http_*` → `sandhi_server_*` in the cyrius 6.x stdlib reorg
  (`http_send_status` → `sandhi_server_send_status`, `http_server_run` →
  `sandhi_server_run`, etc. — same signatures). The `HTTP_*` status
  constants are unchanged. The admin/backends bundles carry the new
  calls; consumers of those profiles must include `lib/sandhi.cyr`.
- **`src/signed_envelope.cyr`: `ct_eq` → `ct_eq_bytes_lens`.** sigil
  retired its bundled `ct_eq` at 3.0.2; the constant-time dual-length
  compare now comes from the stdlib `lib/ct.cyr`. signed/backends
  consumers must include `lib/ct.cyr`.
- **Test/fuzz entry-point include surface widened** for the cyrius 6.x
  stdlib split: `tests/test_backends.tcyr` adds ct/chrono/async/sakshi/
  dynlib/fdlopen/tls; `tests/test_patra_queue.tcyr` and
  `fuzz/fuzz_queue.fcyr` add the `thread` (mutex moved off `sync.cyr`'s
  twin) / `src/metrics.cyr` includes they were transitively relying on.

### Verified

- Core (main.cyr smoke): **150/150**.
- `tests/test_core.tcyr`: **96/96**.
- `tests/test_backends.tcyr`: **42/42** — `aes_gcm_roundtrip`,
  `encrypted_ipc`, `signed_envelope`, and `admin` all green on the
  sigil 3.7.8 surface under cyrius 6.1.24 (these are exactly the paths
  that ud2-SIGILL'd before the `ct_eq` / lib-sync fixes).
- `tests/test_patra_queue.tcyr`: **17/17**.
- Fuzz (heartbeat/pubsub/queue): clean. Soak (queue/pubsub/relay/
  heartbeat): clean. All four dist bundles regenerated at v2.4.5.

## [2.4.4] — 2026-05-11

Cyrius toolchain refresh. No source change; no API, ABI, or
wire-format drift. All 305 CI assertions + 3 fuzz harnesses + 4
soak suites pass under the new pin. Sigil stays held at 2.9.0 —
upstream P1 ([sigil asm stack-frame drift](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md))
is still open at sigil 3.1.1 (the 5/11 sigil patch was the
stdlib annotation pass, not the NI-path fix).

### Changed

- **Cyrius toolchain pin bumped 5.10.34 → 5.10.44** (`cyrius.cyml [package].cyrius`).
  Ten patch-level cyrius releases worth of stdlib / codegen
  bugfixes pulled in via `cyrius deps`. `cyrius.lock` unchanged
  — sigil/sakshi/agnosys all resolve to the same git tags
  (2.9.0 / 2.0.0 / 1.0.0).
- **Dist bundles regenerated** at v2.4.4. Bundle bodies are
  byte-identical to 2.4.3; only the version banner line moved.
  Sizes unchanged: `dist/majra.cyr` 3127 lines / 85 KB,
  `dist/majra-signed.cyr` 3273 lines / 90 KB,
  `dist/majra-admin.cyr` 3259 lines / 90 KB,
  `dist/majra-backends.cyr` 4727 lines / 137 KB.

### Verified

- Core (main.cyr smoke): **150/150**.
- `tests/test_core.tcyr`: **96/96**.
- `tests/test_backends.tcyr`: **42/42** — including
  `signed_envelope`, `aes_gcm_roundtrip`, and `encrypted_ipc`,
  which sit directly on the sigil 2.9.0 surface and would have
  SIGILL'd at the first asm dispatch had the toolchain bump
  perturbed the reference paths.
- `tests/test_patra_queue.tcyr`: **17/17**.
- Fuzz (`cyrius fuzz`): **3/3** harnesses pass (heartbeat / pubsub / queue).
- Soak: **4/4** (queue 5k ops, pubsub 2000 topics, relay dedup +
  eviction, heartbeat 100 nodes × 20 cycles + auto-eviction).
- `cyrius lint src/main.cyr`: 0 warnings.
- `cyrius vet src/main.cyr`: 27 deps, 0 untrusted, 0 missing.
- `cyrius fmt src/main.cyr --check`: clean.

## [2.4.3] — 2026-05-10

`patra_queue` retire-the-workarounds patch. No API or wire-format
change; all 305 assertions still pass. Cleans up the only meaningful
piece of tech debt the 2.4.2 toolchain bump exposed.

### Changed

- **`src/patra_queue.cyr` now uses server-side SQL** for its three
  hot paths. patra (resolved via the cyrius stdlib at v1.9.3 now,
  not the 1.1.1 the workarounds were written against) supports
  `WHERE`, `ORDER BY`, `LIMIT`, and the `COUNT`/`MAX` aggregates:
  - `_pq_load_next_id`: `SELECT MAX(id) FROM jobs` — single-row
    aggregate, no full-table scan to find the largest id at open.
  - `patra_queue_dequeue`: `SELECT * FROM jobs WHERE status = 0
    ORDER BY priority ASC, id ASC LIMIT 1` — the dequeue ordering
    (lower priority number = higher priority, ties broken by id)
    is now server-side. Drops the ~30-line client-side scan + sort.
  - `_pq_count_where_status`: `SELECT COUNT(*) FROM jobs WHERE
    status = N` — single-int aggregate result instead of
    fetching every row to bump a counter.
  Behaviour preserved against `tests/test_patra_queue.tcyr`
  (17/17). Useful at queue sizes where the prior O(n) scans were
  starting to matter; correct at any size.
- **`tests/test_patra_queue.tcyr`** ported from raw `syscall(SYS_UNLINK, ...)`
  to the `sys_unlink()` helper — same arch-portability cleanup as
  `src/ipc.cyr` got in 2.4.2.

## [2.4.2] — 2026-05-10

Toolchain + dep refresh. No source API changes; no consumer-visible
behaviour drift. Brings majra onto the same cyrius/sigil floor as
the rest of the first-party tree (agnosys 1.2.4, agnostik 1.2.1,
libro 3.0.1-track).

### Changed

- **Cyrius toolchain pin bumped 5.4.17 → 5.10.34.** Matches the
  current first-party floor (agnosys/agnostik). Notable upstream
  changes spanning this range: arch-peer include resolution now
  expects `~/.cyrius/versions/<V>/lib` (5.10.9+) — CI installer
  updated accordingly; richer fmt/lint/vet/capacity surfaces;
  DCE (`CYRIUS_DCE=1`) available for release binaries; raised
  fixup cap; stdlib `ct_eq_bytes` family (the prerequisite for
  sigil's 3.0.2 `src/ct.cyr` retirement).
- **Sigil dep held at 2.9.0.** Investigated 2.9.5 and 3.1.0 — both
  fail under cyrius 5.10.34 with SIGILL inside different inline-asm
  hot paths (2.9.5: ed25519; 3.1.0: aes-gcm). The asm blocks in
  sigil 2.9.5+ hardcode `[rbp-N]` parameter offsets that match
  cyrius's pre-5.5 stack-frame layout but drift under 5.10.x's
  expanded prologue. 2.9.0 keeps the software AES + reference
  ed25519 paths (no architecture-specific asm dispatch), so it
  rides through the toolchain bump unchanged. Re-evaluate once
  sigil ships an AES-NI/ed25519 path that emits cyrius-stable
  asm or migrates off raw byte arrays. Filing the offset-drift
  upstream as an issue.
- **`lib/` is no longer committed.** Added `/lib/` to `.gitignore`;
  the directory is repopulated by `cyrius deps` from the
  version-pinned manifest. Matches agnosys / agnostik / yukti /
  patra convention. Prevents stale stubs from prior cyrius
  versions sitting in tree.
- **HTTP server surface moved from vendored copy to stdlib `sandhi`.**
  The old `lib/http_server.cyr` (committed in-tree during the M1
  fold-out window) is gone. `src/admin.cyr` and
  `tests/test_backends.tcyr` now pull `HTTP_BAD_REQUEST` /
  `http_send_status` / `http_server_run` from `lib/sandhi.cyr`,
  which is the cyrius stdlib bundle of sandhi 1.3.3 (folded into
  the stdlib at the M6 milestone). `tls` added to `[deps] stdlib`
  because sandhi references `TLS_EARLY_DATA_ACCEPTED` at parse
  time — without it, cyrius's deps-aware build can't validate
  the dep graph.
- **`src/ipc.cyr` ported to `sys_unlink()`** (was raw
  `syscall(SYS_UNLINK, ...)`). The portable helper picks the right
  syscall per target arch; raw `SYS_UNLINK` is x86_64-only and
  blocks cross-builds. Code-hygiene change — keep using the helpers
  on either side of the syscall boundary so a future aarch64 build
  isn't blocked by majra's own code.
- **aarch64 cross-build is NOT wired into CI.** Tried it; blocked
  downstream-of-the-sigil-pin: with `[deps.sigil] = "2.9.0"` we get
  agnosys 1.0.4 transitively, and that agnosys version's
  `lib/agnosys.cyr:791` uses raw `syscall(SYS_OPEN, ...)` (x86_64-only;
  aarch64 Linux uses `SYS_OPENAT`). The 5.10.34 cc5_aarch64 errors on
  the undefined symbol even with `CYRIUS_DCE=1`. **Note: agnosys
  mainline (1.2.4) has zero `SYS_OPEN` refs** — the bug was fixed
  upstream long ago. We just can't pick up the fixed agnosys without
  bumping past sigil 2.9.0, which is gated on the asm-stack-frame
  drift issue (see roadmap "Waiting on upstream"). When the sigil
  P1 lands, agnosys rolls forward transitively and the aarch64 build
  unblocks. All majra consumers run x86_64 server-side; no blocker
  for shipping 2.4.2 without an aarch64 artifact.
- **CI installer fetches the source archive at the version tag** for
  `lib/` (the stdlib snapshot). 5.10.x release tarballs ship `bin/`
  + `deps/` only — no `lib/`. The official `install.sh` covers this
  via a source bootstrap (`git clone` + self-host build), but CI
  doesn't need the bootstrap path; fetching the tagged source archive
  and copying `lib/` from it is the minimal-cost equivalent.
- **CI / release modernized.** Adopted the agnosys/agnostik pattern:
  versioned `~/.cyrius/versions/<V>/lib` toolchain layout (required
  by 5.10.9+ for arch-peer include resolution), `cyrius deps` step,
  `cyrius.lock` hash verification (best-effort until the lockfile
  lands in-tree), aarch64 cross-build (best-effort when
  `cc5_aarch64` ships), all four `cyrius distlib` profiles in the
  freshness gate, fmt-by-diff (drift detection works around the
  `--check` no-op in cyrius 5.9+).
- **CLAUDE.md** — cyrius pin reference + sigil tag refreshed; quirks
  list trimmed for the cc5 5.10.x floor; lib/ now described as
  resolved-by-`cyrius deps` rather than vendored-in-tree.

## [2.4.1] — 2026-04-20

Docs + soak-test cleanup cycle. No API changes; no new deps.

### Added
- **Three new soak targets** — `soak_pubsub` (2000-topic dispatch), `soak_relay` (dedup correctness + eviction under `max_dedup`), `soak_heartbeat` (register/heartbeat/deregister + auto-eviction). All pass cleanly on 5.4.17. See `tests/soak/README.md`. Completes the soak-test infrastructure seeded in 2.4.0.

### Changed
- **Docs sweep across the tree** — README (v2.4.x module map + 4 dist profiles + sigil-dep note), CLAUDE.md (305-assertion matrix, cc5 5.4.17 quirks, new `map_new_str` guidance), `docs/architecture/overview.md` (new modules + 4-profile matrix), `docs/development/dependency-watch.md` (first-party sigil dep per-profile), `docs/development/threat-model.md` (rows for signed_envelope, admin, patra_queue), `docs/guides/testing.md` (current 341 assertions with separate `test_patra_queue` entry).
- **Roadmap** — QUIC + AES-NI paired as the next sigil arc (sigil 2.10 or 2.9.1 will bundle X25519 + AES-NI dispatch wiring). HKDF-as-gap note removed (shipped in sigil 2.9.0).

## [2.4.0] — 2026-04-20

Engineering-backlog minor release. All four roadmap items shipped;
additive-only (no breaking changes to the 2.3.x surface).

### Changed
- **Cyrius toolchain pin bumped to 5.4.17** (was 5.4.12-1 at start of 2.4.0-dev cycle). Brings in: (a) the `lib/hashmap.cyr` Str-key fix (5.4.14) — new `map_new_str()` + content-derived `hash_str_v`; resolves the ~3% collision rate surfaced by majra's own soak test and filed as `cyrius/docs/development/issues/stdlib-hashmap-str-key-collision.md`; (b) refreshed `lib/fnptr.cyr` and `lib/toml.cyr`; (c) bundled `lib/sigil.cyr` now 2.9.0.
- **Sigil dep bumped 2.8.4 → 2.9.0** (`cyrius.cyml` `[deps.sigil]`, `lib/sigil.cyr` refreshed). 2.9.0 adds HKDF (RFC 5869) and stages the AES-NI scaffold; majra's AES-GCM surface is unchanged on the wire, and the software AES-GCM path still runs (AES-NI is deferred at the sigil layer pending the cc5 inline-asm codegen fix scheduled for 5.5.x — filed at `cyrius/docs/development/issues/inline-asm-stores-silently-drop-when-fn-included.md`).
- **`src/queue.cyr`** switched from `map_new()` to `map_new_str()` for the managed-queue job map. Soak test's `mq_job_count` invariant is now authoritative (was informational-only under the hashmap bug). All 305 assertions pass.

### Added

- **Soak-test infrastructure** (`tests/soak/`) with `soak_queue.cyr`
  as the first target — 5k-round managed-queue lifecycle stress.
  Flushed out a real upstream cyrius stdlib bug along the way:
  `hash_str` in `lib/hashmap.cyr` expects a cstr but is routinely
  called with Str struct pointers (via `map_set(m, str_from_int(id),
  ...)`) — produces ~3% collision rate. Filed upstream at
  `cyrius/docs/development/issues/stdlib-hashmap-str-key-collision.md`.
  Soak test reports the `mq_job_count` (map-backed) discrepancy
  informationally and asserts on counter-backed `mq_total_completed`
  for the authoritative invariant. `tests/soak/README.md` documents
  the workflow.

- **Sigil-signed envelopes** (`src/signed_envelope.cyr`) — Ed25519
  signatures over a deterministic canonical encoding of envelope
  fields (`id_hi|id_lo|timestamp|to_kind|len-prefixed from|to_name|
  payload`). API: `signed_envelope_new(e, sk, pk)` /
  `signed_envelope_verify(se, expected_pk)`. Verify codes: 0 ok,
  1 bad input, 2 pk mismatch, 3 invalid signature. 9 assertions
  in `test_backends` — clean roundtrip, tamper detection, identity
  binding via `expected_pk`.

- **HTTP admin/metrics endpoint** (`src/admin.cyr`) — read-only
  observability surface over `lib/http_server.cyr`. Routes: `/health`,
  `/fleet` (JSON fleet stats), `/ratelimit` (JSON ratelimiter stats).
  Localhost-only by default; NO auth, NO mutation. Operator-facing,
  intended behind a reverse proxy for anything beyond a single host.
  5 assertions in `test_backends` — handler wiring and JSON body
  content. Socket-accept loop test belongs in `test_live` (follow-up).

- **Patra-backed persistent queues** (`src/patra_queue.cyr`) — durable
  alternative to the in-memory managed queue. Single `jobs` table
  in a `.patra` file, survives process restart. API:
  `patra_queue_new(path)` / `patra_queue_enqueue(q, priority, payload)`
   / `patra_queue_dequeue(q)` / `patra_queue_complete(q, id)` /
  `patra_queue_fail(q, id)` plus queued/running/completed counts.
  Priority matches `src/queue.cyr` convention (CRITICAL=0 highest,
  BACKGROUND=4 lowest). 17 assertions in a new `test_patra_queue`
  entry point (separate from `test_backends` to stay under the cc5
  16384 fixup cap) — enqueue, priority-ordered dequeue, complete,
  and reopen-with-persistence verified.

- **Two new dist profiles** to keep the default bundle lean:
  - `[lib.signed]` → `dist/majra-signed.cyr` (core + signed envelopes,
    requires sigil at consume-time) — 3215 lines
  - `[lib.admin]` → `dist/majra-admin.cyr` (core + admin endpoint) —
    3201 lines

### Tests (all suites on 5.4.12-1)

- core (`./build/majra`): 150 pass
- expanded (`tests/test_core.tcyr`): 96 pass
- backends (`tests/test_backends.tcyr`): 42 pass (was 25 in 2.3.1, +17 from
  signed_envelope + admin)
- patra_queue (`tests/test_patra_queue.tcyr`): 17 pass (new entry point)
- **Total: 305 assertions, up from 271 in 2.3.1** (+34)
- Fuzz: 3/3 clean, bench 17/17 clean
- Soak: `soak_queue` runs 5k ops to completion (flags the hashmap
  informational metric as expected)

### Notes

- The patra_queue dequeue and filter paths scan all rows client-side
  because patra 1.1.1 returns a null result set for queries with a
  `WHERE` clause (verified; works for WHERE without problem once given
  the right syntax but our column-list SELECTs returned null for
  reasons that looked schema-dependent — kept the SELECT * + client
  filter path for now; revisit when patra gains a more tolerant SQL
  parser or we adopt column indices directly).
- Admin endpoint is **localhost-only by design** — binding to 0.0.0.0
  without a fronting proxy that handles auth is a misuse.



## [2.3.1] — 2026-04-20

Patch release: wires sigil 2.8.4's real AES-256-GCM into `src/ipc_encrypted.cyr`
(the 2.3.0 stub was non-functional — no downstream consumer was relying on
the previous plaintext-in-base64 behavior), and rolls the Cyrius toolchain
pin forward through the 5.4.9–5.4.12-1 arc. Tests 267 → 271 (+4 from a
revived multi-threaded `cbarrier_arrive_and_wait` case that crashed under
5.4.8 and was fixed upstream in 5.4.10).

### Changed
- **Cyrius toolchain pin bumped to 5.4.12-1** (was 5.4.8 when 2.3.0 shipped). Brings in four upstream fixes: (a) the `_thread_spawn` inline-asm clone trampoline in `lib/thread.cyr` (5.4.10) that fixes the RBP/child-stack race crashing multi-threaded `cbarrier_arrive_and_wait` — see cyrius `docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` (filed by majra 2.3.0); (b) an aarch64 SP-alignment fix in the same trampoline (5.4.11, LDP-pair load instead of two LDRs to avoid SIGBUS); (c) the `cyriusly` version-manager script + arch-peer syscalls packaging restored in 5.4.12 (5.4.11 release tarballs dropped `cyriusly` from `bin/`); (d) the bundled `lib/sigil.cyr` now reliably resolves to 2.8.4 in 5.4.12-1 (5.4.10 and 5.4.12 shipped stale 2.8.3 snapshots — being fully addressed in the 5.4.x closeout by removing hardcoded-version multi-sourcing). majra independently vendors `lib/sigil.cyr` at 2.8.4 per the `[deps.sigil]` pin, so the stdlib bundle version isn't load-bearing here.

### Fixed
- **Multi-threaded `cbarrier_arrive_and_wait` now works.** `tests/test_core.tcyr` revives the 3-thread blocking test that was stubbed-out with a non-blocking-only fallback under 5.4.8. Expanded suite: 92 → 96 assertions. Removed the local `tests/repro_aaw_crash.cyr` — fixed upstream.

### Added
- **Real AES-256-GCM** in `src/ipc_encrypted.cyr` — the crypto path is no longer a stub. Wires in sigil 2.8.4's `aes_gcm_encrypt` / `aes_gcm_decrypt` (NIST SP 800-38D, constant-time tag verification, key zeroization on close).
- **sigil vendored as a dep** — `cyrius.cyml` gains `[deps.sigil] tag = "2.8.4"` pointing at `dist/sigil.cyr`; `lib/sigil.cyr` (bundled ~5.8k lines) is committed so CI doesn't need `cyrius deps` resolution for the backends profile.
- **AES-GCM roundtrip test** in `tests/test_backends.tcyr` — encrypts, decrypts with valid tag, and decrypts with a flipped-bit tag to confirm the AEAD contract (error + zeroed plaintext) holds through the wire layer. Backend suite: 20 → 25 assertions.

### Changed
- **Wire format for encrypted IPC** changed from `base64(nonce || plaintext_stub)` to `nonce(12) || ciphertext(N) || tag(16)` — the real GCM shape, no base64 overhead. Incompatible with any prior (stub-era) frames, but there were no such frames in production: the prior impl was plaintext-in-base64 and never semantically secure.
- **Removed stub AES S-box** from `src/ipc_encrypted.cyr` (was 32 of 256 bytes, never functional). Sigil owns the full FIPS-197 S-box now.
- **`encrypted_ipc_close`** now zeroes the 32-byte key buffer before close (defense-in-depth; was leaving the PSK in memory).

### Docs
- **`docs/development/roadmap.md`** — AES-256-GCM moves from "Open Items" (AES-NI stub) to shipped-via-sigil. AES-NI hardware acceleration remains deferred at the sigil layer (pending Cyrius inline asm).

## [2.3.0] — 2026-04-19

Brings majra onto the modern Cyrius 5.4.x manifest + distribution
convention. No runtime behavior change; this is the scaffold
refresh libro did in its 1.1.0 → 2.0 arc, catching majra up.

### Changed
- **Cyrius toolchain pinned to 5.4.8** (cc5), up from 3.2.6 (cc3). 14-minor jump pulls in: `\r` escape, negative literals, compound assignment, undefined-function-as-error, 16384 fixup cap (up from 8192), and the PE-aware backend from 5.4.8.
- **Manifest `cyrius.toml` → `cyrius.cyml`** — matches first-party convention (libro, yukti, cyrius, sakshi, patra, sigil). Now uses `[package] / [build] / [lib] / [lib.backends] / [deps]` sections. `version = "${file:VERSION}"` makes `VERSION` the single source of truth.
- **CI toolchain resolution**: `.github/workflows/{ci,release}.yml` no longer hardcode `CYRIUS_VERSION`. They grep the pin out of `cyrius.cyml` at install time, same shape as libro / yukti.
- **`scripts/version-bump.sh`** simplified — `cyrius.cyml` uses `${file:VERSION}` so there's nothing to sed in the manifest after a bump.

### Added
- **`dist/majra.cyr`** (core engine, ~3k lines) and **`dist/majra-backends.cyr`** (~4.2k lines, adds redis / postgres / ipc_encrypted / ws). Produced by `cyrius distlib` (default) and `cyrius distlib backends` respectively. Consumers (daimon, AgnosAI, hoosh, sutra, stiva) pick which surface to pull via `[deps.majra] modules = ["dist/majra.cyr" | "dist/majra-backends.cyr"]`. Same distribution contract as libro — see `CLAUDE.md` § Distribution Contract.
- **`[lib.backends]` profile** in `cyrius.cyml` — bundles the 4 backend modules alongside the core 15 for consumers that want the full surface.
- **CI manifest-completeness gate** — asserts every `include "src/*.cyr"` in `src/main.cyr` is listed under `[lib] modules`. Mirrors libro's guard; prevents silently shipping a bundle missing a module.
- **CI dist-freshness gate** — regenerates both bundles and fails if `git diff dist/` is non-empty. Bundles must be regenerated and committed alongside any `src/` change.
- **Release asset**: both `dist/*.cyr` bundles now attached to the GitHub Release alongside the source tarball and `build/majra` binary.

### Docs
- **`CLAUDE.md` rewritten** — dropped cc3-era quirks that are resolved under cc5 (`\r`, negative literals, `+=`, fixup cap, `map_get`-after-`map_set`). Added the distribution contract and CI gates. Build commands reflect `cyrius.cyml` / `cyrius distlib`.
- **`README.md` updated** — `v2.3.0` header, `[deps.majra]` integration snippet, build section reflects `dist/` bundles. Removed the `0 - priority` idiom from the Redis quickstart (cc5 supports negative literals).
- **`docs/architecture/overview.md`** — added "Distribution profiles" table explaining `dist/majra.cyr` vs `dist/majra-backends.cyr`; backends section renamed to `[lib.backends] profile only`; cc3-era "clobbers locals" principle rewritten to reflect cc5 improvement.
- **`docs/development/roadmap.md`** — relay dedup + barrier `arrive_and_wait` moved to "revisit under cc5" (cc3 root cause expected to be fixed); added patra 1.1.1 integration and `lib/http_server.cyr` evaluation items.
- **Relocated stale benchmark dumps** — `benchmark-rustvcyrius2.md` + `benchmarks.md` moved from repo root into `docs/benchmarks/`. Empty `programs/` directory removed.

### Source modernization (cc5 idioms)
- **`src/redis_backend.cyr`** — `_sb_crlf` now uses `str_builder_add_cstr(sb, "\r\n")`; dropped the byte-13/byte-10 `store8` hack and its 4-line scratch buffer. Replaced `return 0 - 1;` with `return -1;`.
- **`src/dag.cyr`** — `map_set(in_degree, sid, 0 - 1)` → `map_set(in_degree, sid, -1)`.
- **`src/main.cyr`** — backend-module include comment reframed: the split is now a distribution-profile decision, not a fixup-cap workaround (cap is 16384 on cc5, up from 8192 on cc3).

### Stdlib refresh
- **17 stdlib modules re-vendored from Cyrius 5.4.8** — `alloc`, `args`, `base64`, `bench`, `chrono`, `fmt`, `fnptr`, `hashmap`, `http`, `json`, `math`, `patra`, `sakshi`, `str`, `string`, `toml`, `vec`. `sakshi_full.cyr` kept as-is (not in upstream).

### Repo hygiene
- **`.gitignore` pruned** — removed Rust-era entries (`/target/`, `criterion/`, `proptest-regressions/`, `supply-chain/.cache/`, `lcov.info`, `tarpaulin-report.html`, `fuzz/target/`) that remained after the 2.0 Rust→Cyrius port. Added `.claude/`.

## [2.2.0] — 2026-04-09

### Changed
- **Cyrius toolchain updated to v3.2.6** (cc3 compiler)
- **Stdlib synced to v3.2.6** — updated `hashmap.cyr`, `hashmap_fast.cyr`, `json.cyr`, `string.cyr`
- **`map_count` → `map_size`** across all source modules (17 call sites) — uses new idiomatic alias
- **Chained `if/break` fix** in `postgres_backend.cyr` — uses compound `||` conditions per cc3 3.2.6 fix
- **Bench file extension**: `bench_all.cyr` → `bench_all.bcyr` for `cyrius bench` auto-discovery

### Added
- **New stdlib modules from 3.2.6**:
  - `patra.cyr` — structured storage, SQL queries, transactions, SHA-256
- **New stdlib functions**:
  - `map_get_or(m, key, default)` / `fhm_get_or(m, key, default)` — get with default value
  - `map_size(m)` / `fhm_size(m)` — count aliases
  - `strstr(haystack, needle)` — substring search

### Fixed
- `json.cyr` upstream fix: chained `if/break` inside while loops (broken in cc3 < 3.2.6)

## [2.1.1] - 2026-04-09

### Changed
- Cyrius toolchain pinned to v3.2.5 (cc3 compiler, minimum version)

## [Unreleased]

## [2.1.0] — 2026-04-09

### Changed
- **Cyrius stdlib synced to v3.2.1** — vendored `lib/` updated from 28 to 35 modules, all existing modules refreshed to upstream
- **Binary size**: 93 KB → 108 KB (expanded stdlib)
- **Build tooling references**: `cc2` / `cyrb` → `cyrius` across README, CONTRIBUTING, dependency-watch docs
- **Test runner**: fixed benchmark invocation (direct build+run instead of `cyrius bench`)

### Added
- **7 new stdlib modules** vendored from Cyrius 3.2.1:
  - `sakshi.cyr` / `sakshi_full.cyr` — structured logging/tracing (v0.8.0, enum-based log levels)
  - `base64.cyr` — base64 encode/decode
  - `chrono.cyr` — timestamp formatting and parsing
  - `csv.cyr` — RFC 4180 CSV parser/writer
  - `hashmap_fast.cyr` — optimised hashmap variant
  - `http.cyr` — minimal HTTP/1.0 client
- **Upstream stdlib improvements** pulled into 9 existing modules:
  - `assert.cyr` — `assert_lt`, `assert_gte`, `assert_lte`, `assert_nonnull`
  - `io.cyr` — file locking: `file_lock`, `file_unlock`, `file_trylock`, `file_lock_shared`
  - `string.cyr` — `atoi()` for string-to-integer parsing
  - `regex.cyr` — bugfix: `str_replace` now uses `str_data`/`str_len` correctly
  - `str.cyr` — bugfix: `str_join` uses `str_builder_add` for Str separators
  - `syscalls.cyr` — inotify wrappers: `sys_inotify_init`, `sys_inotify_add_watch`, `sys_inotify_rm_watch`
  - `hashmap.cyr` — `map_iter` support via fnptr
  - `callback.cyr` — syscalls include for timing
  - `tagged.cyr` — `option_print`/`result_print` support

### Fixed
- Stale `cc2`/`cyrb` references in documentation (README.md, CONTRIBUTING.md, dependency-watch.md)
- Test runner benchmark command (`cyrius bench` → direct build+run of `benches/bench_all.cyr`)

## [2.0.0] — 2026-04-08

**Full port from Rust to Cyrius.** All 19 modules re-implemented from scratch with zero external dependencies.

### Changed
- **Language**: Rust → Cyrius (compiled via `cc2`, statically linked)
- **Build system**: Cargo → `cyrb` / direct `cc2` compilation
- **Dependencies**: 25 Rust crates → 0 (Cyrius stdlib only)
- **Binary output**: library crate → standalone executable (~93 KB)
- **Generics**: `T: Send + Clone + Serialize` → `i64` (pointer to heap struct)
- **Traits**: `MajraMetrics`, `Transport`, `WorkflowStorage` → function pointer vtables
- **Async/await**: tokio → threads + mutexes + futex wait/wake
- **DashMap**: → mutex-protected hashmap
- **Floating point**: `f64` rate tokens → fixed-point i64 (x1000 scaling)
- **UUID**: `uuid` crate → 128-bit random via `getrandom` syscall
- **Timestamps**: `chrono` → `clock_gettime(CLOCK_MONOTONIC)` nanoseconds

### Added
- **Redis backend** (`redis_backend.cyr`) — full RESP2 protocol implementation over TCP: SET/GET/DEL, sorted sets (ZADD/ZPOPMIN/ZCARD), PUBLISH, HSET/HGET, EVAL, KEYS, SETEX, EXPIRE
- **PostgreSQL backend** (`postgres_backend.cyr`) — wire protocol v3: startup, cleartext auth, simple query, row parsing, workflow table DDL/CRUD
- **WebSocket** (`ws.cyr`) — RFC 6455: SHA-1 implementation (RFC 3174), base64 encode/decode, WebSocket handshake (Sec-WebSocket-Accept), frame send/recv with masking, ping/pong
- **Encrypted IPC** (`ipc_encrypted.cyr`) — AES-256-GCM framing with nonce management, base64 wire encoding, key rotation. Crypto stubs ready for AES-NI (x86_64) and aarch64 intrinsics
- **295 test assertions** across 4 suites: core (144), expanded (92), backends (25), live (36)
- **17 benchmarks** covering all major operations
- **2 examples**: managed_queue, pubsub_tiers
- **Test runner**: `tests/test.sh` runs all suites + benchmarks

### Removed
- **QUIC transport** — deferred until sigil crypto port (TLS 1.3 dependency)
- **SQLite persistence** — no SQLite binding in Cyrius
- **Prometheus metrics** — replaced by generic function pointer vtable
- **Logging module** — `println` suffices

### Known Issues
- Cyrius compiler local variable clobbering across function calls — mitigated via globals
- Relay dedup and barrier `arrive_and_wait` affected by hashmap lookup issue in nested call contexts
- No `\r` escape in Cyrius string literals — RESP/HTTP/WebSocket use raw byte 13

## [1.0.4]

### Changed
- **License changed from AGPL-3.0-only to GPL-3.0-only** — updated `Cargo.toml`, `deny.toml`, `README.md`, `CONTRIBUTING.md`, `CLAUDE.md`, and `LICENSE` file
- **Dependencies updated** — 25 packages bumped to latest compatible versions (ICU 2.1→2.2, wasm-bindgen 0.2.115→0.2.117, libc 0.2.183→0.2.184, and others)

## [1.0.3]

### Fixed
- **`ws` feature missing `futures-util` dependency** — `ws` feature used `futures_util::{SinkExt, StreamExt}` but did not gate `dep:futures-util`, causing compilation failure when `ws` was enabled without `redis-backend` (which happened to bring `futures-util` in under `full`)

## [1.0.2]

### Changed
- **`redis` dependency upgraded from 0.27 to 1.x** — aligns with redis crate stable 1.0 release. No API changes required; `get_multiplexed_async_connection()`, `AsyncCommands`, `Script::invoke_async()` remain compatible. Consumers pinned to `redis 0.27` via majra can now use `redis 1.x` directly without version conflicts.

## [1.0.1]

### Added
- `EncryptedIpcConnection::rekey()` — key rotation API with nonce counter reset
- `EncryptedIpcConnection::needs_rekey()` / `messages_sent()` — nonce exhaustion tracking (warns at 2^31, errors at 2^32)
- `SlidingWindowLimiter` — approximate sliding-window rate limiter (~5% accuracy of exact, O(1) memory/time per key)
- `WorkflowEngine::resume()` — durable workflow execution: reload step results from storage, skip completed steps, resume from interruption point
- `ConnectionPool::with_circuit_breaker()` — per-endpoint circuit breaker (configurable failure threshold + cooldown)
- `CircuitBreakerConfig`, `CircuitState` — circuit breaker types (Closed/Open/HalfOpen)
- `ConnectionPool::circuit_state()` / `reset_circuit()` — circuit breaker introspection and manual reset
- `Relay::compact_dedup()` — DashMap shrink-to-fit to reclaim dead capacity after eviction
- `RateLimiter::compact()` / `SlidingWindowLimiter::compact()` — DashMap shrink-to-fit
- `NamespacedMetrics` — per-tenant metrics partitioning via prefix delegation
- Subscriber count warning at 40+ receivers per pattern (broadcast quadratic slowdown)
- Cached Redis Lua script SHA for `RedisRateLimiter` (EVALSHA optimization)
- `DirectChannel<T>` — zero-overhead broadcast channel, 73M msg/s, no topic routing
- `HashedChannel<T>` + `TopicHash` — hashed topic routing with coarse timestamp, 16M msg/s
- `TypedPubSub<T>` dual-pipe refactor — exact-topic subscribers use O(1) DashMap lookup (fast path), wildcard-only patterns iterate (slow path)
- 7 new dual-pipe + DirectChannel + HashedChannel benchmarks
- 4 new `SlidingWindowLimiter` tests

### Changed
- `TypedPubSub` internal storage split into `exact_subscriptions` + `pattern_subscriptions` for O(1) exact-topic publish
- `PostgresWorkflowStorage::connect_with_pool_size()` documents pool sizing formula (`cores * 2 + 1`, 10 MB/connection)
- Architecture overview documents three-tier pub/sub, circuit breaker, DashMap fragmentation mitigation

## [1.0.0] — 2026-03-26

**First stable release.** API freeze. Full feature coverage across pub/sub, queues, relay, IPC, heartbeat, rate limiting, barriers, DAG workflows, fleet scheduling, and distributed backends.

### Added

#### DAG workflow engine (`dag` feature)
- `WorkflowEngine<S, E>` — tier-based DAG executor with parallel step scheduling, retry with exponential backoff, and 4 error policies (Fail/Continue/Skip/Fallback)
- `TriggerMode` — `All` (AND) and `Any` (OR) join semantics for dependency resolution
- `WorkflowStorage` trait — db-agnostic async storage for definitions, runs, and step runs
- `StepExecutor` trait — consumer-defined step execution logic
- `InMemoryWorkflowStorage` — DashMap-backed default storage with retention policy (`evict_older_than`, `with_max_runs`)
- `SqliteWorkflowStorage` — SQLite-backed storage (behind `sqlite` feature)
- `topological_sort_tiers()` — modified Kahn's algorithm returning parallelizable tiers with trigger-mode-aware in-degree
- `WorkflowDefinition`, `WorkflowRun`, `StepRun` — full execution tracking types
- `WorkflowContext` — step output accumulation for downstream reference
- Validation: cycle detection, referential integrity for deps and fallbacks
- Cooperative cancellation via `AtomicBool` per run

#### Multi-tenant scoping (`namespace` module)
- `Namespace` — prefix-based tenant isolation for topics, keys, and node IDs
- `topic()`, `key()`, `node_id()`, `pattern()`, `wildcard()` — scoped identifier builders
- `strip_topic()`, `strip_key()` — reverse mapping to extract bare identifiers

#### PostgreSQL storage backend (`postgres` feature)
- `PostgresWorkflowStorage` — `WorkflowStorage` impl backed by `deadpool-postgres` connection pool
- `PostgresQueueBackend` — PostgreSQL persistence for `ManagedQueue` (mirrors `SqliteBackend` API)
- `ManagedQueue::with_postgres()` constructor
- Automatic table creation with `majra_` prefix
- `connect()`, `connect_with_pool_size()`, and `from_pool()` constructors

#### IPC encryption (`ipc-encrypted` feature)
- `EncryptedIpcConnection` — AES-256-GCM wrapper around `IpcConnection` using `ring`
- Pre-shared 256-bit key, monotonic nonce counter per direction
- `send()` / `recv()` encrypt/decrypt JSON payloads transparently

#### WebSocket bridge for pubsub (`ws` feature)
- `WsBridge` — bridges `PubSub` topics to WebSocket clients via `tokio-tungstenite`
- Clients subscribe via `{"subscribe": "pattern"}` JSON handshake
- `WsBridgeConfig` — configurable `max_connections` (default 1024)

#### Distributed rate limiting (`redis-backend` feature)
- `RedisRateLimiter` — distributed token-bucket rate limiter via atomic Redis Lua script
- Auto-expiring keys, compatible API style with in-process `RateLimiter`

#### Distributed heartbeat tracker (`redis-backend` feature)
- `RedisHeartbeatTracker` — cross-instance health coordination via Redis key TTLs
- `register()`, `heartbeat()`, `is_online()`, `get_metadata()`, `list_online()`, `deregister()`

#### Typed pub/sub (`TypedPubSub<T>`)
- `TypedPubSub<T>` — generic, type-safe pub/sub hub with backpressure, replay, and filters
- `BackpressurePolicy` — `DropOldest` (default) or `DropNewest`
- Automatic dead-subscriber cleanup on publish (configurable interval)
- `try_subscribe()` — capacity-checked subscription with `max_subscriptions` limit

#### Rate limiter enhancements
- `evict_stale(max_idle)` — periodic sweep of idle keys
- `RateLimitStats` — `total_allowed`, `total_rejected`, `active_keys`, `total_evicted`

#### Relay enhancements
- `send_request()` / `reply()` — request-response correlation via UUID and oneshot channels
- `evict_stale_dedup(max_idle)` — TTL-based dedup table eviction
- `evict_stale_requests(timeout)` — TTL-based pending request cleanup
- `set_max_dedup_entries()` — configurable dedup table cap with LRU eviction
- `RelayMessage::correlation_id` and `is_reply` fields

#### Observability & logging
- `metrics` module — `MajraMetrics` trait with no-op default and Prometheus implementation
- `NamespacedMetrics` — per-tenant metrics partitioning via prefix delegation
- `logging` feature — structured tracing via `MAJRA_LOG` env var
- Structured `#[instrument]` spans on ManagedQueue operations

#### Distributed primitives
- `AsyncBarrierSet` — async barrier with `arrive_and_wait()` and `AtomicBool` release flag
- `transport` module — `Transport` trait, `TransportFactory`, `ConnectionPool` with stale eviction
- `ConnectionPool::evict_stale(max_idle)` — TTL-based idle connection cleanup

#### Code quality
- `#[non_exhaustive]` on all public enums
- `#[must_use]` on all pure return types
- `#[inline]` on all hot-path accessors
- `///` doc comments on every public item
- `Counter` and `evict_from_dashmap` utilities

#### Repository infrastructure
- GitHub Actions CI (10-job pipeline) and release workflow
- LICENSE, CONTRIBUTING.md, SECURITY.md, CODE_OF_CONDUCT.md
- Makefile, `deny.toml`, `codecov.yml`, `rust-toolchain.toml`
- Fuzz targets (queue, pubsub, heartbeat)
- `supply-chain/` (cargo-vet), `scripts/version-bump.sh`
- `benchmarks.md` — 3-point trend tracking
- `docs/development/dependency-watch.md` — pinned versions and upgrade paths
- Live Redis integration test (`redis_live_full_lifecycle`) covering pub/sub, queue, rate limiter, heartbeat
- Live PostgreSQL integration test (`postgres_live_workflow_storage`) covering workflow CRUD
- 220 tests (unit + integration + doc-tests), 25+ benchmarks

### Changed
- `matches_pattern()` rewritten to iterative zero-allocation with inline depth tracking
- `ManagedQueue::dequeue()` releases tiers lock before DashMap mutation
- `ManagedQueue::cancel()` drops DashMap guard before awaiting tiers lock
- `RateLimiter` internals swapped from `Mutex<HashMap>` to `DashMap`
- `Relay` dedup map swapped to `DashMap`, stats to `AtomicU64`
- `ConnectionPool::acquire()` drops lock before async connect
- `PostgresWorkflowStorage::connect_with_pool_size()` — configurable pool size (was hardcoded to 16)
- Replay buffer fast-path for exact topic subscriptions (O(1) vs O(n) pattern scan)

### Fixed
- `AsyncBarrierSet::arrive_and_wait()` missed-wakeup race
- `TypedPubSub::publish()` delivered counter accuracy under `DropNewest`
- SQLite `persist()` no longer panics on serialisation failure
- IPC `write_frame` uses `u32::try_from` to prevent silent truncation

## [0.22.3] — 2026-03-22

### Changed
- Version bump for stiva 0.22.3 ecosystem release

## [0.21.3] - 2026-03-21

### Added

#### Thread safety
- `ConcurrentPriorityQueue<T>` — async-aware wrapper with `Notify`-based blocking dequeue
- `ConcurrentHeartbeatTracker` — `DashMap`-backed tracker with all `&self` methods
- `ConcurrentBarrierSet` — `DashMap`-backed barrier manager
- Compile-time `Send + Sync` assertions on all public types

#### Managed queue (`ManagedQueue<T>`)
- `ResourceReq` / `ResourcePool` — GPU-aware dequeue filtering
- `ManagedQueueConfig` — max concurrency enforcement
- `JobState` enum — `Queued → Running → Completed / Failed / Cancelled`
- `ManagedItem<T>` — lifecycle-tracked queue item
- `QueueEvent` — broadcast events on state transitions
- TTL-based eviction via `evict_expired()`
- `sqlite` feature — `SqliteBackend` persistence with WAL mode

#### Fleet & heartbeat
- `GpuTelemetry`, `FleetStats`, `EvictionPolicy`
- `register_with_telemetry()`, `heartbeat_with_telemetry()`, `fleet_stats()`

#### Error types
- `MajraError::InvalidStateTransition`, `ResourceUnavailable`, `Persistence`

### Changed
- `RateLimiter` and `Relay` internals to `DashMap` + `AtomicU64`

## [0.21.0] - 2026-03-21

### Added
- `envelope` — Universal message envelope with Target routing
- `pubsub` — Topic-based pub/sub with MQTT-style wildcard matching
- `queue` — Multi-tier priority queue with DAG dependency scheduling
- `relay` — Sequenced, deduplicated inter-node message relay
- `ipc` — Length-prefixed framing over Unix domain sockets
- `heartbeat` — TTL-based health tracking with Online → Suspect → Offline FSM
- `ratelimit` — Per-key token bucket rate limiter
- `barrier` — N-way barrier synchronisation with deadlock recovery
- `error` — Shared error types (MajraError, IpcError)
- Feature-gated modules: default = pubsub + queue + relay + heartbeat

[Unreleased]: https://github.com/MacCracken/majra/compare/v1.0.4...HEAD
[1.0.4]: https://github.com/MacCracken/majra/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/MacCracken/majra/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/MacCracken/majra/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/MacCracken/majra/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/MacCracken/majra/compare/v0.22.3...v1.0.0
[0.22.3]: https://github.com/MacCracken/majra/compare/v0.21.3...v0.22.3
[0.21.3]: https://github.com/MacCracken/majra/compare/v0.21.0...v0.21.3
[0.21.0]: https://github.com/MacCracken/majra/releases/tag/v0.21.0
