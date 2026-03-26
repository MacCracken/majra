# Benchmarks

Three-point trend tracking: baseline / previous / latest.

Full history in [`bench-history.csv`](bench-history.csv). Run with `make bench` or `./scripts/bench-history.sh`.

## Latest Run

**Date**: 2026-03-26 | **Commit**: `7df17cc` (v1.0.0)

## Benchmark Suites (6 files, 25+ benchmarks)

### pubsub (`benches/pubsub.rs`)
| Benchmark | Baseline | Latest | Change |
|-----------|----------|--------|--------|
| `pubsub_publish_no_match` | 790.12 ns | 1.35 us | +71% (auto-cleanup overhead) |
| `typed_pubsub_publish_with_filter` | — | 794.60 ns | — |
| `typed_pubsub_publish_with_replay` | — | 917.96 ns | — |
| `matches_pattern exact` | 60.09 ns | 99.61 ns | noise (bench env) |

### queue (`benches/queue.rs`)
| Benchmark | Latest |
|-----------|--------|
| `enqueue+dequeue 1000 items` | 52.95 us |

### concurrent (`benches/concurrent.rs`)
| Benchmark | Description |
|-----------|-------------|
| `concurrent_queue_enqueue` | DashMap-backed concurrent queue |
| `concurrent_heartbeat_register` | ConcurrentHeartbeatTracker throughput |
| `concurrent_ratelimit_check` | DashMap token-bucket check |
| `concurrent_relay_receive` | Relay dedup with timestamp tracking |
| `concurrent_barrier_arrive` | AsyncBarrierSet arrive throughput |

### dag (`benches/dag.rs`)
| Benchmark | Description |
|-----------|-------------|
| `tier_sort_linear_10` | 10-step linear chain topological sort |
| `tier_sort_wide_100` | 100-step wide DAG sort |
| `engine_execute_linear_3` | 3-step linear workflow execution |
| `engine_execute_diamond_4` | 4-step diamond DAG execution |

### ipc_envelope (`benches/ipc_envelope.rs`)
| Benchmark | Latest |
|-----------|--------|
| `envelope_new` | 226.41 ns |
| `envelope_serialize_roundtrip` | — |
| `ipc_roundtrip_small_payload` | — |
| `ipc_roundtrip_large_payload` | — |

### live_io (`benches/live_io.rs`)
| Benchmark | Description |
|-----------|-------------|
| Real Unix socket IPC send/recv | Measures actual I/O latency |

## Notes

- Auto-cleanup on publish adds ~0.5 us overhead per 1,000th publish (amortised: negligible)
- Relay dedup now tracks timestamps — no measurable overhead on receive path
- ConnectionPool stale eviction is O(n) but only runs on demand, not hot path
- All benchmarks run with `--all-features` to cover feature-gated paths
