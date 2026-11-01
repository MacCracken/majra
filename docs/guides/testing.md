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

# The four above, then the benches, in one go (exits non-zero if any suite fails)
sh tests/test.sh

# Live integration tests (requires Redis on :6379, PostgreSQL on :5432)
cyrius build --no-deps tests/test_live.tcyr build/test_live && ./build/test_live

# Soak tests (on-demand, not in CI)
cyrius build --no-deps tests/soak/soak_queue.cyr build/soak_queue && ./build/soak_queue
```

## Test Suites

| Suite | File | Assertions | Coverage |
|-------|------|-----------|----------|
| Core | `src/main.cyr` | 205 | All 15 core modules + relay dedup, envelope-id entropy (non-zero *and* distinct), `_majra_sleep_ns` floor |
| Expanded | `tests/test_core.tcyr` | 736 | Deep: queue lifecycle, pubsub patterns, DAG retry (outcome + elapsed backoff), fleet routing, circuit breaker, integration, multi-threaded barrier (proves blocking, not just a counter), concurrency + wildcard-alignment, relay / ratelimit / priority-queue regressions. Per-release additions: CHANGELOG.md |
| Backends | `tests/test_backends.tcyr` | 605 | base64, SHA-1, AES-256-GCM, signed envelopes, admin endpoint, WebSocket, RESP, PG wire, SCRAM-SHA-256 (RFC 7677 vector, a fake SCRAM server) and PG over TLS (fixture CA, `tests/fixtures/`) |
| Patra queue | `tests/test_patra_queue.tcyr` | 101 | Durable enqueue / priority dequeue / complete / counts / reopen persistence, pre-2.6.9 STR files, a corrupt TEXT ref failed so the job behind it is delivered, open/close bump growth, one handle shared by three threads (watchdog-guarded), three writers on three files during opens of a fourth (refused vs lost told apart) |
| Live | `tests/test_live.tcyr` | 47 | 7 Redis + 6 PostgreSQL test functions (SCRAM and TLS since 2.13.0) in `tests/test_live.tcyr`. **CI-only** — needs Redis on :6379 + PostgreSQL on :5432, so a dev-box "full matrix" run is 1,647, not 1,694 |
| **Total** | | **1,694** (1,647 CI + 47 live) | |

Every non-live suite, fuzz harness, soak, example and the benchmark binary also passes cross-built for aarch64 under `qemu-aarch64` (since 2.7.3; re-verified at 2.9.3 under 6.6.20, at 2.10.0 under 6.7.9, at 2.10.1, 2.11.0 and 2.12.0, and at 2.12.1 under 6.7.18 — the expanded suite reports three fewer there (724 at 2.12.1), its one x86-only test carrying three asserts). Since 2.12.2 CI runs the four suites (25× each), the fuzz harnesses and the examples there on every push; soaks and the benchmark binary stay by hand — see [aarch64 cross-build](#aarch64-cross-build-qemu-user) below.

`test_patra_queue` was split out at 2.4.0 to stay under the then-16384 cc5 fixup cap (patra pulls sakshi + io + fs transitively); the table starts at 1,048,576 entries and grows on demand at the 6.7.18 pin (growable since cyrius 6.2.0), and the split is kept as documented architecture.

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

20 benchmarks covering: envelope creation, priority queue, pattern matching (4 variants), pubsub publish, direct channel, heartbeat, fleet stats, rate limiting, relay send, barrier cycle, circuit breaker, counter increment, managed-queue lifecycle and 4-producer enqueue (the last two since 2.8.2), and one `ratelimit_evict_to` pressure sweep — 256 fresh keys trimmed back to a 4,096 cap, timed per sweep (since 2.12.0).

## Soak tests

`tests/soak/` holds on-demand stress tests. Not in CI; run before releases or when a code change could plausibly affect a primary state machine. Each soak file is standalone: `fn main()`, returns 0 on pass, non-zero on invariant violation. See `tests/soak/README.md` for conventions and known limitations (bump-allocator-bounded iteration counts).

Currently shipped (all four are run before a release, not in CI):
- `soak_queue.cyr` — 5k ops (1k rounds × 5 jobs) managed-queue lifecycle; asserts `mq_total_completed`, `mq_job_count`, and per-round `queued_count`/`running_count` invariants.
- `soak_pubsub.cyr` — 2000 distinct topics through `pubsub_new` + topic-map growth + subscribe + publish dispatch.
- `soak_relay.cyr` — relay dedup correctness + eviction under the `max_dedup` cap; 20 senders × 50 msgs × 2 passes, then a 200-sender eviction phase.
- `soak_heartbeat.cyr` — register / heartbeat / deregister cycles + auto-eviction; 100 nodes × 20 heartbeats, then an offline-timeout phase.

## aarch64 cross-build (qemu-user)

CI cross-builds the four suites for aarch64 and agnos (a gate that fails on syscall / symbol diagnostics) and, since 2.12.2, **runs** the aarch64 builds under `qemu-user` (a pinned 11.1.3 since 2.12.3, below): the `aarch64-qemu` job loops each suite 25×, then runs the fuzz harnesses and examples once. The soaks and the benchmark binary are not in it, so run the recipe below before a release and after touching anything that issues a syscall. cyrius's aarch64 backend renumbers 82 source numbers at the 6.7.18 pin, as at 6.7.9 and 6.6.20 (62 x86_64 numbers plus twenty ≥1000 private aliases; 68 at 6.6.9, 60 at 6.6.6, 44 at 6.6.4 — 6.6.5 routed `nanosleep` 35 and `sendto` 44 among fourteen more; per-tag detail in cyrius-quirks.md #9) and passes every other number through **verbatim**, so a stray x86_64 number is not a build error — it is a different, valid syscall. Through 2.7.2 majra shipped three of them (fchmod 91 → capset, getrandom 318 → ENOSYS, nanosleep 35 → unlinkat) and two built without a warning on any toolchain (CHANGELOG.md § [2.7.3]), which is why a build-only `--aarch64` step proves little here (it catches a `var SYS_*` that redefines a stdlib name, like the 318 site, but not a raw literal or a uniquely named constant, like 35 and 91): **the binaries have to run.**

`qemu-aarch64` comes from the distro's `qemu-user` package (Arch: `qemu-user`, installs `/usr/bin/qemu-aarch64`). **It must be qemu 11.1.0 or later, or 11.0.4 or later.** Older releases, Ubuntu 24.04's 8.2.2 among them, race the buffer check qemu makes on every syscall against another thread's `munmap`. They fail a valid `read` or `write` with `EFAULT` (qemu `e03b7dac65`). The patra-queue stall test hits that about once in 1,500 runs, as one refused enqueue ("every enqueue on … returned an id"). CI therefore installs Debian's static 11.1.3, hash-pinned, rather than the runner's package (CHANGELOG § [2.12.3]). `qemu-aarch64 -strace` prints each syscall with its result: the way to tell an emulator `EFAULT` from a real failure. The same `lib/` serves both arches — `--aarch64` selects the peer stdlib and the emitter — so no second `lib sync`.

```bash
# Every entry point CI runs, plus the four soaks, cross-built and run under qemu-user. The live suite
# cross-builds too but needs Redis + PostgreSQL to run (see Live Test Setup).
# Shape: cyrius build --aarch64 --no-deps <entry> build/<name>_aarch64 && qemu-aarch64 ./build/<name>_aarch64
cyrius build --aarch64 --no-deps src/main.cyr                build/majra_aarch64            && qemu-aarch64 ./build/majra_aarch64
cyrius build --aarch64 --no-deps tests/test_core.tcyr        build/test_core_aarch64        && qemu-aarch64 ./build/test_core_aarch64
cyrius build --aarch64 --no-deps tests/test_backends.tcyr    build/test_backends_aarch64    && qemu-aarch64 ./build/test_backends_aarch64
cyrius build --aarch64 --no-deps tests/test_patra_queue.tcyr build/test_patra_queue_aarch64 && qemu-aarch64 ./build/test_patra_queue_aarch64
cyrius build --aarch64 --no-deps benches/bench_all.bcyr      build/bench_all_aarch64        && qemu-aarch64 ./build/bench_all_aarch64   # all 20 targets print; the numbers are TCG's, not the CPU's

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

