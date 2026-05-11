# Architecture Notes

Numbered, never-renumbered notes covering **invariants, constraints, and quirks that a reader cannot derive from the code alone**. Decision rationale goes in `docs/adr/`; system-level module maps + data flow live in `overview.md`; this directory is for the gotchas.

Per [first-party-documentation § Architecture Notes](https://github.com/MacCracken/agnosticos/blob/main/docs/development/planning/first-party-documentation.md#architecture-notes).

## Index

| # | Title | Affects |
|---|---|---|
| [001](001-cyrius-compiler-quirks.md) | Cyrius compiler quirks (5.10.x floor) | all modules |

## Adding a note

1. Pick the next free three-digit number — **never renumber existing notes** even if one is retired (mark as superseded in-place).
2. Filename: `NNN-kebab-case-title.md`.
3. Add a row to the index above with a one-line hook including *what it affects* (so a reader skimming the index can tell if it matters to their work).
4. The note documents *reality*, not *what we chose*. Decisions go in `docs/adr/`.

## See also

- [`overview.md`](overview.md) — system-level module map, data flow, dist-profile composition.
- [`../adr/`](../adr/) — decision records (currently empty; majra hasn't needed an ADR yet).
