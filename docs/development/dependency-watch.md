# Dependency Watch

majra v2.6.8 has **zero declared git dependencies.** Everything it consumes —
sigil included — arrives in the version-pinned stdlib snapshot that
`cyrius lib sync --full` copies into `./lib/`, so the cyrius pin in
`cyrius.cyml [package].cyrius` is the single knob that moves any dep version.

This is the end of a three-release arc. sakshi moved off `[deps.sakshi]` at the
6.5.18 pin; sigil moved off `[deps.sigil]` at **2.6.8**. Both moves are the same
rule, stated once here:

> ⚠ **Never declare a `[deps.<name>]` git dep for a module the toolchain
> snapshot already folds in.** Two things go wrong, and the second is the one
> that bites. (1) `cyrius deps` overlays the git dep's copy *on top of* the
> snapshot, so a pin even slightly behind silently **downgrades** the module for
> every transitive consumer. (2) On a library that publishes bundles, `distlib`
> reclassifies the module **out of the stdlib leaves** and drops it from the
> generated `.deps` sidecars — so the published bundle no longer declares a
> dependency it genuinely has. kavach hit (2) via patra; majra hit (2) via
> sigil, and shipped it from 2.4.x through 2.6.7.

(agnosys is no longer in the graph — sigil 3.8.1 internalized its trust stack.)

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
| `sakshi.cyr`     | Structured tracing (reached transitively by patra + sigil). Folded stdlib module — majra's `[deps.sakshi]` block was retired at the 6.5.18 pin | backends |
| `sigil.cyr`      | Crypto — `ed25519_{init,sign,verify}` + `aes_gcm_{global_init,encrypt,decrypt}`. Folded stdlib module — majra's `[deps.sigil]` block was retired at 2.6.8 (6.5.35 pin) | signed, backends |

## First-party deps

### sigil = 3.12.9 — folded stdlib module since 2.6.8 (was a git dep)
- **Where**: declared in `[deps].stdlib` in `cyrius.cyml`; `lib/sigil.cyr` comes from the `cyrius lib sync --full` snapshot and tracks the toolchain pin. The 6.5.35 snapshot folds **3.12.9**, which is also the latest published sigil tag.
- **Used by**: exactly six symbols — `ed25519_{init,sign,verify}` (`src/signed_envelope.cyr`) + `aes_gcm_{global_init,encrypt,decrypt}` (`src/ipc_encrypted.cyr`). The constant-time pk compare is stdlib `ct_eq_bytes_lens`, **not** sigil.
- **Profiles that pull it**: `signed` (Ed25519 only), `backends` (Ed25519 + AES-GCM). `core` and `admin` pull **no** sigil at build time — though note `dist/majra.deps` still *names* sigil, because the default profile's sidecar mirrors the `[deps].stdlib` hint list verbatim rather than a computed leaf set.

#### Why it stopped being a git dep (2.6.8)
2.6.7 swept the dependency closure for folded modules whose `[deps.X]` pin
lagged the toolchain, and corrected sigil 3.12.7 → 3.12.9. That closed hazard
(1) above. Hazard (2) went unnoticed, and it was the live one:
`dist/majra-signed.deps` and `dist/majra-backends.deps` — the sidecars for the
two profiles that exist *because* they carry crypto — did not name `sigil`.

Reproduced against the shipped 2.6.7 sidecar by building a consumer that
provisions exactly what it declares:

```
warning: undefined function 'ed25519_init'
warning: undefined function 'ed25519_sign'
warning: undefined function 'ed25519_verify'
warning: undefined function 'ct_eq_bytes_lens'
OK
```

The `OK` is the whole problem. Per [`cyrius-quirks.md § undefined functions`](cyrius-quirks.md),
an undefined fn lowers to a trapping `ud2` — so a consumer's build passes and
the process **SIGILLs (exit 132) the first time it signs an envelope**. Moving
sigil into `[deps].stdlib` puts it back in both sidecars; the same clean-room
build then resolves all four symbols.

