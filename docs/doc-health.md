---
name: Majra Documentation Health
description: Living ledger of doc currency in the majra repo — fresh / stale / read-through / evergreen / frozen, refreshed as docs are touched
type: state
---

# Documentation Health — majra

> **Last refresh**: 2026-05-10 (initial audit, after the 2.4.2 + 2.4.3 ship arc) | **Refresh cadence**: when docs are touched, update the affected row. No release attachment.
> **Scope**: This repo only (`majra`) — root-level files (README, CHANGELOG, CLAUDE.md, etc.) plus the entire `docs/` tree, plus `tests/soak/README.md`. Cross-repo dep/pin drift lives in [`development/dependency-watch.md`](development/dependency-watch.md), not here.

This is a **ledger**, not a one-time audit. Rewrite-in-place as docs change. Majra is the distributed-queue + multiplex engine for the AGNOS first-party tree (daimon, AgnosAI, hoosh, sutra, stiva, ifran, secureyeoman); stale module / SQL / crypto docs propagate downstream as consumer-side mis-integrations, so doc currency carries weight. The doc surface is small (~17 files) but most are load-bearing.

Pattern lifted from the agnosys ledger ([`agnosys/docs/doc-health.md`](https://github.com/MacCracken/agnosys/blob/main/docs/doc-health.md)) — same buckets, majra-shaped tiers (no ADRs yet; no audit/review cadence yet; benchmarks live as frozen point-in-time artifacts).

---

## At a glance — 2026-05-10 inventory

**~17 markdown files** total (6 root + 10 under `docs/` + 1 under `tests/soak/`). Buckets after the 2.4.2 + 2.4.3 ship arc:

| Bucket | Count | What it means |
|---|---|---|
| ✅ **Fresh — touched in 2.4.2 → 2.4.3 cycle** | 4 | `CHANGELOG.md`, `CLAUDE.md`, `docs/development/roadmap.md`, `VERSION`. All carry the cyrius-5.10.34 / sigil-2.9.0 / sandhi-from-stdlib / patra-1.9.3 reality. |
| 🟡 **Stale — refresh in place** | 3 | `README.md` (Rust→Cyrius comparison table at L217 still cites `cyrius 5.4.17`), `docs/development/dependency-watch.md` (says sigil pins to 2.9.0 because of cyrius 5.5.x asm-fix promise; reality is now the sigil-2.9.1+ asm-offset drift documented in `roadmap.md` + the sigil-side issue we filed; also says "patra 1.1.1 ships with cyrius stdlib" — now 1.9.3; also lists `http_server.cyr` under stdlib modules — folded into `sandhi`), `docs/guides/testing.md` (refers to the cyrius 5.4.x fixup cap; functionally still right at 16384 in 5.10.x but the version note is dated). |
| 🟠 **Read-through outstanding** | 1 | `docs/development/threat-model.md` — last touched 2026-04-19, pre-2.4.x. The 2.4.0 surface adds (signed_envelope, admin, patra_queue) all show up in the file but were written against the original sigil 2.8.4 / vendored http_server world. Read-through against current src/ to confirm the trust-boundary claims (Ed25519 verification ABI, admin endpoint's localhost-only assertion, patra-queue's on-disk persistence model) still match. |
| 🔵 **Probably evergreen** | 5 | `SECURITY.md`, `CODE_OF_CONDUCT.md`, `LICENSE`, `CONTRIBUTING.md`, `docs/development/semver.md`. No version-tied claims that drift between minor releases. Re-read pass annually. |
| 📦 **Archive / frozen by design** | 4 | `docs/benchmarks/results-v2.0.0.md` + `docs/benchmarks/rust-vs-cyrius-v2.0.0.md` (point-in-time at the 2.0.0 Cyrius-port cutover; the rust-vs-cyrius file is the HEADLINER — don't refresh in place); `docs/guides/migration-ifran.md` + `docs/guides/migration-secureyeoman.md` (consumer-specific migration recipes — frozen at the consumer's adoption point, not refreshed when majra moves). |
| ❓ **Open strategic question** | 0 | See [Open questions](#open-strategic-questions) for what would re-open. |

**Doc cleanup completed 2026-05-10 across the 2.4.2 / 2.4.3 ship arc:**
- ✅ `CHANGELOG.md` — 2.4.2 + 2.4.3 stanzas. 2.4.2 captures the cyrius 5.10.34 jump + sandhi-from-stdlib + sigil pin rationale + CI installer fix. 2.4.3 captures the patra_queue server-side SQL migration.
- ✅ `CLAUDE.md` — cyrius pin reference + sigil tag + lib/ resolution model + cc5 quirks list trimmed for the 5.10.x floor. sandhi-from-stdlib path documented.
- ✅ `docs/development/roadmap.md` — 2.4.2 + 2.4.3 marked "Recently shipped"; the QUIC/AES-NI entry rewritten against the actual sigil-asm-offset bisect; agnosys SYS_OPEN portability filed under "Waiting on upstream"; the patra-1.1.1 workaround item retired (shipped in 2.4.3).
- ✅ A paired P1 issue + roadmap entry filed in **sigil** (not in this repo) capturing the inline-asm `[rbp-N]` offset drift, with the full bisect table majra's 2.4.2 work surfaced.

---

## Tier 1 — Root files

| File | Last touched | Status | Notes |
|---|---|---|---|
| `README.md` | 2026-04-19 | 🟡 Stale | L217 Rust→Cyrius comparison table cites `cyrius 5.4.17` — refresh to `cyrius 5.10.34`. Otherwise structurally fine; module map + 4-profile distribution surface + consumer list still match. Quick edit when next touched. |
| `CHANGELOG.md` | 2026-05-10 | ✅ Fresh | Source of truth for shipped work. Entries through 2.4.3. |
| `CLAUDE.md` | 2026-05-10 | ✅ Fresh | Durable rules. Refreshed in 2.4.2: cyrius pin (5.10.34), sigil pin (2.9.0 + reason), lib/-is-resolved-by-cyrius-deps note, sandhi-from-stdlib path, cc5 5.10.x quirks list. |
| `CONTRIBUTING.md` | 2026-04-09 | 🔵 Evergreen | Generic contributor workflow + tone guidance. No version-tied claims. Re-read annually. |
| `SECURITY.md` | 2026-04-08 | 🔵 Evergreen | Reporting policy + scope. No version-tied claims; re-read annually. |
| `CODE_OF_CONDUCT.md` | 2026-03-21 | 🔵 Evergreen | Standard. |
| `VERSION` | 2026-05-10 | ✅ Fresh | `2.4.3` — single source of truth, read into `cyrius.cyml` via `${file:VERSION}`. |
| `LICENSE` | (initial commit) | 🔵 Evergreen | GPL-3.0-only. |

---

## Tier 2 — Architecture (`docs/architecture/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `overview.md` | 2026-04-19 | ✅ Fresh | Self-labels "majra (v2.4.x, ~5,500 lines across 22 modules)" — module map still matches src/. The 2.4.0 surface additions (signed_envelope, admin, patra_queue) are present. No dist-bundle drift to fix. Last touch predates 2.4.2/2.4.3 but content didn't drift. |

---

## Tier 3 — Development (`docs/development/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `roadmap.md` | 2026-05-10 | ✅ Fresh | 2.4.2 + 2.4.3 marked "Recently shipped". "Waiting on upstream" carries the sigil asm-offset drift bisect + the agnosys SYS_OPEN portability item. patra-WHERE workaround retired. |
| `dependency-watch.md` | 2026-04-19 | 🟡 Stale | Multiple inaccuracies: (a) sigil rationale still talks about the cyrius 5.5.x inline-asm fix as the gate — actual story is now the sigil-2.9.1+ asm-offset drift (see roadmap.md + the sigil-side P1 issue); (b) lists `http_server.cyr` under stdlib modules — folded into `sandhi` as of 5.10.34; (c) "patra 1.1.1 ships with cyrius stdlib" — actually 1.9.3 in the cyrius 5.10.34 snapshot, and 2.4.3 retired the 1.1.1-shaped workarounds. Rewrite next time it's touched. |
| `semver.md` | 2026-04-08 | 🔵 Evergreen | Promise framing for the 2.x line. No version-tied details inside (just the promise + categories). |
| `threat-model.md` | 2026-04-19 | 🟠 Read-through | Pre-2.4.x. Read-through against current src/ to confirm the trust-boundary claims (Ed25519 verify ABI from signed_envelope, admin endpoint's localhost-only contract, patra_queue's on-disk persistence model) still match the as-shipped surface. No urgent gap surfaced — call it before the next minor cut. |

---

## Tier 4 — Guides (`docs/guides/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `testing.md` | 2026-04-19 | 🟡 Stale (light) | The Cyrius 5.4.x fixup-cap-16384 note is functionally still right under 5.10.x but the version reference is dated. Three-line touch next time the file is edited. |
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

*Last refresh: 2026-05-10 (initial audit, after the 2.4.2 + 2.4.3 ship arc). Refresh in place when docs are touched.*