CI's 25 runs per suite are a per-push belt, not this loop: they catch a 7 % crash 84 % of the time (1 − 0.93²⁵) and a 1-in-100 one only 22 % of the time. Loop 100× by hand after touching threads, hashmaps or timing.

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

## agnos cross-build

All four suites cross-build with `--agnos` (CI's cross-build gate), `backends` and `patra-queue` since 2.12.2: they call `lib/io.cyr`'s `xunlink` / `xstat`, a local `_tb_lstat`, and the peers' `sys_socketpair` / `sys_fcntl` / `sys_recvfrom` / `sys_getsockname` / `sys_exit_group` wrappers, where they had Linux arities and `SYS_*` names the agnos peer does not declare. Nothing here runs agnos binaries, so the column proves linking only — and the IPC and socketpair tests would fail there by design (`ipc_bind` fails closed; the peer's `sys_socketpair` returns -38).

## Compiler Limitations

Cyrius's fixup table at the 6.7.18 pin starts at 1,048,576 forward references and grows ×2 on demand (8192 on cc3, 16384 across the cc5 5.x line; growable since 6.2.0). Large test entry points that aggregate many src modules are no longer near it — `test_patra_queue` was split out under the 16384 cap and the split is kept as documented architecture — but the advice stands: if adding significant new test code causes a "fixup table full" error, create a new `.tcyr` entry point scoped to what you're testing.

## Live Test Setup

```bash
# Start Redis and PostgreSQL
docker run -d --name majra-redis -p 6379:6379 redis:7-alpine
docker run -d --name majra-postgres -p 5432:5432 \
  -e POSTGRES_PASSWORD=majra -e POSTGRES_DB=majra postgres:16-alpine

# Configure PostgreSQL as CI's live job does: scram-sha-256 for every TCP login,
# ssl = on with the test-only fixture RSA leaf (tests/fixtures/README.md).
D=/var/lib/postgresql/data
docker cp tests/fixtures/pg_tls_rsa_server.pem majra-postgres:$D/server.crt
docker cp tests/fixtures/pg_tls_rsa_server.key majra-postgres:$D/server.key
docker exec majra-postgres sh -c "chown postgres:postgres $D/server.crt $D/server.key && chmod 600 $D/server.key"
docker exec majra-postgres su postgres -c "printf 'hostssl all all 0.0.0.0/0 scram-sha-256\nhost all all 0.0.0.0/0 scram-sha-256\nlocal all all trust\n' > $D/pg_hba.conf"
docker exec majra-postgres su postgres -c "psql -qc 'ALTER SYSTEM SET ssl = on' && pg_ctl reload -D $D"
# Or run CI's own step: it finds the container by image, so extract
# jobs.live-tests "Configure PostgreSQL (scram-sha-256 + TLS)" from ci.yml.

# Run live tests
cyrius build --no-deps tests/test_live.tcyr build/test_live && ./build/test_live

# Cleanup
docker stop majra-redis majra-postgres && docker rm majra-redis majra-postgres
```
