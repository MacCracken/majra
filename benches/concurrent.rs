use criterion::{Criterion, criterion_group, criterion_main};
use std::collections::HashSet;
use std::sync::Arc;

use majra::queue::{
    ConcurrentPriorityQueue, ManagedQueue, ManagedQueueConfig, Priority, QueueItem, ResourcePool,
    ResourceReq,
};

fn bench_concurrent_priority_queue(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("concurrent_queue_enqueue_dequeue_1000", |b| {
        b.iter(|| {
            rt.block_on(async {
                let q = ConcurrentPriorityQueue::new();
                for i in 0..1000 {
                    let p = match i % 5 {
                        0 => Priority::Background,
                        1 => Priority::Low,
                        2 => Priority::Normal,
                        3 => Priority::High,
                        _ => Priority::Critical,
                    };
                    q.enqueue(QueueItem::new(p, i)).await;
                }
                while q.dequeue().await.is_some() {}
            });
        });
    });

    c.bench_function("concurrent_queue_contended_4_threads", |b| {
        b.iter(|| {
            rt.block_on(async {
                let q = Arc::new(ConcurrentPriorityQueue::new());
                let mut handles = Vec::new();
                for t in 0..4 {
                    let q2 = q.clone();
                    handles.push(tokio::spawn(async move {
                        for i in 0..250 {
                            q2.enqueue(QueueItem::new(Priority::Normal, t * 250 + i))
                                .await;
                        }
                    }));
                }
                for h in handles {
                    h.await.unwrap();
                }
                while q.dequeue().await.is_some() {}
            });
        });
    });
}

fn bench_managed_queue(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("managed_queue_enqueue_dequeue_100", |b| {
        b.iter(|| {
            rt.block_on(async {
                let q = ManagedQueue::new(ManagedQueueConfig::default());
                let pool = ResourcePool {
                    gpu_count: u32::MAX,
                    vram_mb: u64::MAX,
                };
                let mut ids = Vec::new();
                for _ in 0..100 {
                    ids.push(
                        q.enqueue(Priority::Normal, "payload".to_string(), None)
                            .await,
                    );
                }
                for _ in 0..100 {
                    if let Some(item) = q.dequeue(&pool).await {
                        q.complete(item.id).unwrap();
                    }
                }
            });
        });
    });

    c.bench_function("managed_queue_resource_filtered_50", |b| {
        b.iter(|| {
            rt.block_on(async {
                let q = ManagedQueue::new(ManagedQueueConfig::default());
                for i in 0..50 {
                    let req = if i % 2 == 0 {
                        Some(ResourceReq {
                            gpu_count: 4,
                            vram_mb: 32000,
                        })
                    } else {
                        Some(ResourceReq {
                            gpu_count: 1,
                            vram_mb: 8000,
                        })
                    };
                    q.enqueue(Priority::Normal, format!("job-{i}"), req).await;
                }
                // Small pool — can only dequeue the small jobs.
                let pool = ResourcePool {
                    gpu_count: 1,
                    vram_mb: 8000,
                };
                while let Some(item) = q.dequeue(&pool).await {
                    q.complete(item.id).unwrap();
                }
            });
        });
    });
}

fn bench_heartbeat(c: &mut Criterion) {
    use majra::heartbeat::{ConcurrentHeartbeatTracker, GpuTelemetry, HeartbeatConfig};

    c.bench_function("heartbeat_100_nodes_beat_sweep", |b| {
        let tracker = ConcurrentHeartbeatTracker::new(HeartbeatConfig::default());
        for i in 0..100 {
            tracker.register(format!("node-{i}"), serde_json::json!({}));
        }
        b.iter(|| {
            for i in 0..100 {
                tracker.heartbeat(&format!("node-{i}"));
            }
            tracker.update_statuses();
        });
    });

    c.bench_function("heartbeat_telemetry_update_50", |b| {
        let tracker = ConcurrentHeartbeatTracker::new(HeartbeatConfig::default());
        for i in 0..50 {
            tracker.register(format!("gpu-{i}"), serde_json::json!({}));
        }
        let telemetry = vec![GpuTelemetry {
            utilization_pct: 75.0,
            memory_used_mb: 6000,
            memory_total_mb: 8000,
            temperature_c: Some(65.0),
        }];
        b.iter(|| {
            for i in 0..50 {
                tracker.heartbeat_with_telemetry(&format!("gpu-{i}"), telemetry.clone());
            }
        });
    });

    c.bench_function("heartbeat_fleet_stats_100", |b| {
        let tracker = ConcurrentHeartbeatTracker::new(HeartbeatConfig::default());
        for i in 0..100 {
            tracker.register_with_telemetry(
                format!("node-{i}"),
                serde_json::json!({}),
                vec![GpuTelemetry {
                    utilization_pct: 50.0,
                    memory_used_mb: 4000,
                    memory_total_mb: 8000,
                    temperature_c: None,
                }],
            );
        }
        b.iter(|| tracker.fleet_stats());
    });
}

fn bench_ratelimit(c: &mut Criterion) {
    use majra::ratelimit::RateLimiter;

    c.bench_function("ratelimit_check_1000", |b| {
        let limiter = RateLimiter::new(1000.0, 100);
        b.iter(|| {
            for i in 0..1000 {
                limiter.check(&format!("key-{}", i % 10));
            }
        });
    });

    c.bench_function("ratelimit_check_single_key_1000", |b| {
        let limiter = RateLimiter::new(100000.0, 10000);
        b.iter(|| {
            for _ in 0..1000 {
                limiter.check("hot-key");
            }
        });
    });
}

fn bench_relay(c: &mut Criterion) {
    use majra::relay::{Relay, RelayMessage};

    c.bench_function("relay_send_1000", |b| {
        let relay = Relay::new("node-1");
        b.iter(|| {
            for _ in 0..1000 {
                relay.send("node-2", "data", serde_json::json!({"v": 1}));
            }
        });
    });

    c.bench_function("relay_receive_dedup_1000", |b| {
        let relay = Relay::new("receiver");
        b.iter(|| {
            for seq in 1..=1000u64 {
                let msg = RelayMessage {
                    seq,
                    from: "sender".into(),
                    to: "receiver".into(),
                    topic: "data".into(),
                    payload: serde_json::Value::Null,
                    timestamp: chrono::Utc::now(),
                    correlation_id: None,
                    is_reply: false,
                };
                relay.receive(msg);
            }
        });
    });
}

fn bench_barrier(c: &mut Criterion) {
    use majra::barrier::AsyncBarrierSet;

    c.bench_function("barrier_arrive_release_10", |b| {
        b.iter(|| {
            let set = AsyncBarrierSet::new();
            let participants: HashSet<String> = (0..10).map(|i| format!("p-{i}")).collect();
            set.create("sync", participants);
            for i in 0..10 {
                let _ = set.arrive("sync", &format!("p-{i}"));
            }
        });
    });

    c.bench_function("barrier_create_arrive_complete_cycle", |b| {
        b.iter(|| {
            let set = AsyncBarrierSet::new();
            for round in 0..20 {
                let name = format!("barrier-{round}");
                let participants: HashSet<String> = (0..4).map(|i| format!("w-{i}")).collect();
                set.create(&name, participants);
                for i in 0..4 {
                    let _ = set.arrive(&name, &format!("w-{i}"));
                }
                set.complete(&name);
            }
        });
    });
}

criterion_group!(
    benches,
    bench_concurrent_priority_queue,
    bench_managed_queue,
    bench_heartbeat,
    bench_ratelimit,
    bench_relay,
    bench_barrier,
);
criterion_main!(benches);
