# `ratelimit_new` / `ratelimit_check` collide with consumer limiters — namespace as `majra_ratelimit_*`

**Filed:** 2026-06-23 (by a hoosh consumer — hoosh 2.4.7 toolchain bump to cyrius 6.2.37)
**Severity:** Medium — `last-definition-wins` build warning today; **arity
mismatch** makes a silent wrong-binding a real corruption risk if include order
ever flips.
**Component:** `src/ratelimit.cyr` — `ratelimit_new(rate_x1000, burst)` (`:22`),
`ratelimit_check(rl, key)` (`:35`) → `dist/majra.cyr:546,559` (and
`dist/majra-signed.cyr`, `dist/majra-backends.cyr`, `dist/majra-admin.cyr`).
**majra's role: RECOMMENDED FIX OWNER.** majra's public limiter names are the
generic ones; a consumer's own `ratelimit_*` is the natural keeper of the bare
name. Part of the broader ecosystem namespacing effort (see Cross-references).
**Repos:** majra `2.4.7` (sibling ERR_* enum issues filed in sigil/yukti/bote/
sakshi/ai-hwaccel).

## Summary

majra exports public **functions** whose names collide with a consumer-defined
limiter of a **different shape**:

| Definer | Symbol | Signature / shape |
|---|---|---|
| **majra 2.4.7** | `ratelimit_new` | `(rate_x1000, burst)` → 48-byte RateLimiter `{rate, burst, buckets map, mutex, allowed, rejected}` |
| **majra 2.4.7** | `ratelimit_check` | `(rl, key)` → per-key token bucket |
| hoosh 2.4.7 | `ratelimit_new` | `(rpm)` → hoosh's own limiter struct |
| hoosh 2.4.7 | `ratelimit_check` | `(rl)` → single-bucket check |

Cyrius include semantics are textual paste + **last-definition-wins (with a
warning)**. hoosh vendors majra's core pubsub (`src/vendor/majra.cyr`) for its
event bus and sees:

```
warning:src/lib/ratelimit.cyr:12: duplicate fn 'ratelimit_new' (last definition wins)
warning:src/lib/ratelimit.cyr:42: duplicate fn 'ratelimit_check' (last definition wins)
```

## Why the arity mismatch makes this sharper than the ERR_* enum cases

For the enum-constant collisions the value just changes. Here the two functions
have **different arities** (`(rate_x1000, burst)` vs `(rpm)`; `(rl, key)` vs
`(rl)`). Today hoosh wins (its definitions are included last) and majra's copies
are dead code in the pubsub core — harmless. But if include order ever flips so
majra's `ratelimit_check(rl, key)` wins, every hoosh call `ratelimit_check(rl)`
passes one arg to a two-arg function → reads an undefined `key` slot and a
`map_get(buckets, <garbage>)` — a memory-safety bug, not just a warning. The
collision is order-fragile, which is exactly what namespacing removes.

## Recommended fix

Prefix majra's public limiter surface `ratelimit_* → majra_ratelimit_*` and
`sliding_window_* → majra_sliding_window_*` for consistency, updating internal
callers (`src/main.cyr:157-168` test harness) and regenerating all `dist/majra*`
bundles:

- `ratelimit_new` → `majra_ratelimit_new`
- `ratelimit_check` → `majra_ratelimit_check`
- `ratelimit_key_count` / `ratelimit_evict_stale` / `ratelimit_stats` → `majra_ratelimit_*`
- `sliding_window_*` → `majra_sliding_window_*`

Breaking change to majra's exported surface → suggest **majra 2.5.0**, optionally
keeping bare aliases for one minor if direct downstreams exist.

## Interim (consumer-side)

hoosh tolerates the warning today (last-wins keeps hoosh's limiter; majra's are
dead in its core pubsub). The upstream rename removes the order-fragility for all
majra consumers. (Precedent: szal already vendor-renames a 9-symbol majra set
locally — this retires that workaround.)

## Cross-references

- ai-hwaccel `2026-06-11-registry-new-collision.md` (same class — function-name collision, fixed by namespacing).
- sigil/yukti/bote/sakshi/ai-hwaccel `…2026-06-23-err-*-enum-collision-namespace.md` (sibling enum-constant collisions).
