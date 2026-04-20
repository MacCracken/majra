# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [2.4.0-dev] - unreleased

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
