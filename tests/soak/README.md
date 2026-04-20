# Soak tests

Long-running correctness stress tests for majra's primary state
machines. Not part of the default CI pass — run on-demand before
a release when a code change could plausibly affect queue
lifecycle, relay dedup, pub/sub fan-out, barrier cycles, or
heartbeat eviction.

## Running

```
cyrius build tests/soak/soak_queue.cyr     build/soak_queue     && ./build/soak_queue
cyrius build tests/soak/soak_pubsub.cyr    build/soak_pubsub    && ./build/soak_pubsub
cyrius build tests/soak/soak_relay.cyr     build/soak_relay     && ./build/soak_relay
cyrius build tests/soak/soak_heartbeat.cyr build/soak_heartbeat && ./build/soak_heartbeat
```

Each soak file prints a banner, runs to completion, and exits 0
on success / non-zero on invariant violation.

## Current targets

| File              | Surface                                                  | Load |
|-------------------|----------------------------------------------------------|------|
| `soak_queue`      | ManagedQueue lifecycle (enqueue, priority dequeue, complete, invariants) | 1000 rounds × 5 jobs = 5k ops |
| `soak_pubsub`     | pubsub_new + topic-map growth + subscribe + publish dispatch | 2000 distinct topics |
| `soak_relay`      | Relay dedup correctness + eviction under max_dedup cap   | 20 senders × 50 msgs × 2 passes + 200-sender eviction phase |
| `soak_heartbeat`  | Register/heartbeat/deregister cycles + auto-eviction     | 100 nodes × 20 heartbeats + offline-timeout phase |

## Known limitations

- Iteration counts are chosen to stay within the bump allocator's
  working set. `cyrius alloc()` doesn't free, and several majra
  paths (e.g. `_next_job_key` → `str_from_int` in `src/queue.cyr`)
  leak small buffers per operation. A proper multi-million-op soak
  will need either a freelist-backed key alloc or arena reset hooks
  — tracked as follow-up work.
- Map-size invariants that depend on Str-keyed hashmaps are affected
  by an upstream cyrius stdlib bug (`hash_str` treats Str struct
  pointers as cstrs, ~3% collision rate). Soak tests report these
  counts informationally and assert on counter-backed invariants
  instead. See `cyrius/docs/development/issues/stdlib-hashmap-str-key-collision.md`.

## Adding a new soak target

Name: `soak_<subsystem>.cyr`. Each file is a standalone program
with its own `fn main()`. Keep includes minimal — pull only what
the subsystem needs. Report tallies at the end; return 0 on pass.
