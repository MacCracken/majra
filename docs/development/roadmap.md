# Majra Roadmap

Completed items live in [CHANGELOG.md](../../CHANGELOG.md).

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
- **QUIC transport + AES-NI hardware acceleration** — gated on **sigil emitting cyrius-stable asm**. Bisect against cyrius 5.10.34 (2026-05-10): sigil 2.9.0 = full pass; 2.9.1–3.0.1 = SIGILL on the ed25519-NI path; 3.1.0 = SIGILL earlier on aes_gcm-NI too. All the regressions trace to inline-asm blocks that hardcode `[rbp-N]` parameter offsets matching cyrius's pre-5.5 stack frame, which has shifted under 5.10.x's expanded prologue. We pinned at sigil 2.9.0 (asm-free reference paths) during the 2.4.2 toolchain bump. Filed upstream as P1 in sigil's v3.1 work arc: [`sigil/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md`](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md). Once sigil ships the fix, bump the pin, regenerate the four dist bundles, and revisit QUIC (still needs X25519 from sigil too — same release).
  - **Note on the transitive agnosys/SYS_OPEN observation:** during 2.4.2 work we noticed `lib/agnosys.cyr:791` uses raw `syscall(SYS_OPEN, ...)` which blocks aarch64 cross-builds. The resolved agnosys (1.0.4) is what sigil 2.9.0's deps pin transitively. agnosys mainline (1.2.4) has zero `SYS_OPEN` refs in its dist bundle — the bug was fixed upstream long ago. Bumping past sigil 2.9.0 rolls agnosys forward transitively, so this resolves automatically when the asm-drift fix lands. Not a separate upstream filing.

### Engineering backlog

- **Shared-memory IPC transport** (mmap-based) — still deferred. Most workloads are fine with Unix-socket IPC; revisit when a consumer hits the syscall-per-message ceiling.

### Upstream cleanup (not majra work)
- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` is fixed in 5.4.10 but hasn't been moved to `issues/archived/` with a `— RESOLVED` suffix yet. Per the `issues/README.md` lifecycle, someone on the Cyrius side should archive it.

## Non-goals

- **Application-level business logic** — majra provides primitives, consumers define semantics.
- **Message broker replacement** — in-process library first; `redis_backend` covers cross-process.
- **LLVM/Cargo dependency** — Cyrius compiles directly to machine code.
