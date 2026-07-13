# Majra Roadmap

Completed items live in [CHANGELOG.md](../../CHANGELOG.md).

## Recently shipped (2.5.1)

- **Cyrius toolchain pin 6.3.15 → 6.4.62** + **sigil 3.9.8 → 3.11.1** (latest). No source-logic change; the four dist bundle bodies stay byte-identical (only the version banner + re-subsetted `.deps` sidecars move). Full matrix re-ran clean: 305/305 CI + 3/3 fuzz + 4/4 soak.
- **`cyrius lib sync --full` is now load-bearing.** cyrius 6.4.x made bare `lib sync` copy only the declared `[deps].stdlib` subset (40 files); `--full` provisions the whole 99-file snapshot the sigil/sandhi surface needs. CI + release + CLAUDE.md quick-start already/now use `--full`. `cyrius.lock` carries 99 hashes (was 97).
- **agnosys dropped from the dependency graph** — sigil 3.8.1 internalized its whole trust stack (agnosys → agnodrm), so sigil 3.11.x resolves with no external agnosys dep.
- **Sigil-footprint review.** majra's entire sigil surface is 6 symbols (`ed25519_*` + `aes_gcm_*`); `core`/`admin` pull none. Evaluated sigil 3.11.0's per-primitive `[lib.<type>]` profiles: kept the full `dist/sigil.cyr` (majra's combined-primitive test needs both, and the narrow ed25519+aes closures overlap on 121 fns), and documented the profiles as the recommendation for single-primitive consumers. Banked sigil 3.9.9's crypto-bank thread-local-slot fix (matters for the sigil+patra `backends` profile). See [`dependency-watch.md`](dependency-watch.md).
- **Fixed latent undersized `var X[N]` buffers in the test/soak harnesses** — the 2.5.0 audit fixed `src/` but missed the non-CI files. `soak_heartbeat` phase B was silently failing (`var ts[2]` holding a 16-byte timespec); resized `ts`/`key`/`nonce`/`buf` in `soak_heartbeat.cyr`, `test_core.tcyr`, `test_backends.tcyr`. Soak now 4/4 (was 3/4).

## Recently shipped (2.5.0)

- **agnos-target support for the core pub/sub engine** — `barrier`/`queue`/`envelope`/`dag`/`ipc` gained `#ifdef CYRIUS_TARGET_AGNOS` guards (futex→sched_yield, clock→uptime_ms, getrandom→#45, nanosleep→sleep_ms, AF_UNIX IPC fail-closes). Core `dist/majra.cyr` is agnos-clean. Cyrius pin 6.2.11 → 6.3.15.
- **Fixed undersized array-local buffer overflows in `src/`** (host crash under cyrius ≥ 6.3.13, which moved `var X[N]` locals onto the guarded thread stack) — `envelope.cyr`/`dag.cyr`/`main.cyr` timespecs, `ipc.cyr`/`ws.cyr`/`postgres_backend.cyr` frame headers. *(The 2.5.1 fix above closes the sibling test/soak-file instances this pass missed.)*

## Recently shipped (2.4.7)

- **Cyrius toolchain pin 6.1.35 → 6.2.11** (first move onto the 6.2.x line) + routine **sigil 3.7.10 → 3.7.14** (latest). No source-logic change; the four dist bundle bodies stay byte-identical (only the version banner moves). Transitive **agnosys 1.3.2 → 1.4.3** (via sigil). The 6.2.x stdlib snapshot grew the lib-sync set 88 → 97 files; `cyrius.lock` now carries 97 hashes (was 88). Full matrix re-ran clean: 305/305 CI + 3/3 fuzz + 4/4 soak under the new pin.

## Recently shipped (2.4.6)

- **Cyrius toolchain pin 6.1.24 → 6.1.35** + routine **sigil 3.7.8 → 3.7.10** (latest). No source-logic change; the four dist bundle bodies stay byte-identical (only the version banner moves). agnosys holds at 1.3.2 (transitive via sigil). Full matrix re-ran clean: 305/305 CI + 3/3 fuzz + 4/4 soak under the new pin.
- **`bigint` dropped from the stdlib surface** — cyrius 6.1.35's stdlib snapshot retired `lib/bigint.cyr` (94 → 88 files). majra never called it (sigil 3.x bundles its own `u256_*` field arithmetic), so the stale `include` in `tests/test_backends.tcyr` and the `[deps] stdlib` hint entry were removed. `cyrius.lock` now carries 88 hashes (was 94).

## Recently shipped (2.4.5)

- **Cyrius 6.x migration** — toolchain pin 5.10.44 → 6.1.24. The major bump cleared the long-standing sigil crypto-NI blocker: **sigil 2.9.0 → 3.7.8 (latest)**, the first sigil bump since 2.4.0. Under cyrius 6.x the `[rbp-N]` asm-offset SIGILL is gone (sigil's NI asm moved onto the `param_load` pseudo), and transitive **agnosys → 1.3.2** (zero `SYS_OPEN`) resolves the dormant aarch64 cross-build blocker too.
- **New cyrius-6 build workflow** — `cyrius lib sync` (stdlib snapshot) precedes `cyrius deps` (git deps); builds pass `--no-deps`. `cyrius.lock` grew 3 → 94 hashes. CI + release workflows updated. A bare `cyrius deps` now leaves a partial `./lib/` that ud2-SIGILLs at runtime (cyrius 6.1.x warns-then-`ud2` on undefined symbols).
- **Stdlib-reorg ports** — `src/admin.cyr` migrated `http_*` → `sandhi_server_*`; `src/signed_envelope.cyr` migrated `ct_eq` → `ct_eq_bytes_lens` (stdlib `lib/ct.cyr`). Test/fuzz entry points gained the explicit ct/chrono/async/sakshi/dynlib/fdlopen/tls/thread/metrics includes the cyrius-6 split now requires. 305/305 + fuzz + soak clean.

## Recently shipped (2.4.4)

- **Cyrius toolchain pin bumped 5.10.34 → 5.10.44** — ten patch-level cyrius releases. No source change; `cyrius.lock` byte-identical (sigil/sakshi/agnosys git tags unchanged); dist bundle bodies byte-identical (only the version banner moves). Full matrix re-ran clean: 305/305 CI assertions + 3/3 fuzz harnesses + 4/4 soak suites. Sigil held at 2.9.0 — [upstream P1](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md) still open at sigil 3.1.1 (the 5/11 sigil patch was the stdlib annotation pass, not the NI-path fix).

## Recently shipped (2.4.3)

- **`patra_queue` retires the patra-1.1.1 workarounds.** Migrated dequeue / count / max-id paths to server-side `WHERE` + `ORDER BY` + `LIMIT` + `COUNT(*)`/`MAX()` using patra 1.9.3's SQL surface. Drops the O(n) client-side scans that the original implementation had to do. Tests still 17/17.
- **`tests/test_patra_queue.tcyr` `sys_unlink()` cleanup** — same arch-portability fix as `src/ipc.cyr` got in 2.4.2.

## Recently shipped (2.4.2)

- **Cyrius toolchain pin bumped 5.4.17 → 5.10.34** + sandhi-from-stdlib for the HTTP server surface (no more vendored `lib/http_server.cyr`).
- **`lib/` gitignored** — repopulated by `cyrius deps`. Matches agnosys/agnostik/yukti/patra convention. `cyrius.lock` committed for hash verification.
- **`src/ipc.cyr` ported to `sys_unlink()`** (was raw `syscall(SYS_UNLINK, ...)`) so majra's own code stays cross-arch portable even though we don't currently ship an aarch64 binary.
- **CI installer fetches the stdlib via the source archive at the version tag** — 5.10.x release tarballs ship `bin/` + `deps/` only, no `lib/`.

## Upstream wins landed in 2.4.0

- **Hashmap Str-key fix (cyrius 5.4.14)** — the ~3% collision bug majra filed upstream during soak-test development. Fixed with a new `map_new_str()` + content-derived `hash_str_v`. `src/queue.cyr` flipped to `map_new_str()`; soak test's `mq_job_count` invariant is now authoritative.
- **Sigil 2.9.0 with HKDF + AES-NI scaffold** — HKDF live, AES-NI wiring deferred to 5.5.x per the inline-asm include-bug we surfaced during sigil 2.9.0 development.

## Recently shipped (2.4.0)

- **Soak-test infrastructure** (`tests/soak/`) — `soak_queue` ships; README and extension conventions in place. Surfaced + filed an upstream cyrius hashmap bug as a byproduct.
- **Sigil-signed envelopes** (`src/signed_envelope.cyr`) — Ed25519 signatures over canonical envelope encoding. Available via the new `[lib.signed]` profile.
- **HTTP admin/metrics endpoint** (`src/admin.cyr`) — `/health` + `/fleet` + `/ratelimit` via `lib/http_server.cyr`. Available via the new `[lib.admin]` profile.
- **Patra-backed persistent queues** (`src/patra_queue.cyr`) — durable alternative to in-memory managed queue, survives restart.

## Recently shipped (2.3.x)

- **AES-256-GCM wire encryption** for `ipc_encrypted` — via sigil 2.8.4. Real NIST-vector crypto, not the old plaintext stub. Constant-time tag verify; key zeroization on close. (2.3.1)
- **Multi-threaded `cbarrier_arrive_and_wait`** — revived under Cyrius 5.4.10+ after the RBP/child-stack race in `lib/thread.cyr`'s clone trampoline was fixed upstream. Filed by majra 2.3.0, fixed 5.4.10. (2.3.1)
- **Relay dedup assertions** — the cc3-era `map_get`-after-`map_set` bug that forced six commented-out dedup checks in `test_relay` is resolved in cc5. Assertions back on. (2.3.0)
- **Distribution bundles** — `dist/majra.cyr` (core, ~3k lines) and `dist/majra-backends.cyr` (+redis/pg/ws/encrypted-ipc, ~4.2k lines) via `cyrius distlib`. Consumers (daimon, AgnosAI, hoosh, sutra, stiva) pin per-profile. (2.3.0)
- **Manifest migration** — `cyrius.toml` → `cyrius.cyml` with `[package]/[build]/[lib]/[lib.backends]/[deps]` and `version = "${file:VERSION}"` single-source-of-truth. Toolchain pin jumped 3.2.6 → 5.4.12-1 (14+ minors of cc5 improvements). (2.3.0)

## Open Items

### Waiting on upstream
- _(empty)_ — the two long-standing upstream blockers both cleared at 2.4.5:
  - ~~**sigil asm-drift SIGILL**~~ — **RESOLVED** by the cyrius 6.x toolchain. sigil's NI asm moved onto the `param_load` pseudo (cyrius 6.0.67+), dissolving the `[rbp-N]` offset fragility. sigil now tracks latest (3.7.8) and AES-NI / SHA-NI / ed25519-NI acceleration is live for the `signed` + `backends` profiles.
  - ~~**agnosys SYS_OPEN / aarch64**~~ — **RESOLVED** transitively: sigil 3.7.8 rolls agnosys to 1.3.2, which has zero `SYS_OPEN` refs.

### Engineering backlog

- **QUIC transport** — now unblocked on the sigil side (X25519 available in 3.7.8); a real engineering item rather than waiting-on-upstream. Scope when a consumer needs it.
- **aarch64 cross-build wiring** — the SYS_OPEN blocker is gone (agnosys 1.3.2); wiring + verifying the `cyrius build --aarch64` CI step is now a discrete task. majra consumers are all x86_64-server-side, so low priority.
- **Shared-memory IPC transport** (mmap-based) — still deferred. Most workloads are fine with Unix-socket IPC; revisit when a consumer hits the syscall-per-message ceiling.

### Upstream cleanup (not majra work)
- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` is fixed in 5.4.10 but hasn't been moved to `issues/archived/` with a `— RESOLVED` suffix yet. Per the `issues/README.md` lifecycle, someone on the Cyrius side should archive it.

## Non-goals

- **Application-level business logic** — majra provides primitives, consumers define semantics.
- **Message broker replacement** — in-process library first; `redis_backend` covers cross-process.
- **LLVM/Cargo dependency** — Cyrius compiles directly to machine code.
