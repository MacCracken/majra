# Majra Roadmap

Completed items live in [CHANGELOG.md](../../CHANGELOG.md).

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
- **QUIC transport + AES-NI hardware acceleration** — both are pending the **next sigil release** (2.10.0 or 2.9.1 — TBD), which will bundle two things together: (a) **X25519 key agreement** — the last TLS 1.3 primitive sigil needs to complete its crypto surface, which unblocks majra's QUIC transport module; (b) **AES-NI / pmull dispatch wiring** on top of sigil 2.9.0's staged scaffold. AES-NI is currently blocked by a cc5 inline-asm codegen bug (`cyrius/docs/development/issues/inline-asm-stores-silently-drop-when-fn-included.md`) scheduled for cyrius 5.5.x. Both items land together so consumers see one coherent crypto-surface bump rather than two.

### Engineering backlog

- **Shared-memory IPC transport** (mmap-based) — still deferred. Most workloads are fine with Unix-socket IPC; revisit when a consumer hits the syscall-per-message ceiling.
- **Multi-row patra WHERE** — `patra_queue` currently scans all rows and filters client-side because patra 1.1.1 returned null result sets for column-list SELECT + WHERE. Revisit when patra's SQL parser tolerates more query shapes, or switch to explicit column indices once we've characterized the parser's WHERE behavior.

### Upstream cleanup (not majra work)
- `cyrius/docs/development/issues/majra-cbarrier-arrive-and-wait-crash.md` is fixed in 5.4.10 but hasn't been moved to `issues/archived/` with a `— RESOLVED` suffix yet. Per the `issues/README.md` lifecycle, someone on the Cyrius side should archive it.

## Non-goals

- **Application-level business logic** — majra provides primitives, consumers define semantics.
- **Message broker replacement** — in-process library first; `redis_backend` covers cross-process.
- **LLVM/Cargo dependency** — Cyrius compiles directly to machine code.
