# Testing Guide

## Running Tests

```bash
# One-time per checkout / after any toolchain or dep bump (cyrius 6.x):
#   lib sync provisions the stdlib snapshot; deps writes and verifies cyrius.lock
#   (majra declares **zero** git deps since 2.6.8 — sigil and sakshi are folded stdlib modules, so `cyrius deps` resolves nothing and exists only for the lockfile). `--full` is load-bearing since cyrius 6.4.x — a bare
#   `lib sync` copies only the declared `[deps].stdlib` subset and omits the
#   toolchain modules sigil/sandhi reach into, which compile to a runtime
#   `ud2` (SIGILL at runtime, not a build error).
cyrius lib sync --full && cyrius deps

# Full audit (fmt + lint + docs + tests + bench); vet / deny are separate subcommands
cyrius audit
cyrius vet src/main.cyr
cyrius deny src/main.cyr

# Individual test suites (manual) — build with --no-deps so the build's
# auto-deps doesn't perturb the lib-synced ./lib/ (see CLAUDE.md § Quick Start).
cyrius build --no-deps src/main.cyr                build/majra            && ./build/majra
cyrius build --no-deps tests/test_core.tcyr        build/test_core        && ./build/test_core
cyrius build --no-deps tests/test_backends.tcyr    build/test_backends    && ./build/test_backends
cyrius build --no-deps tests/test_patra_queue.tcyr build/test_patra_queue && ./build/test_patra_queue

# Live integration tests (requires Redis on :6379, PostgreSQL on :5432)
cyrius build --no-deps tests/test_live.tcyr build/test_live && ./build/test_live

# Soak tests (on-demand, not in CI)
cyrius build --no-deps tests/soak/soak_queue.cyr build/soak_queue && ./build/soak_queue
```

## Test Suites

| Suite | File | Assertions | Coverage |
|-------|------|-----------|----------|
| Core | `src/main.cyr` | 203 | All 15 core modules + relay dedup, envelope-id entropy (non-zero *and* distinct), `_majra_sleep_ns` floor |
| Expanded | `tests/test_core.tcyr` | 645 | Deep: queue lifecycle, pubsub patterns, DAG retry (outcome + elapsed backoff), fleet routing, circuit breaker, integration, multi-threaded barrier (proves blocking, not just a counter), concurrency + wildcard-alignment, relay / ratelimit / priority-queue regressions. Per-release additions: CHANGELOG.md |
| Backends | `tests/test_backends.tcyr` | 547 | base64, SHA-1, AES-256-GCM, signed envelopes, admin endpoint, WebSocket, RESP, PG wire |
| Patra queue | `tests/test_patra_queue.tcyr` | 71 | Durable enqueue / priority dequeue / complete / counts / reopen persistence |
| Live | `tests/test_live.tcyr` | 36 | 7 Redis + 4 PostgreSQL test functions in `tests/test_live.tcyr`. **CI-only** — needs Redis on :6379 + PostgreSQL on :5432, so a dev-box "full matrix" run is 1,466, not 1,502 |
| **Total** | | **1,502** (1,466 CI + 36 live) | |

