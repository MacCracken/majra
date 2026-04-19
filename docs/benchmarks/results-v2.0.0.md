# Benchmarks

Run: `cyrius bench`

## Results (v2.0.0)

| Benchmark | Avg | Iterations |
|-----------|-----|-----------|
| `circuit_state` | 3 ns | 10,000 |
| `counter_inc` | 8 ns | 10,000 |
| `pattern_no_match` | 80 ns | 10,000 |
| `pattern_wildcard_#` | 97 ns | 10,000 |
| `pattern_wildcard_+` | 115 ns | 10,000 |
| `pattern_exact` | 139 ns | 10,000 |
| `pubsub_publish_nosub` | 211 ns | 10,000 |
| `direct_channel_send` | 380 ns | 10,000 |
| `heartbeat_100nodes` | 461 ns | 1,000 |
| `ratelimit_check` | 570 ns | 10,000 |
| `pq_enqueue` | 612 ns | 10,000 |
| `relay_send` | 633 ns | 1,000 |
| `pubsub_1sub_publish` | 1 us | 60 |
| `envelope_new` | 1 us | 10,000 |
| `barrier_cycle` | 3 us | 1,000 |
| `fleet_stats_100` | 9 us | 100 |
| `pq_dequeue` | 15 us | 10,000 |

## History

Tracked automatically by `cyrius bench`. See `bench-history.csv` for trends.

## Comparison

See [benchmark-rustvcyrius2.md](benchmark-rustvcyrius2.md) for Rust v1.0.4 vs Cyrius v2.0.0 comparison.
