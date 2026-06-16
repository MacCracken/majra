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
| `tls.cyr`        | TLS primitives (transitive — `sandhi` references `TLS_BACKEND_LIBSSL` at parse time; must be included before `sandhi.cyr`) | admin, backends |
| `slice.cyr`      | First-class 16-byte slice helpers (`_slice_idx_get_W`) — required by agnosys 1.4.3's slice subscripts; provided by `cyrius lib sync` | all (transitively via syscalls→agnosys) |
| `ct.cyr`         | Constant-time compare (`ct_eq_bytes`, `ct_eq_bytes_lens`, `ct_select`) — sigil 3.x retired its bundled `ct_eq` and relies on this | signed, backends |
| `chrono.cyr`     | Clock / sleep (`clock_now_ms`, `clock_epoch_secs`, `sleep_ms`) — reached by sigil/sandhi | backends, admin |
| `async.cyr`      | Async runtime (`async_new`/`run`/`spawn`) — reached by sandhi server | backends, admin |
| `dynlib.cyr` / `fdlopen.cyr` | Dynamic loader (auxv-based `dlopen`/`dlsym`) — reached by tls's optional libssl backend | backends, admin |
| `tagged.cyr`     | Option/Result tagged unions (Ok, Err, Some, None) | all |
| `fnptr.cyr`      | Function pointer dispatch (fncall0..fncall4) | all |
| `thread.cyr`     | Threads (clone), mutexes (futex), MPSC channels | all |
| `assert.cyr`     | Test assertions (assert, assert_eq, assert_summary) | tests only |
| `bench.cyr`      | Benchmarking (bench_new, bench_batch_start/stop, bench_report) | benches only |
| `net.cyr`        | TCP/UDP sockets | backends, admin |
| `io.cyr`         | File I/O, stdin/stdout | backends, admin, tests |
| `fs.cyr`         | File system ops | backends (patra_queue) |
| `sandhi.cyr`     | HTTP server primitives — `HTTP_*` codes plus the `sandhi_server_*` server API (`sandhi_server_get_path`, `_send_status`, `_send_response`, `_path_only`, `_get_param`, `_run`). Renamed from the pre-6.x `http_*` namespace in the cyrius 6.x reorg. | admin, backends |
| `patra.cyr`      | SQL-backed storage (patra 1.9.3 via cyrius stdlib; full `WHERE` / `ORDER BY` / `LIMIT` / `COUNT(*)` / `MAX()` surface — `src/patra_queue.cyr` retired its 1.1.1-shaped client-side workarounds in 2.4.3) | backends (patra_queue) |
| `sakshi.cyr`     | Structured tracing (pulled transitively by patra) | backends |

## First-party deps

### sigil = 3.7.14 (latest)
- **Where**: `[deps.sigil]` in `cyrius.cyml`, resolved into `lib/sigil.cyr` by `cyrius deps` from the pinned git tag.
- **Used by**: `src/ipc_encrypted.cyr` (AES-256-GCM), `src/signed_envelope.cyr` (Ed25519)
- **Profiles that pull it**: `signed`, `backends`
- **History — why it was pinned at 2.9.0 for the 2.4.0–2.4.4 line**: bisect during the 2.4.2 cyrius 5.10.34 bump (2026-05-10) — 2.9.0 = full pass; 2.9.1–3.0.1 = SIGILL on the ed25519-NI path; 3.1.0 = SIGILL earlier on the aes_gcm-NI path. The breakage traced to inline-asm blocks in the NI dispatch fns that hardcoded `[rbp-N]` parameter offsets matching cyrius's pre-5.5 stack frame; 5.10.x's expanded prologue shifted the parameter slots so the asm loaded garbage and the subsequent `aesenc` / `pmull` faulted. 2.9.0 kept the asm-free reference paths, so it survived untouched.
- **Why latest is fine now (at majra 2.4.5 / cyrius 6.1.24, 2026-06-10)**: the cyrius 6.x toolchain dissolved the whole failure class. sigil moved its NI asm off the hardcoded `mov r__, [rbp-N]` parameter loads onto the **`param_load(reg, idx)` pseudo** (cyrius 6.0.67+), which the compiler resolves to the correct frame slot regardless of prologue shape. sigil 3.7.8's own changelog confirms the residual cyrius-6.1.20 "NI re-break" was actually a *different* mechanism — cyrius 6.1.x only **warns** on an undefined function and compiles the call to a runtime-trapping `ud2`, so a bundle/consumer with a missing symbol SIGILLs the moment that call executes (looks identical to an asm fault under gdb until you see the `ud2`). 3.7.8 resolves the symbol omissions. Under cyrius 6.1.24, `tests/test_backends.tcyr` (`aes_gcm_roundtrip` / `encrypted_ipc` / `signed_envelope`) is green.
- **Two majra-side adjustments the 3.x bump required** (see CHANGELOG 2.4.5):
  - sigil retired its bundled `ct_eq` at **3.0.2**; the dual-length constant-time compare now lives in the stdlib `lib/ct.cyr` as `ct_eq_bytes_lens`. `src/signed_envelope.cyr` was calling the old `ct_eq` → migrated. signed/backends consumers must include `lib/ct.cyr`.
  - The `ud2`-on-undefined behavior means every symbol sigil/sandhi reach into must be **present in the compilation unit** or it becomes a latent SIGILL. The test/fuzz entry points gained explicit includes (`ct`, `chrono`, `async`, `sakshi`, `dynlib`, `fdlopen`, `tls`) accordingly; `cyrius lib sync` makes them available in `./lib/`.