**Cost of the move**: sigil can no longer be pinned independently of the
toolchain. That was judged the cheaper side — the independent pin existed to
work around the cyrius-5.10.x asm-offset SIGILL, which has been gone since
2.4.5, whereas the sidecar is what downstream consumers actually resolve
against.

#### Sigil-footprint review (2.5.1 — do we still need the full bundle?)
sigil 3.11.0 shipped twelve per-primitive `[lib.<type>]` distlib profiles
(`dist/sigil-ed25519.cyr`, `dist/sigil-aes.cyr`, …) so a consumer can pull one
primitive's self-contained closure instead of the full 61-module,
**25,391-line** `dist/sigil.cyr`. majra evaluated switching:
- **Kept the full `dist/sigil.cyr`.** majra's only local sigil consumer,
  `tests/test_backends.tcyr`, exercises *both* Ed25519 and AES-GCM. The two
  narrow closures (~2k lines each) **overlap on 121 functions** — Ed25519 uses
  SHA-512 internally, and both re-bundle sigil's `u256_*` field arithmetic +
  `crypto_scratch` + `random` floor. Including both emits 121
  "last-definition-wins" duplicate-fn warnings (verified: the shared fns are
  byte-identical, so it's *correct* but noisy + brittle), whereas the full
  bundle is a single deduplicated closure that resolves with zero sigil-side
  warnings. sigil publishes no `dist/sigil/index.cyml`, so the clean
  `[deps.sigil] modular = ["ed25519","aes_gcm"]` dedup path (cyrius 6.2.50) is
  **not** available.
- **Consumer guidance**: the per-primitive win is real for a **single**-
  primitive downstream. A `signed`-only consumer (e.g. secureyeoman) should
  pull `dist/sigil-ed25519.cyr` (~2k lines, `.deps` = 10 leaves) rather than
  the full bundle (23 leaves). A `backends` consumer needs both, so the full
  bundle stays simplest there too.
- **Crypto-bank slot fix banked (sigil 3.9.9)**: `_SIGIL_CBANK_SLOT` moved off
  cyrius thread-local slot 0 → 8. Slot 0 is also owned by **patra**, so a
  process linking *both* — precisely the `backends` profile (sigil crypto +
  `patra_queue`) — could corrupt sigil's crypto bank on a patra query. The
  3.9.8 → 3.11.1 bump makes `backends` safe on that axis.
- **History — why it was pinned at 2.9.0 for the 2.4.0–2.4.4 line**: bisect during the 2.4.2 cyrius 5.10.34 bump (2026-05-10) — 2.9.0 = full pass; 2.9.1–3.0.1 = SIGILL on the ed25519-NI path; 3.1.0 = SIGILL earlier on the aes_gcm-NI path. The breakage traced to inline-asm blocks in the NI dispatch fns that hardcoded `[rbp-N]` parameter offsets matching cyrius's pre-5.5 stack frame; 5.10.x's expanded prologue shifted the parameter slots so the asm loaded garbage and the subsequent `aesenc` / `pmull` faulted. 2.9.0 kept the asm-free reference paths, so it survived untouched.
- **Why latest is fine now (at majra 2.4.5 / cyrius 6.1.24, 2026-06-10)**: the cyrius 6.x toolchain dissolved the whole failure class. sigil moved its NI asm off the hardcoded `mov r__, [rbp-N]` parameter loads onto the **`param_load(reg, idx)` pseudo** (cyrius 6.0.67+), which the compiler resolves to the correct frame slot regardless of prologue shape. sigil 3.7.8's own changelog confirms the residual cyrius-6.1.20 "NI re-break" was actually a *different* mechanism — cyrius 6.1.x only **warns** on an undefined function and compiles the call to a runtime-trapping `ud2`, so a bundle/consumer with a missing symbol SIGILLs the moment that call executes (looks identical to an asm fault under gdb until you see the `ud2`). 3.7.8 resolves the symbol omissions. Under cyrius 6.1.24, `tests/test_backends.tcyr` (`aes_gcm_roundtrip` / `encrypted_ipc` / `signed_envelope`) is green.
- **Two majra-side adjustments the 3.x bump required** (see CHANGELOG 2.4.5):
  - sigil retired its bundled `ct_eq` at **3.0.2**; the dual-length constant-time compare now lives in the stdlib `lib/ct.cyr` as `ct_eq_bytes_lens`. `src/signed_envelope.cyr` was calling the old `ct_eq` → migrated. signed/backends consumers must include `lib/ct.cyr`.
  - The `ud2`-on-undefined behavior means every symbol sigil/sandhi reach into must be **present in the compilation unit** or it becomes a latent SIGILL. The test/fuzz entry points gained explicit includes (`ct`, `chrono`, `async`, `sakshi`, `dynlib`, `fdlopen`, `tls`) accordingly; `cyrius lib sync` makes them available in `./lib/`.
- **Transitive agnosys — gone as of sigil 3.8.1.** sigil 3.7.x pulled agnosys (1.0.4 under 2.9.0 → 1.4.3 at 3.7.14); sigil **3.8.1** internalized the whole trust stack (the agnosys → agnodrm decomposition), so sigil 3.11.x resolves with **no** external agnosys dep. One fewer node in majra's dependency graph; the dormant aarch64 cross-build concern (agnosys `SYS_OPEN`) is moot on that axis.
- **3.11.1 → 3.12.1 bump (majra 2.5.2 / cyrius 6.4.83, 2026-07-28)**: latest. majra's six-symbol surface (`ed25519_*`, `aes_gcm_*`) is unchanged — the four dist bundle bodies stay byte-identical (banner only). `dist/sigil.cyr` grows 25,391 → **26,254 lines**. **Carries a hard toolchain floor**: sigil 3.12.1's crypto-bank slot moved from the hardcoded `_SIGIL_CBANK_SLOT = 8` to a CAS-gated `thread_local_alloc()`, and that symbol first exists in the **6.4.64** snapshot (`TLOCAL_MAX_SLOTS` 16 → 128) — so sigil 3.12.1 + cyrius 6.4.62 does not build (`refusing to emit binary with N reachable undefined function(s)`). sigil's own comment says ≥ 6.4.65; 6.4.64 is where it actually lands. The sigil and cyrius pins therefore move together — **do not bump one without the other**. majra's own entry points were never exposed (`test_backends.tcyr` pulls `lib/tls.cyr` → `lib/thread_local.cyr` before sigil), but downstream `signed`/`backends` consumers are, because the dist bundles carry no `include "lib/…"` lines of their own. `lib/thread_local.cyr` was *already* required at 3.11.1 for `thread_local_{init,get,set}` — only `thread_local_alloc` is new. `test_backends` (`aes_gcm_roundtrip` / `encrypted_ipc` / `signed_envelope`) stays green. **One graph-shape consequence**: sigil's own manifest declares `[deps.sakshi] tag = "2.4.3"`, which `cyrius deps` overlays on top of the `lib sync --full` snapshot — and the 6.4.83 snapshot ships sakshi **2.4.6**. Left implicit that overlay silently downgrades `lib/sakshi.cyr`; majra now declares `[deps.sakshi]` itself to pin the resolution forward. See the sakshi section below.
- **3.7.14 → 3.11.1 bump (majra 2.5.1 / cyrius 6.4.62, 2026-07-13)**: latest. sigil's `signed`/`backends` surface (`ed25519_*`, `aes_gcm_*`) is unchanged — the four dist bundle bodies stay byte-identical (only the banner + re-subsetted `.deps` move). Picks up 3.9.9's crypto-bank slot fix (see the footprint-review block above), 3.10/3.11's UEFI Secure Boot enrollment (not majra-relevant), and the per-primitive `[lib.<type>]` profiles. agnosys dropped from the graph. `test_backends` (`aes_gcm_roundtrip` / `encrypted_ipc` / `signed_envelope`) stays green.
- **3.7.8 → 3.7.10 bump (majra 2.4.6 / cyrius 6.1.35, 2026-06-11)**: routine patch bump, no majra-side adjustment. sigil's `signed`/`backends` surface is unchanged — the four dist bundle bodies are byte-identical to 2.4.5. sigil bundles its own `u256_*` field arithmetic (24 fns in `bigint_ext`), so it has **no** dependency on the stdlib `lib/bigint.cyr` that cyrius 6.1.35 dropped (see the stdlib-modules note below).
- **3.7.10 → 3.7.14 bump (majra 2.4.7 / cyrius 6.2.11, 2026-06-15)**: routine patch bump alongside the cyrius 6.1.35 → 6.2.11 minor move, no majra-side adjustment. sigil's `signed`/`backends` surface is unchanged — the four dist bundle bodies stay byte-identical (only the version banner moves). Transitive agnosys rolled 1.3.2 → 1.4.3; `test_backends` (`aes_gcm_roundtrip` / `encrypted_ipc` / `signed_envelope`) stays green.

### sakshi — folded stdlib module (its `[deps.sakshi]` block was retired at the 6.5.18 pin)
- **Where**: `[deps].stdlib` in `cyrius.cyml`; `lib/sakshi.cyr` comes from the `cyrius lib sync --full` snapshot and tracks the toolchain pin. No commit-pin line in `cyrius.lock` — as of 2.6.8 there are no git deps at all, so the lockfile is 108 pure hashes.
- **Used by**: **nothing in `src/`.** sakshi reaches the compilation unit only transitively — patra calls `sakshi_error` / `sakshi_set_level` from its file/lib paths (`src/patra_queue.cyr` → `lib/patra.cyr`), and sigil carries it as its logging floor. `tests/test_backends.tcyr` and `tests/test_patra_queue.tcyr` include it explicitly so the symbols are present in the unit (the `ud2`-on-undefined rule).
- **Why majra declares a dep it never calls.** Cyrius resolves declared git deps and overlays them onto the `lib sync --full` snapshot. sigil's own manifest declares `[deps.sakshi] tag = "2.4.3"`; patra's declares `2.4.2`. The 6.4.83 stdlib snapshot ships sakshi **2.4.6**. Left implicit, `cyrius deps` overlaid sigil's 2.4.3 *over* the newer snapshot copy — a silent **downgrade**, surfaced only as a `./lib/ shadows version-pinned … sakshi 2.4.3 (pinned: 2.4.6)` warning on every subsequent build. A top-level `[deps.sakshi]` wins the resolution, so `lib/sakshi.cyr` now matches the snapshot byte-for-byte and the warning is gone.
- **Why 2.4.6 is safe against sigil's 2.4.3 expectation**: the span is backward-compatible. 2.4.4 is purely additive (`sakshi_trace_set_128` / `sakshi_trace_id_hi` / `_lo` for W3C 128-bit trace-ids; the 64-bit `sakshi_trace_set` / `sakshi_trace_id` are unchanged), 2.4.5 fixes the agnos `_sk_open` `O_RDWR`→`AO_WRONLY` access-mode fold (a real read-path bug on the agnos target, mirroring the cyrius 6.4.27 `lib/io.cyr` fix), and 2.4.6 is a toolchain pin catch-up with no behavior change. `test_backends` (42) + `test_patra_queue` (17) are green on 2.4.6 with sigil 3.12.1 in the same unit.
- **Why the block is gone.** sigil **3.12.7** dropped its own `[deps.sakshi]`, so there was nothing left to counteract — and keeping a git dep on a folded module carried the sidecar hazard described at the top of this file. The historical rationale below is retained because it is the worked example of the *downgrade* half of that hazard.

## Upgrade considerations

- **Cyrius compiler upgrades** — when `cyrius = "..."` in `cyrius.cyml` is bumped, run `cyrius lib sync --full && cyrius deps` to repopulate `lib/`, then recompile (`cyrius build --no-deps`) and re-run all four test suites (core + expanded + backends + patra_queue) + the soak set. The 2.4.5 jump (5.10.44 → 6.1.24) is the worked example for a *major*-spanning bump and is anything but byte-identical: the cyrius 6.x stdlib reorg renamed `http_*` → `sandhi_server_*`, split toolchain modules out (`slice`/`ct`/`chrono`/`async`/`dynlib`/`fdlopen`), and changed undefined-symbol handling to a runtime `ud2`. Budget real porting time and audit every entry point's reachable-undefined warnings — under cyrius 6.1.x a leftover undefined call is a latent SIGILL, not a build failure.
- **Stdlib changes** — `lib/` is gitignored; under cyrius 6.x `cyrius lib sync` copies the version-pinned snapshot into `./lib/` and `cyrius deps` overlays the git deps. The snapshot size tracks the toolchain: **94 files under 6.1.24, 88 under 6.1.35, 97 under 6.2.11, 99 under 6.4.62–6.4.83, 108 under 6.5.31 and still 108 under 6.5.35** (`--full` is load-bearing since 6.4.x — a bare `lib sync` copies only the declared `[deps].stdlib` subset). The `cyrius.lock` file in-tree carries SHA-256 hashes over all resolved files (**108 pure hashes at 2.6.8, no commit-pin line** — majra declares no git deps); CI's `cyrius deps --verify` enforces match. **Watch the snapshot's bundled copies of git-resolvable deps too**: when the snapshot ships a *newer* build of something a declared dep pins older (sakshi at 2.5.2), the overlay downgrades it silently — the only signal is the `./lib/ shadows version-pinned …` warning. **Always build with `--no-deps`** so the build's auto-`deps` doesn't perturb the synced lib. **Watch for snapshot drops on a cyrius bump**: a module listed in `[deps] stdlib` that the new snapshot no longer ships makes `cyrius deps` error with `cannot read ./lib/<mod>.cyr` — if majra has no live call site for it (grep `src/ tests/`), drop it from the `[deps] stdlib` list and any stale `include`. `bigint` was retired this way at 2.4.6.
- **sigil upgrades** — **there is no separate sigil bump any more.** Since 2.6.8 sigil rides the stdlib snapshot, so moving the cyrius pin is what moves sigil; the 6.5.35 snapshot folds 3.12.9, which is also the latest published tag. Check the fold against the published tag list on each cyrius bump — if the snapshot ever lags a sigil release majra needs, raise it upstream rather than re-adding a `[deps.sigil]` block (that would drop sigil from the published `.deps` sidecars again — see the sigil section above). On a future bump, rerun the full matrix and watch `test_backends` for any new `ud2`-SIGILL (a missing symbol sigil newly reaches into → add the providing `lib/<mod>.cyr` include). The majra-side payoff already banked: AES-NI / SHA-NI / ed25519-NI hardware acceleration for the `signed` + `backends` profiles. If a downstream needs only ONE primitive, point it at that primitive's `dist/sigil-<type>.cyr` profile (sigil 3.11.0) rather than the full bundle — see the sigil-footprint review above. QUIC transport is a separate longer-horizon item; needs X25519 from sigil too.
- **patra upgrades** — patra is resolved transitively via the cyrius stdlib snapshot (provisioned by `cyrius lib sync`). A patra upgrade that changes result-row column ordering, SELECT semantics, or aggregate-return shape would affect `src/patra_queue.cyr`; regression-test via `tests/test_patra_queue.tcyr` (17 assertions covering enqueue / dequeue priority order / status counts / persistence on reopen) before bumping the cyrius pin. The 2.4.3 migration to server-side `WHERE` + `ORDER BY` + `LIMIT` + `COUNT(*)` + `MAX()` exercises most of patra's SQL surface, so any parser regression should surface fast.
- **sandhi upgrades** — sandhi is folded into the cyrius stdlib (since the M6 fold-in). At cyrius 6.x its HTTP-server surface was renamed from the `http_*` namespace to `sandhi_server_*` (`src/admin.cyr` was ported at 2.4.5); the `HTTP_*` status constants stayed put. It also now references `TLS_BACKEND_LIBSSL` at parse time, so `lib/tls.cyr` must be included *before* `lib/sandhi.cyr`. Watch for further renames at sandhi major bumps.
