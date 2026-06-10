---
name: Majra Documentation Health
description: Living ledger of doc currency in the majra repo — fresh / stale / read-through / evergreen / frozen, refreshed as docs are touched
type: state
---

# Documentation Health — majra

> **Last refresh**: 2026-06-10 (2.4.5 cyrius 6.x migration — cyrius pin 5.10.44 → 6.1.24, sigil 2.9.0 → 3.7.8, agnosys → 1.3.2; new `lib sync` + `--no-deps` workflow; `admin.cyr`/`signed_envelope.cyr` ports. CHANGELOG + state.md + dependency-watch.md + roadmap.md + cyrius-quirks.md + CLAUDE.md + testing.md + both CI workflows touched. The sigil asm-drift P1 and the agnosys SYS_OPEN blocker are both RESOLVED.) | **Refresh cadence**: when docs are touched, update the affected row. No release attachment.
> **Scope**: This repo only (`majra`) — root-level files (README, CHANGELOG, CLAUDE.md, etc.) plus the entire `docs/` tree, plus `tests/soak/README.md`. Cross-repo dep/pin drift lives in [`development/dependency-watch.md`](development/dependency-watch.md), not here.

This is a **ledger**, not a one-time audit. Rewrite-in-place as docs change. Majra is the distributed-queue + multiplex engine for the AGNOS first-party tree (daimon, AgnosAI, hoosh, sutra, stiva, ifran, secureyeoman); stale module / SQL / crypto docs propagate downstream as consumer-side mis-integrations, so doc currency carries weight. The doc surface is small (~17 files) but most are load-bearing.

