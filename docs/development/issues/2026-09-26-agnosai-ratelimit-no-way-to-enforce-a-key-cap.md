# `ratelimit_evict_stale` can no longer enforce a hard key cap — a consumer needs an explicit force-evict

**Status:** 🔴 **OPEN — MEASURED** at majra **2.9.1** (latest tag at filing, worktree clean). The
behaviour dates from **2.8.1**'s "evict only a bucket that has refilled to burst" change.
**Re-checked:** 2026-10-15 at majra **2.11.0** — still open (no force-evict / key-cap call); scheduled as the headline of [`roadmap.md` § 2.12.0](../roadmap.md).
**Filed:** 2026-09-26, by agnosai (the 2.0.10 bump from majra 2.7.2 → 2.9.1). The request is for a new
opt-in call. It is not a request to revert 2.8.1, whose metering fix is right.
**Severity:** P2 for a consumer that bounds limiter memory under key-spraying. The memory is still
bounded, but by a refill interval of keys rather than by the consumer's cap. See §1 for the numbers.
**Component:** `src/ratelimit.cyr:230` (`ratelimit_evict_stale`), the refill gate at `:253`, and
`_ratelimit_projected` at `:82` (line numbers are 2.9.1's).

## 1. What was measured

agnosai holds its rate-limit map at 4,096 keys. Once over the cap it sweeps with
`ratelimit_evict_stale(rl, 0)` — "anything idle at all" — so that a client minting a fresh key per request
cannot grow the map (agnosai `src/server/rate_limit.cyr:64`, `:82`, `:409`). Since 2.8.1 that sweep skips
every bucket that has not refilled to burst. A sprayed key has spent one token, so its bucket is
unevictable for `1/rate` seconds after its only request.

**Repro, majra-only** (run, not reasoned). Save it as `repro.tcyr` anywhere. Then, from a checkout of the
tag, run `cyrius lib sync --full`, `cyrius build --no-deps /path/repro.tcyr /tmp/repro` and `/tmp/repro`.
It uses the same includes as `tests/test_core.tcyr`, which majra builds with `--no-deps`:

```cyrius
include "lib/string.cyr"
include "lib/fmt.cyr"
include "lib/alloc.cyr"
include "lib/freelist.cyr"
include "lib/vec.cyr"
include "lib/str.cyr"
include "lib/hashmap.cyr"
include "lib/syscalls.cyr"
include "lib/chrono.cyr"
include "lib/tagged.cyr"
include "lib/fnptr.cyr"
include "lib/thread.cyr"
include "lib/assert.cyr"
include "src/error.cyr"
include "src/counter.cyr"
include "src/envelope.cyr"
include "src/namespace.cyr"
include "src/metrics.cyr"
include "src/ratelimit.cyr"

fn main(): i64 {
    alloc_init();
    var rl = ratelimit_new(1000, 200);          # 1 req/s, burst 200
    var i = 0;
    while (i < 20000) {                         # 20,000 distinct keys, one request each
        var sb = str_builder_new();
        str_builder_add_cstr(sb, "spray-");
        str_builder_add_int(sb, i);
        ratelimit_check(rl, str_data(str_builder_build(sb)));
        i = i + 1;
    }
    var before = ratelimit_key_count(rl);
    var evicted = ratelimit_evict_stale(rl, 0);
    println("before / evicted / after:");
    fmt_int(before); println(""); fmt_int(evicted); println(""); fmt_int(ratelimit_key_count(rl)); println("");
    return 0;
}
var r = main();
syscall(60, r);
```

| tag (its own pin) | before | evicted by `evict_stale(rl, 0)` | after |
|---|---|---|---|
| **2.7.2** (6.6.2) | 20,000 | **20,000** | **0** |
| **2.9.1** (6.6.6) | 20,000 | **0** | **20,000** |

Through agnosai's own limiter (200,000 fresh keys, sweeping every 256 requests), the peak tracked-key count
was:

- **default 100 req/s:** 4,352 on both tags. The 10 ms refill hides the change.
- **strict 1 req/s:** 4,352 on 2.7.2, but **72,447 on 2.9.1**, 17.7× the cap.

The 2.9.1 sweep is also ~115× cheaper under this spray, because it no longer allocates and it compacts
tombstones. That part is a real win and should stay.

## 2. Why a flag rather than a revert

2.8.1's reasoning holds for idle eviction. Evicting a partly-drained bucket hands its client a fresh burst,
which is a metering bug. A consumer under memory pressure is making a different, explicit trade: it would
rather forget some clients' spent tokens than let an attacker grow the map. Today majra offers no call that
makes that trade, so the consumer cannot hold its cap at all.

## 3. Requested

An opt-in call that evicts without the refill gate. Any of these would do; the first is the smallest:

1. `ratelimit_evict_stale_force(rl, max_idle_ns)` — today's walk, minus the `:253` projection check.
2. `ratelimit_evict_to(rl, max_keys)` — evicts the least-recently-seen buckets until
   `ratelimit_key_count(rl) <= max_keys`, regardless of fill. It bounds the map by count directly.
3. A limiter-side cap, `ratelimit_with_max_keys(rate_x1000, burst, max_keys)`, enforced on insert.

Keep `ratelimit_evict_stale` as it is. On adoption, agnosai will call the new function from its
pressure sweep only (`src/server/rate_limit.cyr:409`).

## 4. Verification a fix should carry

- The repro above: `evicted == 20000`, `after == 0` through the new call; `ratelimit_evict_stale(rl, 0)`
  still evicts 0 (the 2.8.1 guarantee is unchanged).
- A drained bucket evicted by force and then re-checked gets a fresh burst. That is expected, but it
  should be documented on the new call so nobody reaches for it on the idle path.
