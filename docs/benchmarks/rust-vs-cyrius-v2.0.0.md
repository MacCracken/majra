# Benchmarks

Cyrius v2.0.0 vs Rust v1.0.4 — comparable operations measured on the same hardware.

## Head-to-Head Comparison

| Operation | Rust (Criterion) | Cyrius (bench.cyr) | Delta |
|-----------|-----------------|-------------------|-------|
| `envelope_new` | 226 ns | 1,000 ns | 4.4x slower |
| `pattern_exact` | 54 ns | 139 ns | 2.6x slower |
| `pattern_wildcard_+` | 50 ns | 115 ns | 2.3x slower |
| `pattern_wildcard_#` | 57 ns | 97 ns | 1.7x slower |
| `direct_channel_send` | 13.6 ns | 380 ns | 28x slower |
| `pubsub_publish (no match)` | 790 ns | 211 ns | 3.7x faster |
| `pq_enqueue` | 53 ns (1000-batch) | 612 ns | ~11x slower |

### Analysis

- **Pattern matching** (2-3x) — Cyrius byte-by-byte `load8` vs Rust SIMD-friendly `str` iteration. Expected gap for a single-pass compiler without optimization passes.
- **Envelope creation** (4.4x) — UUID generation via `getrandom` syscall (318) vs Rust's optimized `uuid::Uuid::new_v4()`. Syscall overhead dominates.
- **Direct channel** (28x) — Rust's `broadcast::Sender` is a lock-free ring buffer; Cyrius uses futex-based MPSC channel with mutex. Lock contention on hot path.
- **Pubsub no-match** (3.7x faster) — Cyrius hashmap lookup is cheaper when there are no subscribers (empty map fast-path). Rust's `DashMap` has shard overhead even when empty.
- **Priority queue** (11x) — Cyrius `fl_alloc` per item + mutex lock vs Rust `VecDeque::push_back` (amortized O(1), no allocation per item).

### Where Cyrius wins

- Zero startup time (no runtime initialization, no tokio scheduler)
- 93 KB binary vs multi-MB Rust release binary
- No dependency tree to audit (0 vs 25 crates)

### Where Rust wins

- Lock-free data structures (DashMap, broadcast channel)
- LLVM optimization passes (inlining, vectorization, constant folding)
- Amortized allocation (Vec/VecDeque arena vs per-item fl_alloc)

## Full Cyrius Benchmark Results

| Benchmark | Avg | Iterations |
|-----------|-----|-----------|
| `envelope_new` | 1 us | 10,000 |
| `pq_enqueue` | 612 ns | 10,000 |
| `pq_dequeue` | 15 us | 10,000 |
| `pattern_exact` | 139 ns | 10,000 |
| `pattern_wildcard_+` | 115 ns | 10,000 |
| `pattern_wildcard_#` | 97 ns | 10,000 |
| `pattern_no_match` | 80 ns | 10,000 |
| `pubsub_publish_nosub` | 211 ns | 10,000 |
| `pubsub_1sub_publish` | 1 us | 60 |
| `direct_channel_send` | 380 ns | 10,000 |
| `heartbeat_100nodes` | 461 ns | 1,000 |
| `fleet_stats_100` | 9 us | 100 |
| `ratelimit_check` | 570 ns | 10,000 |
| `relay_send` | 633 ns | 1,000 |
| `barrier_cycle` | 3 us | 1,000 |
| `circuit_state` | 3 ns | 10,000 |
| `counter_inc` | 8 ns | 10,000 |

## Rust v1.0.4 Reference (from bench-history.csv)

| Benchmark | Latency |
|-----------|---------|
| `direct_channel_publish_1sub` | 13.6 ns |
| `hashed_channel_publish_1sub` | 62 ns |
| `typed_exact_topic_publish` | 896 ns |
| `typed_wildcard_topic_publish` | 931 ns |
| `matches_pattern exact` | 54 ns |
| `matches_pattern wildcard *` | 50 ns |
| `matches_pattern #` | 57 ns |
| `envelope_new` | 226 ns |
| `pubsub_publish_no_match` | 790 ns |
| `enqueue+dequeue 1000 items` | 52.95 us |

Run benchmarks: `cyrius bench`