Pattern lifted from the agnosys ledger ([`agnosys/docs/doc-health.md`](https://github.com/MacCracken/agnosys/blob/main/docs/doc-health.md)) — same buckets, majra-shaped tiers (no ADRs yet; no audit/review cadence yet; benchmarks live as frozen point-in-time artifacts).

---

## At a glance — 2026-05-10 inventory

**~19 markdown files** total (6 root + 12 under `docs/` + 1 under `tests/soak/`). Buckets after the 2.4.2 + 2.4.3 ship arc + the 2026-05-10 CLAUDE audit restructure:

| Bucket | Count | What it means |
|---|---|---|
| ✅ **Fresh — touched in 2.4.2 → 2.4.3 cycle, the 2026-05-10 sweep, or the CLAUDE audit restructure** | 10 | `CHANGELOG.md`, `docs/development/roadmap.md`, `VERSION` (ship arc); `README.md`, `docs/development/dependency-watch.md`, `docs/guides/testing.md`, `docs/development/threat-model.md` (sweep); `CLAUDE.md` (rewritten per standards), `docs/development/state.md` (new — live state ledger), `docs/development/cyrius-quirks.md` (new — toolchain gotchas, formerly inlined in CLAUDE.md). All carry the cyrius-6.1.24 / sigil-3.7.8 / agnosys-1.3.2 reality (the 2.4.5 cyrius 6.x migration on 2026-06-10 — major toolchain bump, sigil unpinned to latest, new `lib sync` + `--no-deps` workflow, `http_*`→`sandhi_server_*` and `ct_eq`→`ct_eq_bytes_lens` source ports). |
| 🟡 **Stale — refresh in place** | 0 | All 3 stale rows from the initial audit closed in the 2026-05-10 sweep — see the "Sweep" block below. |
| 🟠 **Read-through outstanding** | 0 | `docs/development/threat-model.md` read-through completed 2026-05-10: all four trust-boundary claims (admin localhost-only + read-only contract, signed_envelope `ct_eq` constant-time pk compare, ipc_encrypted nonce limit + rekey warning, patra_queue payload-injection contract) verified against current src/ — no drift. Two minor wording fixes landed (sigil "vendored" → "resolved by `cyrius deps`" in the Crypto trust boundary + Supply Chain sections). |
| 🔵 **Probably evergreen** | 5 | `SECURITY.md`, `CODE_OF_CONDUCT.md`, `LICENSE`, `CONTRIBUTING.md`, `docs/development/semver.md`. No version-tied claims that drift between minor releases. Re-read pass annually. |
| 📦 **Archive / frozen by design** | 4 | `docs/benchmarks/results-v2.0.0.md` + `docs/benchmarks/rust-vs-cyrius-v2.0.0.md` (point-in-time at the 2.0.0 Cyrius-port cutover; the rust-vs-cyrius file is the HEADLINER — don't refresh in place); `docs/guides/migration-ifran.md` + `docs/guides/migration-secureyeoman.md` (consumer-specific migration recipes — frozen at the consumer's adoption point, not refreshed when majra moves). |
| ❓ **Open strategic question** | 0 | See [Open questions](#open-strategic-questions) for what would re-open. |

**Doc sweep completed 2026-05-10:**
- ✅ `README.md` L217 — cyrius 5.4.17 → 5.10.34 in the Rust→Cyrius comparison table.
- ✅ `docs/development/dependency-watch.md` — rewrote five items: (a) admin-profile row notes sandhi-from-stdlib path instead of `lib/http_server.cyr`; (b) stdlib modules table adds `tls.cyr` (transitive for sandhi's TLS-early-data references), drops the dated 5.4.x version notes on `hashmap.cyr` / `syscalls.cyr`; (c) `http_server.cyr` row replaced with `sandhi.cyr`; (d) `patra.cyr` row updated to 1.9.3 + the 2.4.3 SQL-surface migration note; (e) sigil "Why pinned" rewritten against the actual 2026-05-10 bisect (2.9.0 = pass, 2.9.1–3.0.1 = ed25519-NI SIGILL, 3.1.0 = aes-gcm-NI too) with a cross-link to the upstream sigil-side P1 issue; (f) "Upgrade considerations" rewritten with `cyrius deps` flow, the new sigil-upgrade gate (asm-stable NI dispatch surface), the patra/sandhi stdlib-fold story.
- ✅ `docs/guides/testing.md` — fixup-cap note rephrased to drop the dated "5.4.x" framing while keeping the 16384 number; reads as cc5-line-wide instead of stuck on the 2.4.0-era pin.
- ✅ `docs/development/threat-model.md` — read-through against current src/; all four trust-boundary claims verified; two wording fixes (sigil "vendored" → "resolved by `cyrius deps`").

**CLAUDE audit restructure completed 2026-05-10** (separate pass from the sweep — addresses standards violations that the sweep left in place):
- ✅ `CLAUDE.md` rewritten per [`first-party-documentation.md § CLAUDE.md`](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#claudemd) — 243 lines → ~155. Removed all inlined volatile state (test counts, dep versions, consumer list, bundle counts, "currently 5.10.34"). Added required sections: Goal, Current State pointer, P(-1) Hardening (replacing the scaffolding section since majra is past scaffolding), Docs pointer. Quick Start trimmed from ~15 commands to 5. Compiler quirks moved out (see below); allocator-discipline + struct-layout conventions kept as the "Cyrius Conventions" section.
- ✅ `docs/development/state.md` scaffolded — live volatile state ledger (version, cyrius pin, resolved dep versions, test counts per suite, bundle sizes, consumer table, recent releases, in-flight blockers). Cadence: refresh every release.
- ✅ `docs/development/cyrius-quirks.md` scaffolded — toolchain gotchas (clobbering, fixup cap, hashmap-key-type pairing, `var buf[N]` static-not-stack, inline-asm fragility) + a "resolved in cc5" historical footer. Lives under `docs/development/` because *that's where toolchain-affects-how-we-work content belongs* — not under `docs/architecture/` (which is for majra design notes, not language-side gotchas). Strikethrough-and-archive resolved entries when the cyrius pin moves; don't delete.

---

**Doc cleanup completed 2026-05-10 across the 2.4.2 / 2.4.3 ship arc** (separate from the sweep + CLAUDE audit blocks above; this block records what the ship arc itself touched):
- ✅ `CHANGELOG.md` — 2.4.2 + 2.4.3 stanzas. 2.4.2 captures the cyrius 5.10.34 jump + sandhi-from-stdlib + sigil pin rationale + CI installer fix. 2.4.3 captures the patra_queue server-side SQL migration.
- ✅ `CLAUDE.md` — cyrius pin reference + sigil tag + lib/ resolution model + cc5 quirks list trimmed for the 5.10.x floor. sandhi-from-stdlib path documented.
- ✅ `docs/development/roadmap.md` — 2.4.2 + 2.4.3 marked "Recently shipped"; the QUIC/AES-NI entry rewritten against the actual sigil-asm-offset bisect; agnosys SYS_OPEN portability filed under "Waiting on upstream"; the patra-1.1.1 workaround item retired (shipped in 2.4.3).
- ✅ A paired P1 issue + roadmap entry filed in **sigil** (not in this repo) capturing the inline-asm `[rbp-N]` offset drift, with the full bisect table majra's 2.4.2 work surfaced.

---

## Tier 1 — Root files

| File | Last touched | Status | Notes |
|---|---|---|---|
| `README.md` | 2026-05-11 | ✅ Fresh | Rust→Cyrius comparison table moved to `cyrius 5.10.44` at the 2.4.4 bump. Module map + 4-profile distribution surface + consumer list still match. |
| `CHANGELOG.md` | 2026-05-11 | ✅ Fresh | Source of truth for shipped work. Entries through 2.4.4 (cyrius 5.10.34 → 5.10.44 toolchain bump, sigil held at 2.9.0). |
| `CLAUDE.md` | 2026-05-10 | ✅ Fresh | Rewritten 2026-05-10 against [`first-party-documentation § CLAUDE.md`](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#claudemd) — durable rules only. Volatile state moved to [`docs/development/state.md`](development/state.md); compiler quirks moved to [`docs/development/cyrius-quirks.md`](development/cyrius-quirks.md). Now has all 8 required sections (identity / goal / state-pointer / quick-start / principles / rules / process-with-P(-1) / docs-pointer). ~155 lines, down from 243. |
| `CONTRIBUTING.md` | 2026-04-09 | 🔵 Evergreen | Generic contributor workflow + tone guidance. No version-tied claims. Re-read annually. |
| `SECURITY.md` | 2026-04-08 | 🔵 Evergreen | Reporting policy + scope. No version-tied claims; re-read annually. |
| `CODE_OF_CONDUCT.md` | 2026-03-21 | 🔵 Evergreen | Standard. |
| `VERSION` | 2026-05-11 | ✅ Fresh | `2.4.4` — single source of truth, read into `cyrius.cyml` via `${file:VERSION}`. |
| `LICENSE` | (initial commit) | 🔵 Evergreen | GPL-3.0-only. |

---

## Tier 2 — Architecture (`docs/architecture/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `overview.md` | 2026-04-19 | ✅ Fresh | Self-labels "majra (v2.4.x, ~5,500 lines across 22 modules)" — module map still matches src/. The 2.4.0 surface additions (signed_envelope, admin, patra_queue) are present. No dist-bundle drift to fix. Last touch predates 2.4.2/2.4.3 but content didn't drift. |

`docs/architecture/` holds majra design notes, not toolchain gotchas. The 2026-05-10 CLAUDE audit restructure initially mis-placed compiler quirks here; corrected and moved to [`docs/development/cyrius-quirks.md`](development/cyrius-quirks.md) per the convention that toolchain-affects-how-we-work content belongs under `development/`. No numbered architecture notes earned yet for majra design; open the series when a load-bearing invariant earns one.

---

## Tier 3 — Development (`docs/development/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `state.md` | 2026-06-10 | ✅ Fresh | Refreshed for the 2.4.5 cyrius 6.x migration: version 2.4.5, cyrius pin 6.1.24, sigil 3.7.8, agnosys 1.3.2, the lib-sync workflow note, 94-entry lockfile, and both upstream blockers moved to RESOLVED. Cadence: refresh every release; ideally bumped by the release post-hook. Pair-document with `doc-health.md` (state.md = code state; doc-health.md = doc state). |
| `cyrius-quirks.md` | 2026-05-10 | ✅ Fresh | Scaffolded by the CLAUDE audit restructure — five active toolchain gotchas + a "resolved in cc5" historical footer. Strikethrough-and-archive resolved entries when the cyrius pin moves; don't delete. Lives under `development/` (not `architecture/`) because it's about how the toolchain affects how the work happens, not about majra design. |
| `roadmap.md` | 2026-05-11 | ✅ Fresh | 2.4.4 entry added at top of "Recently shipped" (cyrius 5.10.34 → 5.10.44 toolchain bump). 2.4.2 + 2.4.3 entries retained. "Waiting on upstream" carries the sigil asm-offset drift (filed P1, still open at sigil 3.1.1) with the agnosys SYS_OPEN observation as a sub-bullet (resolves transitively when sigil unblocks). |
| `dependency-watch.md` | 2026-05-11 | ✅ Fresh | Refreshed at the 2.4.4 toolchain bump: 2026-05-11 sigil status-check bullet added under "Why exactly 2.9.0, not 2.9.x+" noting sigil 3.1.1 shipped a stdlib annotation pass rather than the P1 fix; "Cyrius compiler upgrades" upgrade-consideration now lists the 2.4.4 in-minor stair-step as the worked example alongside the 2.4.2 minor-spanning case; patra row notes 1.9.3 unchanged across the 5.10.34 → 5.10.44 snapshot range. Underlying 2026-05-10 rewrite (sigil bisect, sandhi M6 fold, stdlib table) carries through. |
| `semver.md` | 2026-04-08 | 🔵 Evergreen | Promise framing for the 2.x line. No version-tied details inside (just the promise + categories). |
| `threat-model.md` | 2026-05-10 | ✅ Fresh | Read-through completed 2026-05-10: admin localhost-only + read-only contract (src/admin.cyr L4, L8, L124), signed_envelope `ct_eq` constant-time pk compare (src/signed_envelope.cyr L125), ipc_encrypted nonce limit at 2^31 with hard-fail at 2^32 (src/ipc_encrypted.cyr L17, L127), patra_queue payload-injection contract (still applies after the 2.4.3 server-side WHERE rewrite — payload is the only consumer-provided string concatenated into SQL) — all four match current src/. Two wording fixes landed: sigil "first-party, vendored" → "resolved into `lib/sigil.cyr` by `cyrius deps`" in the Crypto trust boundary + Supply Chain sections. |

---

## Tier 4 — Guides (`docs/guides/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `testing.md` | 2026-05-10 | ✅ Fresh | Fixup-cap note rephrased in the 2026-05-10 sweep to drop the "5.4.x" framing while keeping the 16384 number — now reads as cc5-line-wide rather than stuck on a 2.4.0-era pin. |
| `migration-ifran.md` | 2026-04-08 | 📦 Frozen | Consumer-specific recipe captured at ifran's adoption point. Don't refresh in place when majra evolves; ifran is the source of truth for what got migrated. Re-author only if ifran returns for a second migration round. |
| `migration-secureyeoman.md` | 2026-04-08 | 📦 Frozen | Same posture as `migration-ifran.md`. |

---

## Tier 5 — Benchmarks (`docs/benchmarks/`)

Date / version-stamped, frozen by design. Each major perf cutover gets its own file — old files stay verbatim as the historical record. Live bench history lives in `bench-history.csv` (gitignored).

| File | Pinned at | Status | Notes |
|---|---|---|---|
| `results-v2.0.0.md` | v2.0.0 | 📦 Frozen | Snapshot of the v2.0.0 bench surface (Cyrius-port cutover). Don't refresh in place — capture the next snapshot in a new file (`results-v2.5.0.md` or similar) at the next significant perf-affecting release. Today's bench numbers from `bench-history.csv` differ from these in places (e.g. `pubsub_publish_nosub` improved); that's expected. |
| `rust-vs-cyrius-v2.0.0.md` | v2.0.0 | 📦 Frozen — HEADLINER | Rust→Cyrius port comparison at the 2.0.0 cutover. Heritage artifact, deliberately kept at `docs/benchmarks/`. |

---

## Tier 6 — Test READMEs

| File | Last touched | Status | Notes |
|---|---|---|---|
| `tests/soak/README.md` | 2026-04-19 | ✅ Fresh | Lists soak_queue / soak_pubsub / soak_relay / soak_heartbeat — all four targets ship per `tests/soak/`. No version-tied claims to drift; refresh only when targets are added/retired. |

---

## What this repo does NOT have yet (and doesn't need to invent)

The agnosys ledger has tiers for **ADRs**, **audit reports**, **engineering reviews**, and **engineering issues**. Majra has none of those structures, by design:

- **No ADRs.** Decision velocity is low; rationale rides CHANGELOG entries + roadmap notes + design comments. The 2.3.0 manifest migration, the 2.4.0 four-profile distlib, and the 2.4.2 sandhi-from-stdlib swap were all candidates and none earned an ADR — the entries in CHANGELOG carry the reasoning. Re-evaluate at v3.0.0 cut. Open the directory only when a decision is reversible-but-load-bearing and would benefit from a referenceable "we decided X because Y" artifact.
- **No `docs/audit/` cadence.** Majra's surface is smaller than agnosys's kernel-interface surface; there's no equivalent of the kernel-syscall-boundary attack surface that agnosys's P(-1) hardening passes were designed around. The fuzz harnesses (`fuzz/*.fcyr`) + soak tests carry the equivalent assurance for queue/pubsub/relay correctness. Open a `docs/audit/` directory only if a CVE pattern surfaces or a consumer asks for a structured audit artifact.
- **No `docs/development/reviews/` cadence.** Same logic — internal review artifacts emerge if/when audit cadence does.
- **No `docs/development/issues/` directory.** Upstream issues filed *by* majra live in the upstream repo's `issues/` directory (the sigil-2.9.1+ asm-offset issue we filed on 2026-05-10 sits at [`sigil/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md`](https://github.com/MacCracken/sigil/blob/main/docs/development/issues/2026-05-10-cyrius-510-asm-stack-frame-drift-breaks-ni-paths.md), not in majra). Cross-references from majra's roadmap point at those upstream tickets. Open `docs/development/issues/` only if majra accumulates self-internal blockers that don't belong upstream.

---

## Open strategic questions

None outstanding for the 2.4.3 cut. This section will repopulate when:

- A new doc category appears (e.g. an `adr/` if a reversible architectural decision needs a referenceable record, or `audit/` if a CVE pattern or consumer asks for it).
- A consumer migration becomes generic enough that `docs/guides/migration-*.md` should be replaced by a single `docs/guides/adopting-majra.md`. Currently per-consumer is the right shape.
- A second benchmark snapshot lands and we need to decide whether `bench-history.csv` (gitignored, live data) supersedes the date-pinned files entirely.

---

## In-flight (blocked, not stale)

- **sigil asm-offset drift** — paired P1 issue filed in sigil; majra-side mitigation (pin at 2.9.0) is in place and documented in CHANGELOG 2.4.2 + roadmap "Waiting on upstream". No majra-side action owed until sigil ships the fix.
- **`lib/agnosys.cyr:791` SYS_OPEN aarch64 portability** — transitive dep blocker for aarch64 cross-build. No consumer asks for an aarch64 majra binary today; passive tracker only.

---

## Forward doc-policy commitments

| # | Commitment | Trigger | Source | Notes |
|---|---|---|---|---|
| 1 | **Bench snapshot retention** — keep `docs/benchmarks/results-v*.md` + `rust-vs-cyrius-v*.md` verbatim through at least v3.0.0; re-evaluate at the major cut whether pre-2.0 snapshots get folded into a single historical summary. | v3.0.0 cut | This file | Today the surface is 2 files (the v2.0.0 cutover pair) — purge pressure is zero. |
| 2 | **Migration guide retention** — per-consumer recipes stay frozen at the consumer's adoption point. If a consumer returns for a second migration round, write a second file (`migration-<consumer>-v2.md`) rather than refreshing the original in place. | When the next consumer migrates | This file | Pattern keeps the historical record clean. |
| 3 | **Open audit/review tiers only on a real trigger.** Don't add empty `docs/audit/` or `docs/development/reviews/` directories — they'll degrade into checklist noise without a forcing function. | When a CVE pattern or a consumer ask materialises | This file | Agnosys earned its audit cadence from kernel-syscall-boundary work; majra doesn't have an equivalent surface today. |

---

## Refresh procedure

When docs are touched:

1. Find the affected row in the relevant tier table.
2. Update **Last touched** column to the new date.
3. Update **Status** column if the bucket changed.
4. Update **Notes** column if the next step changed.
5. If a doc moved or was archived, update its row to reflect the new home.
6. Re-anchor "Last refresh" date in the header.

When the bucket counts at the top drift by more than ~2 in any cell, refresh the at-a-glance table.

This file's refresh cadence is **opportunistic** (touched when other docs are touched), not periodic and not tied to releases. The 2.4.2 → 2.4.3 ship arc established the baseline; future minor cuts' doc-sync step touches this file alongside CHANGELOG + roadmap when something here actually drifts.

---

## What this file is NOT

- Not a substitute for [`development/dependency-watch.md`](development/dependency-watch.md) (which holds live cyrius / sigil / patra / sandhi version-tracking state).
- Not a CHANGELOG (which records what shipped, not what's stale).
- Not a roadmap (forward work lives in [`development/roadmap.md`](development/roadmap.md)).
- Not a per-doc review log (we record the result of an audit pass, not the per-doc reasoning).

---

*Last refresh: 2026-05-11 (2.4.4 cyrius 5.10.34 → 5.10.44 toolchain bump — CHANGELOG / state.md / dependency-watch.md / VERSION rows refreshed). Refresh in place when docs are touched.*
