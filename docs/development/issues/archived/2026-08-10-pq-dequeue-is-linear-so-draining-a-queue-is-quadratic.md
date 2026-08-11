# `pq_dequeue` is O(n) per pop, so draining a priority queue is O(n²) — RESOLVED

**Status:** ✅ **RESOLVED in majra 2.6.2** (2026-08-11). Each tier gained a read index;
a pop advances it instead of moving the survivors, and the consumed prefix is reclaimed
once the head passes the midpoint — amortised O(1). Measured after the fix, the same
drain that produced the table below:

| depth | before | after |
|---|---|---|
| 2,000 | 2.00 µs | **34 ns** |
| 16,000 | 15.56 µs | **34 ns** |
| 200,000 | 198.70 µs | **33 ns** |

Flat across a 100x range. `PriorityQueue` went 48 → 88 bytes; the tier pointers and the
total keep their offsets, so `pq_len`/`pq_is_empty` were untouched.

**Swap-with-last was not used** — the issue text below rules it out for destroying FIFO
within a tier, and that reasoning held.

Guarded by `test_queue_deep_fifo_and_compaction` in `tests/test_core.tcyr`, which asserts
the **backing vec length** as well as ordering: deleting the compaction leaves every
ordering assertion passing while the tier grows without bound, so an order-only test
cannot see the defect. Both mutants (compaction removed; head not reset) fail the suite.

⚠ A second, unrelated bug in the same file was found and fixed in the same release:
`pq_enqueue` clamped over-range priorities but not **negative** ones, so a `-1` reached
`load64(pq + pri * 8)` — an out-of-bounds read one slot before the struct, followed by a
`vec_push` through the result. Without the guard the test suite does not fail an
assertion, it **dies mid-test**.

---

**Original report follows.**

**Status when filed:** 🔴 OPEN — filed from a consumer (agnosai), measured, not inferred.
**Discovered:** 2026-08-10, benchmarking agnosai's new `llm/inference_queue` against
majra 2.6.1 on cyrius 6.5.18.
**Severity:** Medium-High. Correct, but it degrades exactly where the queue is supposed to
earn its keep — under a backlog. A queue that never builds one never notices.

## The defect

`pq_dequeue` (`src/queue.cyr`) takes the head of a tier with

```cyr
var item = vec_get(tier, 0);
vec_remove(tier, 0);
```

`vec_remove(tier, 0)` shifts every remaining element down one slot. So a pop from a tier
holding *n* items costs O(n), and draining the tier costs **O(n²)**.

## Measured

A tier is filled with *n* items, then fully drained; the figure is the mean cost of one
`pq_dequeue`:

| tier depth | ns per pop | ratio vs previous |
|---|---|---|
| 2,000 | 2.00 µs | — |
| 4,000 | 4.02 µs | **2.01×** |
| 8,000 | 7.92 µs | **1.97×** |
| 16,000 | 15.56 µs | **1.96×** |

Doubling the depth doubles the per-pop cost. That is the signature of a linear pop, and it
rules out cache effects or allocator noise as the explanation.

At the depth agnosai's benchmark originally used — 200,000 items in one tier — a single pop
averaged **198.7 µs** and the full drain took roughly **40 seconds of memmove**.

## Why it matters more than a microbenchmark usually would

`ConcurrentPriorityQueue` is the backing store for priority work-shedding: agnosai's
`llm/inference_queue` puts background summarization behind interactive crew tasks. The whole
point is that the low-priority tier is *allowed* to accumulate. So the deep-tier case is not
the pathological case — it is the design case, and it is the one that degrades.

It also compounds with the tier scan: `pq_dequeue` walks all five tiers looking for the
first non-empty one, so a backlog parked at `PRIORITY_BACKGROUND` pays the scan *and* the
worst memmove.

## Expected

Any of, in preference order:

1. **A head index per tier.** Keep the vec, add a `head` offset, pop with
   `vec_get(tier, head)` and `head = head + 1`, compacting only when `head` exceeds some
   fraction of the length. O(1) amortised, no new data structure, and it preserves FIFO
   within a tier.
2. **A ring buffer per tier**, which is the same idea with a bound.
3. At minimum, **document the cost** on `pq_dequeue` and `cpq_dequeue`, so a consumer sizing
   a backlog knows what it is buying.

⚠ **Swap-with-last is not a valid fix here.** It is the obvious O(1) trick and it destroys
FIFO ordering within a priority level, which `queue_item_new`'s monotonic id and the
oracle's `ManagedQueue` semantics both rely on.

## Repro

```cyr
fn drain(n) {
    var pq = pq_new();
    for (var i = 0; i < n; i = i + 1) { pq_enqueue(pq, queue_item_new(PRIORITY_NORMAL, i)); }
    var b = bench_new("drain");
    bench_batch_start(b);
    for (var j = 0; j < n; j = j + 1) { pq_dequeue(pq); }
    bench_batch_stop(b, n);
    bench_report(b);
}
```

Run it at 2k / 4k / 8k / 16k and read the ratios.

## Consumer-side note

agnosai's `benches/llm.bcyr` now measures the drain at **two** depths rather than one, so
the quadratic stays visible in its bench history instead of being averaged into a single
uninterpretable figure. If this is fixed upstream, those two numbers converge — which is a
nicer regression signal than either number alone.
