# Dependency Watch

majra v2.4.x has **one external first-party dependency (sigil)** used
only by the richer distribution profiles; the core profile remains
stdlib-only.

## Profile / dep matrix

| Profile               | Cyrius stdlib | sigil | patra | Notes |
|-----------------------|:-------------:|:-----:|:-----:|-------|
| `majra`               | ✓             |       |       | Core engine; no crypto, no network |
| `majra-signed`        | ✓             | ✓     |       | Adds Ed25519-signed envelopes |
| `majra-admin`         | ✓             |       |       | Adds HTTP admin endpoint (uses `lib/http_server.cyr` from stdlib) |
| `majra-backends`      | ✓             | ✓     | ✓     | Everything: signed + admin + network backends + patra_queue |

## Cyrius stdlib modules used

| Module | Purpose | Profiles |
|--------|---------|----------|
| `string.cyr`     | C string operations (strlen, streq, memcpy, memset) | all |
| `fmt.cyr`        | Integer formatting (fmt_int, fmt_int_fd) | all |
| `alloc.cyr`      | Bump allocator (alloc, alloc_reset) | all |
| `freelist.cyr`   | Free-list allocator with individual free (fl_alloc, fl_free) | all |
| `vec.cyr`        | Dynamic i64 array (vec_new, vec_push, vec_get) | all |
| `str.cyr`        | Fat string type (str_from, str_len, str_eq, str_builder) | all |
| `hashmap.cyr`    | Hash table — `map_new()` for cstr keys, `map_new_str()` for Str-struct keys (5.4.14+) | all |
| `syscalls.cyr`   | Linux syscall wrappers (auto-dispatched x86_64/aarch64 in 5.4.11+) | all |
| `tagged.cyr`     | Option/Result tagged unions (Ok, Err, Some, None) | all |
| `fnptr.cyr`      | Function pointer dispatch (fncall0..fncall4) | all |
| `thread.cyr`     | Threads (clone), mutexes (futex), MPSC channels | all |
| `assert.cyr`     | Test assertions (assert, assert_eq, assert_summary) | tests only |
| `bench.cyr`      | Benchmarking (bench_new, bench_batch_start/stop, bench_report) | benches only |
| `net.cyr`        | TCP/UDP sockets | backends, admin |
| `io.cyr`         | File I/O, stdin/stdout | backends, admin, tests |
| `fs.cyr`         | File system ops | backends (patra_queue) |
| `bigint.cyr`     | u256 / field arithmetic | backends (via sigil) |
| `http_server.cyr` | HTTP/1.1 server primitives | admin, backends |
| `patra.cyr`      | SQL-backed storage (vendored patra ≥ 1.1.1) | backends (patra_queue) |
| `sakshi.cyr`     | Structured tracing (pulled transitively by patra) | backends |

## First-party deps

### sigil ≥ 2.9.0
- **Where**: `[deps.sigil]` in `cyrius.cyml`, vendored as `lib/sigil.cyr`
- **Used by**: `src/ipc_encrypted.cyr` (AES-256-GCM), `src/signed_envelope.cyr` (Ed25519)
- **Profiles that pull it**: `signed`, `backends`
- **Why pinned**: `aes_gcm_encrypt` / `aes_gcm_decrypt` surfaces shipped in sigil 2.8.4; HKDF in 2.9.0. AES-NI hardware acceleration staged in 2.9.0 but deferred to 2.10 (pending cyrius 5.5.x inline-asm fix). majra pins 2.9.0 as the minimum.

## Upgrade considerations

- **Cyrius compiler upgrades** — when `cyrius = "..."` in `cyrius.cyml` is bumped, recompile and re-run all four test suites + soak. Refresh any drifted `lib/*.cyr` from `~/.cyrius/lib/`.
- **Stdlib changes** — if vendored `lib/` modules are updated, verify no function signatures changed and run the full matrix. Track drift with a one-liner per the CLAUDE.md dev loop.
- **sigil upgrades** — sigil 2.10 will bundle X25519 key agreement (unblocks QUIC transport) and AES-NI dispatch wiring (transparent speedup). When sigil ships, bump `[deps.sigil]` tag, refresh `lib/sigil.cyr`, run the full matrix.
- **patra upgrades** — patra 1.1.1 ships with cyrius stdlib. A patra upgrade that changes result-row column ordering or SELECT semantics would affect `src/patra_queue.cyr`; regression-test via `tests/test_patra_queue.tcyr` before bumping.