Every non-live suite, fuzz harness, soak, example and the benchmark binary also passes cross-built for aarch64 under `qemu-aarch64` as of 2.7.3 — see [aarch64 cross-build](#aarch64-cross-build-qemu-user) below.

`test_patra_queue` was split out at 2.4.0 to stay under the then-16384 cc5 fixup cap (patra pulls sakshi + io + fs transitively); the table starts at 1,048,576 entries and grows on demand at the 6.6.6 pin (growable since cyrius 6.2.0), and the split is kept as documented architecture.

## Test Categories

> Counts below are indicative of coverage SHAPE, not exact totals — they were
> apportioned at 2.5.x and the suites have roughly doubled since. The
> authoritative per-suite numbers are the table above and
> [`../development/state.md`](../development/state.md).

| Category | Where | Count |
|----------|-------|-------|
| Error/envelope/namespace | `src/main.cyr` | ~30 |
| Queue (priority, lifecycle, cancel) | `src/main.cyr` + `tests/test_core.tcyr` | ~30 |
| PubSub (patterns, wildcards, filters) | `src/main.cyr` + `tests/test_core.tcyr` | ~25 |
| Relay (send, subscribe, routing) | `src/main.cyr` + `tests/test_core.tcyr` | ~15 |
| Barrier (sync, concurrent, force) | `src/main.cyr` + `tests/test_core.tcyr` | ~15 |
| Heartbeat (FSM, telemetry, eviction) | `src/main.cyr` + `tests/test_core.tcyr` | ~25 |
| RateLimit (bucket, window, keys) | `src/main.cyr` + `tests/test_core.tcyr` | ~15 |
| Fleet (routing, submit, rebalance) | `src/main.cyr` + `tests/test_core.tcyr` | ~10 |
| DAG (linear, parallel, retry, skip) | `src/main.cyr` + `tests/test_core.tcyr` | ~12 |
| Transport (circuit breaker, pool) | `src/main.cyr` + `tests/test_core.tcyr` | ~10 |
| Crypto/protocol (base64, SHA-1, RESP, PG) | `tests/test_backends.tcyr` | ~25 |
| Live Redis | `tests/test_live.tcyr` | ~20 |
| Live PostgreSQL | `tests/test_live.tcyr` | ~16 |
| Integration (training job, namespaced pubsub) | `tests/test_core.tcyr` | ~10 |

## Benchmarks

```bash
# Run with automatic history tracking
cyrius bench

# Manual run
cyrius build --no-deps benches/bench_all.bcyr build/bench_all && ./build/bench_all
```

19 benchmarks covering: envelope creation, priority queue, pattern matching (4 variants), pubsub publish, direct channel, heartbeat, fleet stats, rate limiting, relay send, barrier cycle, circuit breaker, counter increment, managed-queue lifecycle and 4-producer enqueue (the last two since 2.8.2).

## Soak tests

`tests/soak/` holds on-demand stress tests. Not in CI; run before releases or when a code change could plausibly affect a primary state machine. Each soak file is standalone: `fn main()`, returns 0 on pass, non-zero on invariant violation. See `tests/soak/README.md` for conventions and known limitations (bump-allocator-bounded iteration counts).

Currently shipped (all four are run before a release, not in CI):
- `soak_queue.cyr` — 5k ops (1k rounds × 5 jobs) managed-queue lifecycle; asserts `mq_total_completed`, `mq_job_count`, and per-round `queued_count`/`running_count` invariants.
- `soak_pubsub.cyr` — 2000 distinct topics through `pubsub_new` + topic-map growth + subscribe + publish dispatch.
- `soak_relay.cyr` — relay dedup correctness + eviction under the `max_dedup` cap; 20 senders × 50 msgs × 2 passes, then a 200-sender eviction phase.
- `soak_heartbeat.cyr` — register / heartbeat / deregister cycles + auto-eviction; 100 nodes × 20 heartbeats, then an offline-timeout phase.

## aarch64 cross-build (qemu-user)

CI only **cross-builds** the four suites for aarch64 (a build-only gate that fails on syscall / symbol diagnostics); the run half needs `qemu-user` on the runner and is still a roadmap item, so run this before a release and after touching anything that issues a syscall. cyrius's aarch64 backend renumbers 60 source numbers at runtime at the 6.6.6 pin (54 x86_64 numbers plus its six ≥1000 private aliases; 44 at 6.6.4 — 6.6.5 routed `nanosleep` 35 and `sendto` 44 among fourteen more, 6.6.6 added `statfs`/`fstatfs`) and passes every other number through **verbatim**, so a stray x86_64 number is not a build error — it is a different, valid syscall. Through 2.7.2 majra shipped three of them (fchmod 91 → capset, getrandom 318 → ENOSYS, nanosleep 35 → unlinkat) and two built without a warning on any toolchain (CHANGELOG.md § [2.7.3]), which is why a build-only `--aarch64` step proves little here (it catches a `var SYS_*` that redefines a stdlib name, like the 318 site, but not a raw literal or a uniquely named constant, like 35 and 91): **the binaries have to run.**

`qemu-aarch64` comes from the distro's `qemu-user` package (Arch: `qemu-user`, installs `/usr/bin/qemu-aarch64`). The same `lib/` serves both arches — `--aarch64` selects the peer stdlib and the emitter — so no second `lib sync`.

```bash
# Every entry point CI runs, plus the four soaks, cross-built and run under qemu-user. The live suite
# cross-builds too but needs Redis + PostgreSQL to run (see Live Test Setup).
# Shape: cyrius build --aarch64 --no-deps <entry> build/<name>_aarch64 && qemu-aarch64 ./build/<name>_aarch64
cyrius build --aarch64 --no-deps src/main.cyr                build/majra_aarch64            && qemu-aarch64 ./build/majra_aarch64
cyrius build --aarch64 --no-deps tests/test_core.tcyr        build/test_core_aarch64        && qemu-aarch64 ./build/test_core_aarch64
cyrius build --aarch64 --no-deps tests/test_backends.tcyr    build/test_backends_aarch64    && qemu-aarch64 ./build/test_backends_aarch64
cyrius build --aarch64 --no-deps tests/test_patra_queue.tcyr build/test_patra_queue_aarch64 && qemu-aarch64 ./build/test_patra_queue_aarch64
cyrius build --aarch64 --no-deps benches/bench_all.bcyr      build/bench_all_aarch64        && qemu-aarch64 ./build/bench_all_aarch64   # all 19 targets print; the numbers are TCG's, not the CPU's

for f in fuzz/*.fcyr; do            # usage: <harness> [iterations] [seed]; the seed is printed, and an oracle failure exits 1 with it
  n=$(basename "$f" .fcyr); cyrius build --aarch64 --no-deps "$f" "build/${n}_aarch64" && qemu-aarch64 "./build/${n}_aarch64" || { echo "FUZZ CRASH: $n"; exit 1; }
done
for f in tests/soak/*.cyr examples/*.cyr; do
  n=$(basename "$f" .cyr);  cyrius build --aarch64 --no-deps "$f" "build/${n}_aarch64" && qemu-aarch64 "./build/${n}_aarch64" || { echo "FAIL: $n"; exit 1; }
done
```

Timing under qemu is not native timing: TCG translates each path cold on first touch, so the **first** `ratelimit_check` costs ~1.2 ms where four native checks take ~14 µs. Any test that asserts inside a millisecond window will fail here for a reason unrelated to the code under test — budget the window in whole seconds (the ratelimit tests' burst buckets are 1 token/sec for exactly this).

### Loop it

A single green run proves little on a seed-dependent path: the `cbarrier_arrive_and_wait` defect fixed at 2.7.3 crashed **~7 % of native `tests/test_core.tcyr` runs at 2.7.2 (14 / 200)** and a green CI step hid it (CHANGELOG.md § [2.7.3]). Loop the expanded suite and count exit code 139 (qemu re-raises the guest's SIGSEGV on the host, so the shell sees it as it would natively):

```bash
crashes=0
for i in $(seq 1 100); do
  qemu-aarch64 ./build/test_core_aarch64 > /dev/null 2>&1
  [ $? -eq 139 ] && crashes=$((crashes + 1))
done
echo "SIGSEGV: $crashes / 100"
```

⚠ **Run the loop sequentially, one suite at a time.** The suites use fixed
paths — `/tmp/majra_patra_*.patra`, the `ipc_bind` socket paths,
`/tmp/majra_ipc_mode_test.sock` — so concurrent copies of the same suite
overwrite each other's files. A `xargs -P8` loop at 2.8.2 reported 24/25
patra-queue failures and two SIGSEGVs that were pure self-interference; the same
binaries were 0/25 run one at a time.

2.7.3 baseline: `test_core` 0 / 100 under qemu and 0 / 200 native; core / backends / patra-queue 25× under qemu and 40× native, 0 failures. A non-zero count is a finding even when the plain run passes.

### Localising a crash

cyrius binaries carry no symbol table, so gdb shows every frame as `?? ()`. The recipe that found the barrier defect (`_map_find+0xf8`, reading through a hashmap *entry* that had been passed as a map):

1. **Build with a symbol map.** `CYRIUS_SYMS=<file>` makes the compiler write one `<hex VA> <name>` line per function, sorted by address:

   ```bash
   CYRIUS_SYMS=build/test_core_aarch64.syms \
     cyrius build --aarch64 --no-deps tests/test_core.tcyr build/test_core_aarch64
   ```

2. **Run under qemu's gdbstub and attach.** Plain `gdb` works if it was built with aarch64 support (Arch's is); otherwise `gdb-multiarch`. `continue` runs to the fault; the last command prints the faulting `pc` and the frame pointer:

   ```bash
   qemu-aarch64 -g 1234 ./build/test_core_aarch64 &
   gdb -batch -ex "set architecture aarch64" \
       -ex "target remote localhost:1234" \
       -ex continue \
       -ex "info registers pc x29" \
       ./build/test_core_aarch64
   ```

3. **Resolve `pc` against the map** — the last symbol at or below it (substitute the `pc` step 2 printed; addresses differ per binary, and the 2.7.3 fault resolved to `_map_find+0xf8`):

   ```bash
   awk -v pc=0x415f4c 'BEGIN{pc=strtonum(pc)} {va=strtonum("0x"$1); if (va<=pc){sym=$2; base=va}} END{printf "%s+0x%x\n", sym, pc-base}' build/test_core_aarch64.syms
   # _map_find+0xf8
   ```

4. **Read the frame.** cyrius's aarch64 frame convention is **parameters first, then locals, in declaration order, one 8-byte slot each below the frame pointer**: the first parameter at `[x29-8]`, the second at `[x29-16]`, the first local after the last parameter, and so on (`_map_find(m, key)` spills `x0` → `[x29-8]`, `x1` → `[x29-16]`, and its first local lands at `[x29-24]`). `x/4gx $x29-32` in the same session dumps the first four slots. `[x29]` is the caller's saved `x29` and `[x29+8]` the return address, so when the fault is inside a stdlib function — as it was — walk one frame up and read the caller's slots the same way; resolve the return address against the map to name it.

## Compiler Limitations

Cyrius's fixup table at the 6.6.6 pin starts at 1,048,576 forward references and grows ×2 on demand (8192 on cc3, 16384 across the cc5 5.x line; growable since 6.2.0). Large test entry points that aggregate many src modules are no longer near it — `test_patra_queue` was split out under the 16384 cap and the split is kept as documented architecture — but the advice stands: if adding significant new test code causes a "fixup table full" error, create a new `.tcyr` entry point scoped to what you're testing.

## Live Test Setup

```bash
# Start Redis and PostgreSQL
docker run -d --name majra-redis -p 6379:6379 redis:7-alpine
docker run -d --name majra-postgres -p 5432:5432 \
  -e POSTGRES_PASSWORD=majra -e POSTGRES_DB=majra postgres:16-alpine

# Configure PostgreSQL for cleartext auth (required by Cyrius PG client)
docker exec majra-postgres su postgres -c \
  "echo 'host all all 0.0.0.0/0 password' > /var/lib/postgresql/data/pg_hba.conf && \
   echo 'local all all trust' >> /var/lib/postgresql/data/pg_hba.conf && \
   pg_ctl reload -D /var/lib/postgresql/data"

# Run live tests
cyrius build --no-deps tests/test_live.tcyr build/test_live && ./build/test_live

# Cleanup
docker stop majra-redis majra-postgres && docker rm majra-redis majra-postgres
```
