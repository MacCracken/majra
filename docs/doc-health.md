---
name: Majra Documentation Health
description: Living ledger of doc currency in the majra repo — fresh / evergreen / frozen, refreshed as docs are touched
type: state
---

# Documentation Health — majra

> **Last refresh**: 2026-10-30 (2.12.1) — the cyrius pin moved 6.7.9 → 6.7.18:
> every doc that carries the pin, a fold version or a test count re-measured,
> and the shipped roadmap item (the pin move) deleted. The 2.12.0 refresh before
> it (the rate-limiter key cap, the metrics hook arguments) is in that stanza.
> **Scope**: this repo only — root files, the whole `docs/` tree and
> `tests/soak/README.md`. Cross-repo pin drift lives in
> [`development/dependency-watch.md`](development/dependency-watch.md).

A **ledger**, not a one-time audit: rewrite rows in place as docs change. Stale
module, SQL or crypto docs here propagate downstream as consumer
mis-integrations, so currency carries weight. Pattern from the
[agnosys ledger](https://github.com/MacCracken/agnosys/blob/main/docs/doc-health.md),
majra-shaped.

---

## At a glance — 2026-10-15

**29 tracked markdown files**: 6 at the root, 22 under `docs/`, 1 under
`tests/soak/`.

| Bucket | Count | What it means |
|---|---|---|
| ✅ **Fresh** — checked on 2026-10-15 (2.11.0 sweep, 2.12.0 touch-ups); the pin-carrying ones again on 2026-10-30 (2.12.1) | 15 | Living docs; refreshed every release |
| 🔵 **Evergreen** — no version-tied claims | 1 | `CODE_OF_CONDUCT.md` (plus `LICENSE`, not markdown). Re-read annually |
| 📦 **Frozen by design** — point-in-time records, never refreshed in place | 13 | 4 audit reports, 2 benchmark snapshots, 4 migration guides, 3 archived issues |
| 🟡 **Stale** | 0 | |

---

## Tier 1 — Root files

| File | Last touched | Status | Notes |
|---|---|---|---|
| `README.md` | 2026-10-30 | ✅ Fresh | Module table (ratelimit names the key cap), architecture header (v2.12.1: 10,365 lines across 22 modules; 11,239 across 23 files with `src/main.cyr`), sidecar table and toolchain floor (backends re-checked on 6.6.20–6.7.9 at 2.12.1), test count 1,571, toolchain row 6.7.18. Names the version in six places — re-measure each on every release |
| `CHANGELOG.md` | 2026-10-30 | ✅ Fresh | Source of truth for shipped work. `[2.12.1]` is the 6.7.18 pin move; `[2.12.0]` folds in the post-2.11.0 roadmap / doc sweep; the stray empty `[Unreleased]` heading between 2.1.1 and 2.1.0 (compare link to v1.0.4) is gone |
| `CLAUDE.md` | 2026-10-30 | ✅ Fresh | Durable rules only — no inlined state. Language conventions re-checked at 6.7.18 (bare `return;` refused in a `: bool` fn, block-scoped `var`; reserved words at 6.7.9); the roadmap description matches its release-target shape |
| `CONTRIBUTING.md` | 2026-10-15 | ✅ Fresh | Points at `sh tests/test.sh`; `cyrius audit` vs `cyrius deny` split stated; style examples name code that exists. One toolchain-tied figure (the fixup table) tracks `cyrius-quirks.md` §2 |
| `SECURITY.md` | 2026-10-15 | ✅ Fresh | Attack-surface rows re-checked against the code at 2.11.0; supported-versions table reads `current: see VERSION`, so it does not drift per release |
| `CODE_OF_CONDUCT.md` | 2026-08-22 | 🔵 Evergreen | Standard text |
| `VERSION` (not markdown) | 2026-10-15 | ✅ Fresh | `2.12.0`, pending tag |

## Tier 2 — Architecture (`docs/architecture/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `overview.md` | 2026-10-15 | ✅ Fresh | Module map labelled v2.12.0; principle 3 records `metrics` as the last global holdout, gone at 2.12.0; principle 5 names the key cap; latencies quote the 2.11.0 bench row |

`docs/architecture/` holds majra design notes, not toolchain gotchas (those live
in [`development/cyrius-quirks.md`](development/cyrius-quirks.md)). No numbered
notes earned yet.

## Tier 3 — Development (`docs/development/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `state.md` | 2026-10-30 | ✅ Fresh | Refreshed every release (2.12.1: pin 6.7.18, fold versions, lock, footprint, counts; the adopted sigil-warning row dropped from In-flight). In-flight table holds open items only, each pointing at its roadmap target; footprint and lockfile history condensed |
| `roadmap.md` | 2026-10-30 | ✅ Fresh | Forward-facing only, by release target (2.12.x / later MINORs / 3.0.0); the pin move shipped at 2.12.1 and was deleted, so no MINOR has a fired trigger. Every item re-checked open at 2.12.1, upstream items at the latest *tagged* cyrius (6.7.18) and sigil (3.13.15) |
| `dependency-watch.md` | 2026-10-30 | ✅ Fresh | sigil 3.13.15 / patra 1.16.6 / sandhi 1.13.7 / sakshi 2.5.10 / bayan 1.11.1 at the 6.7.18 pin, each its latest tag; the static-buffer watch resolved; `ESYSXLAT` re-derived (82, unchanged) |
| `cyrius-quirks.md` | 2026-10-30 | ✅ Fresh | Header stamped 6.7.18 (#2, #7, #9, #10 re-measured; hashmap byte-identical, so #3 / #13 hold); #1 re-tensed (not seen on 6.x); #3 names the current job-key shape |
| `semver.md` | 2026-10-15 | ✅ Fresh | Promise, documented exceptions (four renames, three 2.6.9 signature changes, two 2.9.0 wire breaks), deprecations (bare `ERR_*`; `_metrics_arg3` / `_metrics_arg4` since 2.12.0); MAJOR line points at roadmap § 3.0.0 |
| `threat-model.md` | 2026-10-30 | ✅ Fresh | sigil version under the 6.7.18 pin. Audit history lists all four passes in order; nonce row carries the volume limits; the ratelimit row names the key cap and its ⚠ |
| `issues/archived/2026-09-26-agnosai-ratelimit-no-way-to-enforce-a-key-cap.md` | 2026-10-15 | 📦 Frozen | Resolved at 2.12.0 (`ratelimit_evict_to`); the resolution paragraph carries the §4 numbers and agnosai's one-line adoption |
| `issues/archived/2026-08-10-pq-dequeue-…md` | 2026-08-11 | 📦 Frozen | Resolved at 2.6.2 |
| `issues/archived/2026-09-14-raw-x86-syscall-numbers-aarch64.md` | 2026-09-15 | 📦 Frozen | Resolved at 2.7.3; its body is the measurement record |

Issue convention: an archived file keeps its body and gains
`**Status:** ✅ **RESOLVED in majra <version>** (<date>)` plus a
`**Resolution:**` paragraph. Issues majra files *upstream* live in the upstream
repo.

## Tier 4 — Guides (`docs/guides/`)

| File | Last touched | Status | Notes |
|---|---|---|---|
| `testing.md` | 2026-10-30 | ✅ Fresh | Counts at 2.12.1 (1,571 CI + 36 live); pin stamps 6.7.18; the qemu-aarch64 recipe and loop guidance; `tests/test.sh` |
| `migration-2.9.0.md` | 2026-09-15 | 📦 Frozen | The 2.9.0 consumer guide (two wire breaks, fail-closed `verify(se, 0)`); about that release |
| `migration-2.6.9.md` | 2026-08-22 | 📦 Frozen | The 2.6.9 / 2.6.10 hardening guide |
| `migration-ifran.md` | 2026-08-22 | 📦 Frozen | Consumer recipe at ifran's adoption point; ⚠ banner — not safe to copy against today's API |
| `migration-secureyeoman.md` | 2026-08-22 | 📦 Frozen | Same posture as `migration-ifran.md` |

## Tier 5 — Benchmarks and audits

| File | Pinned at | Status | Notes |
|---|---|---|---|
| `docs/benchmarks/results-v2.0.0.md` | v2.0.0 | 📦 Frozen | The Cyrius-port cutover snapshot. Live history is `bench-history.csv` (tracked) |
| `docs/benchmarks/rust-vs-cyrius-v2.0.0.md` | v2.0.0 | 📦 Frozen — headliner | The Rust → Cyrius comparison |
| `docs/audit/2026-08-22-audit.md` | 2.6.9 | 📦 Frozen | First P(-1) pass, 115 findings |
| `docs/audit/2026-08-22-audit-pass2.md` | 2.6.10 | 📦 Frozen | Second pass, 68 findings (50 were 2.6.9 regressions). Its one never-scheduled LOW — the metrics globals — was fixed at 2.12.0 |
| `docs/audit/2026-09-15-audit.md` | 2.8.1 | 📦 Frozen | Third pass, 130 findings |
| `docs/audit/2026-10-15-audit.md` | 2.10.1 | 📦 Frozen | Fourth pass, 12 findings, after the 6.6.20 and 6.7.9 language bumps |

## Tier 6 — Test READMEs

| File | Last touched | Status | Notes |
|---|---|---|---|
| `tests/soak/README.md` | 2026-10-15 | ✅ Fresh | Load figures match the soak sources (queue 500k ops); both former "known limitations" re-stated as resolved |

---

## What this repo deliberately does not have

- **No ADRs.** Rationale rides CHANGELOG entries and design comments; no decision
  has needed a referenceable record yet. Re-evaluate at the 3.0.0 cut.
- **`docs/audit/` is point-in-time, not a cadence** — one frozen report per P(-1)
  pass. No `reviews/` directory without an equivalent forcing function.

## Forward commitments

| # | Commitment | Trigger |
|---|---|---|
| 1 | Keep `docs/benchmarks/` snapshots verbatim through 3.0.0; decide then whether `bench-history.csv` supersedes date-pinned files | 3.0.0 cut |
| 2 | Per-consumer migration recipes stay frozen; a second round gets a second file (`migration-<consumer>-v2.md`) | Next consumer migration |
| 3 | When an audit finding is deferred, it goes on the roadmap in the same cut — the pass-2 metrics finding sat unscheduled for seven weeks | Every P(-1) pass |

## Refresh procedure

When a doc is touched: update its row (date, status, a one-line note on where
it stands — not what changed; that is the CHANGELOG's job), and re-anchor the
header date. Refresh the at-a-glance counts when a file is added, archived or
frozen. This file is touched on every release's doc-sync step (`CLAUDE.md`
dev-loop step 7, closeout step 4, P(-1) step 8) and whenever another doc is.

## What this file is NOT

- Not dependency tracking — [`development/dependency-watch.md`](development/dependency-watch.md).
- Not a changelog — rows say where a doc stands, not its history.
- Not a roadmap — forward work lives in [`development/roadmap.md`](development/roadmap.md).