- **Transitive agnosys**: sigil 3.7.x pulls agnosys (was 1.0.4 under sigil 2.9.0; 1.3.2 across 3.7.8 → 3.7.10; **rolled to 1.4.3 at sigil 3.7.14**). Both have zero `SYS_OPEN` refs — the dormant aarch64 cross-build blocker stays resolved. agnosys also uses first-class slice subscripts, so its compilation unit needs `lib/slice.cyr` (provided by `cyrius lib sync`).
- **3.7.8 → 3.7.10 bump (majra 2.4.6 / cyrius 6.1.35, 2026-06-11)**: routine patch bump, no majra-side adjustment. sigil's `signed`/`backends` surface is unchanged — the four dist bundle bodies are byte-identical to 2.4.5. sigil bundles its own `u256_*` field arithmetic (24 fns in `bigint_ext`), so it has **no** dependency on the stdlib `lib/bigint.cyr` that cyrius 6.1.35 dropped (see the stdlib-modules note below).
- **3.7.10 → 3.7.14 bump (majra 2.4.7 / cyrius 6.2.11, 2026-06-15)**: routine patch bump alongside the cyrius 6.1.35 → 6.2.11 minor move, no majra-side adjustment. sigil's `signed`/`backends` surface is unchanged — the four dist bundle bodies stay byte-identical (only the version banner moves). Transitive agnosys rolled 1.3.2 → 1.4.3; `test_backends` (`aes_gcm_roundtrip` / `encrypted_ipc` / `signed_envelope`) stays green.

## Upgrade considerations

- **Cyrius compiler upgrades** — when `cyrius = "..."` in `cyrius.cyml` is bumped, run `cyrius lib sync && cyrius deps` to repopulate `lib/`, then recompile (`cyrius build --no-deps`) and re-run all four test suites (core + expanded + backends + patra_queue) + the soak set. The 2.4.5 jump (5.10.44 → 6.1.24) is the worked example for a *major*-spanning bump and is anything but byte-identical: the cyrius 6.x stdlib reorg renamed `http_*` → `sandhi_server_*`, split toolchain modules out (`slice`/`ct`/`chrono`/`async`/`dynlib`/`fdlopen`), and changed undefined-symbol handling to a runtime `ud2`. Budget real porting time and audit every entry point's reachable-undefined warnings — under cyrius 6.1.x a leftover undefined call is a latent SIGILL, not a build failure.
- **Stdlib changes** — `lib/` is gitignored; under cyrius 6.x `cyrius lib sync` copies the version-pinned snapshot into `./lib/` and `cyrius deps` overlays the git deps. The snapshot size tracks the toolchain: **94 files under 6.1.24, 88 under 6.1.35, 97 under 6.2.11** (the 6.2.x snapshot ships more `.cyr` files than 6.1.35 did). The `cyrius.lock` file in-tree carries SHA-256 hashes over all resolved files (97 at 2.4.7); CI's `cyrius deps --verify` enforces match. **Always build with `--no-deps`** so the build's auto-`deps` doesn't perturb the synced lib. **Watch for snapshot drops on a cyrius bump**: a module listed in `[deps] stdlib` that the new snapshot no longer ships makes `cyrius deps` error with `cannot read ./lib/<mod>.cyr` — if majra has no live call site for it (grep `src/ tests/`), drop it from the `[deps] stdlib` list and any stale `include`. `bigint` was retired this way at 2.4.6.
- **sigil upgrades** — now tracking **latest (3.7.14)**; the cyrius-5.10.x asm-offset constraint that pinned it at 2.9.0 is gone under cyrius 6.x. On a future bump, rerun the full matrix and watch `test_backends` for any new `ud2`-SIGILL (a missing symbol sigil newly reaches into → add the providing `lib/<mod>.cyr` include). The majra-side payoff already banked: AES-NI / SHA-NI / ed25519-NI hardware acceleration for the `signed` + `backends` profiles. QUIC transport is a separate longer-horizon item; needs X25519 from sigil too.
- **patra upgrades** — patra is resolved transitively via the cyrius stdlib snapshot (provisioned by `cyrius lib sync`). A patra upgrade that changes result-row column ordering, SELECT semantics, or aggregate-return shape would affect `src/patra_queue.cyr`; regression-test via `tests/test_patra_queue.tcyr` (17 assertions covering enqueue / dequeue priority order / status counts / persistence on reopen) before bumping the cyrius pin. The 2.4.3 migration to server-side `WHERE` + `ORDER BY` + `LIMIT` + `COUNT(*)` + `MAX()` exercises most of patra's SQL surface, so any parser regression should surface fast.
- **sandhi upgrades** — sandhi is folded into the cyrius stdlib (since the M6 fold-in). At cyrius 6.x its HTTP-server surface was renamed from the `http_*` namespace to `sandhi_server_*` (`src/admin.cyr` was ported at 2.4.5); the `HTTP_*` status constants stayed put. It also now references `TLS_BACKEND_LIBSSL` at parse time, so `lib/tls.cyr` must be included *before* `lib/sandhi.cyr`. Watch for further renames at sandhi major bumps.
