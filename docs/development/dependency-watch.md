# Dependency Watch

majra v2.4.x has **one external first-party dependency (sigil)** used
only by the richer distribution profiles; the core profile remains
stdlib-only.

## Profile / dep matrix

| Profile               | Cyrius stdlib | sigil | patra | Notes |
|-----------------------|:-------------:|:-----:|:-----:|-------|
| `majra`               | ✓             |       |       | Core engine; no crypto, no network |
| `majra-signed`        | ✓             | ✓     |       | Adds Ed25519-signed envelopes |
| `majra-admin`         | ✓             |       |       | Adds HTTP admin endpoint (uses `lib/sandhi.cyr` from stdlib — http_server surface folded into sandhi at the M6 stdlib fold-in) |
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
| `hashmap.cyr`    | Hash table — `map_new()` for cstr keys, `map_new_str()` for Str-struct keys | all |
| `syscalls.cyr`   | Linux syscall wrappers (auto-dispatched x86_64/aarch64 via per-arch peer files) | all |
| `tls.cyr`        | TLS primitives (transitive — `sandhi` references `TLS_EARLY_DATA_ACCEPTED` at parse time) | admin, backends |
| `tagged.cyr`     | Option/Result tagged unions (Ok, Err, Some, None) | all |
| `fnptr.cyr`      | Function pointer dispatch (fncall0..fncall4) | all |
| `thread.cyr`     | Threads (clone), mutexes (futex), MPSC channels | all |
| `assert.cyr`     | Test assertions (assert, assert_eq, assert_summary) | tests only |
| `bench.cyr`      | Benchmarking (bench_new, bench_batch_start/stop, bench_report) | benches only |
| `net.cyr`        | TCP/UDP sockets | backends, admin |
| `io.cyr`         | File I/O, stdin/stdout | backends, admin, tests |
| `fs.cyr`         | File system ops | backends (patra_queue) |
| `bigint.cyr`     | u256 / field arithmetic | backends (via sigil) |
| `sandhi.cyr`     | HTTP server primitives — `HTTP_*` codes, `http_send_status`, `http_server_run` (and the rest of sandhi's service-boundary surface; only the http server bits are actually called from majra) | admin, backends |
| `patra.cyr`      | SQL-backed storage (patra 1.9.3 via cyrius stdlib; full `WHERE` / `ORDER BY` / `LIMIT` / `COUNT(*)` / `MAX()` surface — `src/patra_queue.cyr` retired its 1.1.1-shaped client-side workarounds in 2.4.3) | backends (patra_queue) |
| `sakshi.cyr`     | Structured tracing (pulled transitively by patra) | backends |

## First-party deps

### sigil = 2.9.0
- **Where**: `[deps.sigil]` in `cyrius.cyml`, resolved into `lib/sigil.cyr` by `cyrius deps` from the pinned git tag.
- **Used by**: `src/ipc_encrypted.cyr` (AES-256-GCM), `src/signed_envelope.cyr` (Ed25519)
- **Profiles that pull it**: `signed`, `backends`
- **Why exactly 2.9.0, not 2.9.x+**: bisect during the 2.4.2 cyrius 5.10.34 toolchain bump (2026-05-10) — 2.9.0 = full pass; 2.9.1–3.0.1 = SIGILL on the ed25519-NI path; 3.1.0 = SIGILL earlier on the aes_gcm-NI path. The breakage traces to inline-asm blocks in the NI dispatch fns that hardcode `[rbp-N]` parameter offsets matching cyrius's pre-5.5 stack frame; 5.10.x's expanded prologue shifts the parameter slots so the asm loads garbage and the subsequent `aesenc` / `pmull` faults. 2.9.0 keeps the asm-free reference paths (no AES-NI / SHA-NI / ed25519-NI dispatch) so it survives the toolchain bump untouched. Tracked upstream at [`sigil/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md`](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md) as P1 in the sigil v3.1 work arc.
- **2026-05-11 status check (at majra 2.4.4 / cyrius 5.10.44 bump)**: sigil tagged 3.1.1 on 2026-05-11, but the changelog stanza is a "stdlib annotation pass" (mechanical `: i64` return-type sweep matching cyrius v5.11.x), not the NI-path fix. The P1 issue file is still present on `sigil/main`. No revisit of the sigil pin attempted at this bump; 2.9.0 continues to ride through, and `tests/test_backends.tcyr` (aes_gcm_roundtrip / signed_envelope / encrypted_ipc) passes under cyrius 5.10.44.

## Upgrade considerations

- **Cyrius compiler upgrades** — when `cyrius = "..."` in `cyrius.cyml` is bumped, run `cyrius deps` to repopulate `lib/`, then recompile and re-run all four test suites (core + expanded + backends + patra_queue) + the soak set. The 2.4.2 jump (5.4.17 → 5.10.34) is the worked example for a *minor*-spanning bump; the 2.4.4 stair-step (5.10.34 → 5.10.44, ten patch releases) is the worked example for an in-minor bump — `cyrius.lock` stays byte-identical (sigil/sakshi/agnosys git tags unchanged) and the four dist bundle bodies stay byte-identical (only the version banner moves), so the patch is essentially "prove the test matrix still passes."
- **Stdlib changes** — `lib/` is gitignored; `cyrius deps` rewrites it from the version-pinned snapshot on every invocation. The `cyrius.lock` file in-tree carries SHA-256 hashes for the git-resolved first-party deps (sigil, sakshi, agnosys); CI's `cyrius deps --verify` enforces match. Stdlib drift detection lives in the toolchain itself.
- **sigil upgrades** — gated on sigil shipping a NI-dispatch surface that survives a cyrius minor bump (see "Why exactly 2.9.0, not 2.9.x+" above). When that lands, bump `[deps.sigil]` tag, rerun `cyrius deps`, run the full matrix, regenerate all four dist bundles. The likely majra-side payoff: AES-NI / SHA-NI / ed25519-NI hardware acceleration (~360× block-encrypt, ~21–44× SHA compress per sigil's 2.9.x perf notes) for the `signed` + `backends` profiles. QUIC transport is a separate longer-horizon item; needs X25519 from sigil too.
- **patra upgrades** — patra is resolved transitively via the cyrius stdlib (1.9.3 across the cyrius 5.10.34 → 5.10.44 snapshot range; unchanged at the 2.4.4 bump). A patra upgrade that changes result-row column ordering, SELECT semantics, or aggregate-return shape would affect `src/patra_queue.cyr`; regression-test via `tests/test_patra_queue.tcyr` (17 assertions covering enqueue / dequeue priority order / status counts / persistence on reopen) before bumping the cyrius pin. The 2.4.3 migration to server-side `WHERE` + `ORDER BY` + `LIMIT` + `COUNT(*)` + `MAX()` exercises most of patra's SQL surface, so any parser regression should surface fast.
- **sandhi upgrades** — sandhi is folded into the cyrius stdlib (since the M6 fold-in). The HTTP server surface used by `src/admin.cyr` (`HTTP_*` constants, `http_send_status`, `http_server_run`) has been API-stable since the sandhi M1 lift from `lib/http_server.cyr`. Watch for renames at sandhi major bumps.
