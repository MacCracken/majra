# Soak tests

Long-running correctness stress tests for majra's primary state
machines. Not part of the default CI pass — run on-demand before
a release when a code change could plausibly affect queue
lifecycle, relay dedup, pub/sub fan-out, barrier cycles, or
heartbeat eviction.

## Running

```
cyrius build --no-deps tests/soak/soak_queue.cyr     build/soak_queue     && ./build/soak_queue
cyrius build --no-deps tests/soak/soak_pubsub.cyr    build/soak_pubsub    && ./build/soak_pubsub
cyrius build --no-deps tests/soak/soak_relay.cyr     build/soak_relay     && ./build/soak_relay
cyrius build --no-deps tests/soak/soak_heartbeat.cyr build/soak_heartbeat && ./build/soak_heartbeat
```

Each soak file prints a banner, runs to completion, and exits 0
on success / non-zero on invariant violation.

## Current targets

| File              | Surface                                                  | Load |
|-------------------|----------------------------------------------------------|------|
| `soak_queue`      | ManagedQueue lifecycle (enqueue, priority dequeue, complete, invariants) | 100,000 rounds × 5 jobs = 500k ops |
| `soak_pubsub`     | pubsub_new + topic-map growth + subscribe + publish dispatch | 2000 distinct topics |
| `soak_relay`      | Relay dedup correctness + eviction under max_dedup cap   | 20 senders × 50 msgs × 2 passes + 200-sender eviction phase |
| `soak_heartbeat`  | Register/heartbeat/deregister cycles + auto-eviction     | 100 nodes × 20 heartbeats + offline-timeout phase |

## Known limitations

- The pubsub, relay and heartbeat soaks are sized to finish in seconds,
  not to probe memory growth. `soak_queue` was the one held down by a
  per-operation leak — job keys came from the bump allocator, capping it
  at 1,000 rounds — and has run 100,000 since 2.8.1 moved them to a
  freelist block that `queue_item_free` releases.
- The Str-keyed job map hashes by content (`map_new_str`, cyrius-quirks
  §3). The ~3% `hash_str` collision majra's soaks surfaced in 2026-04
  is fixed upstream; that cyrius issue is archived.

## Adding a new soak target

Name: `soak_<subsystem>.cyr`. Each file is a standalone program
with its own `fn main()`. Keep includes minimal — pull only what
the subsystem needs. Report tallies at the end; return 0 on pass.
