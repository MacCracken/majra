# Soak tests

Long-running correctness stress tests for majra's primary state
machines. Not part of the default CI pass — run on-demand before
a release when a code change could plausibly affect queue
lifecycle, relay dedup, pub/sub fan-out, barrier cycles, or
heartbeat eviction.

## Running

```
cyrius build tests/soak/soak_queue.cyr build/soak_queue
./build/soak_queue
```

Each soak file prints a banner, runs to completion, and exits 0
on success / non-zero on invariant violation.

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
