# Benchmarks

Three-point trend tracking: baseline / previous / latest.

Full history in [`bench-history.csv`](bench-history.csv). Run with `make bench` or `./scripts/bench-history.sh`.

## Three-Tier Pub/Sub Throughput

| Channel | Latency | Throughput | Use case |
|---------|---------|-----------|----------|
| `DirectChannel<T>` | **13.6 ns** | **~73M msg/s** | Raw broadcast, no routing |
| `HashedChannel<T>` | **62 ns** | **~16M msg/s** | Hashed topic + coarse timestamp |
| `TypedPubSub<T>` exact | **896 ns** | **~1.1M msg/s** | Full features, O(1) exact topic |
| `TypedPubSub<T>` wildcard | **931 ns** | **~1.07M msg/s** | MQTT wildcards, filters, replay |
| `TypedPubSub<T>` 100 exact subs | **902 ns** | **~1.1M msg/s** | Scales flat — O(1) lookup |
| `TypedPubSub<T>` 100 wild subs | **6,368 ns** | **~157K msg/s** | O(n) pattern iteration |

## Benchmark Suites (6 files, 30+ benchmarks)

### pubsub (`benches/pubsub.rs`)
| Benchmark | Latest |
|-----------|--------|
| `direct_channel_publish_1sub` | 13.6 ns |
| `direct_channel_publish_10sub` | 13.7 ns |
| `direct_channel_1000_publish` | 12.3 us |
| `hashed_channel_publish_1sub` | 62.1 ns |
| `hashed_channel_10topics_1match` | 55.1 ns |
| `hashed_channel_1000_publish` | 63.7 us |
| `typed_exact_topic_publish` | 896 ns |
| `typed_wildcard_topic_publish` | 931 ns |
| `typed_exact_100subs_1match` | 902 ns |
| `typed_wild_100subs_publish` | 6,368 ns |
| `typed_mixed_10exact_10wild` | 1,459 ns |
| `matches_pattern exact` | 54 ns |
| `matches_pattern wildcard *` | 50 ns |
| `matches_pattern #` | 57 ns |

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
| `envelope_new` | 226 ns |
| `envelope_serialize_roundtrip` | — |
| `ipc_roundtrip_small_payload` | — |
| `ipc_roundtrip_large_payload` | — |

### live_io (`benches/live_io.rs`)
| Benchmark | Description |
|-----------|-------------|
| Real Unix socket IPC send/recv | Measures actual I/O latency |

## Notes

- **DirectChannel** scales flat with subscriber count (13.6ns at 1 and 10 subscribers)
- **HashedChannel** uses `Instant::elapsed()` (monotonic, no syscall) instead of `Utc::now()`
- **TypedPubSub dual-pipe**: exact-topic subscribers use O(1) DashMap lookup; only wildcard patterns iterate
- Auto-cleanup on publish adds ~0.5 us per 1,000th publish (amortised: negligible)
- Relay dedup tracks timestamps — no measurable overhead on receive path
- ConnectionPool circuit breaker adds one DashMap lookup per acquire (~10ns overhead)
- All benchmarks run with `--all-features`
