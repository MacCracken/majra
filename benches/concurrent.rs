use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;

use majra::queue::{ConcurrentPriorityQueue, ManagedQueue, ManagedQueueConfig, Priority, QueueItem, ResourcePool};

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
                            q2.enqueue(QueueItem::new(Priority::Normal, t * 250 + i)).await;
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
                    ids.push(q.enqueue(Priority::Normal, "payload".to_string(), None).await);
                }
                for _ in 0..100 {
                    if let Some(item) = q.dequeue(&pool).await {
                        q.complete(item.id).unwrap();
                    }
                }
            });
        });
    });
}

fn bench_heartbeat(c: &mut Criterion) {
    use majra::heartbeat::{ConcurrentHeartbeatTracker, HeartbeatConfig};

    c.bench_function("concurrent_heartbeat_100_nodes", |b| {
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
}

criterion_group!(
    benches,
    bench_concurrent_priority_queue,
    bench_managed_queue,
    bench_heartbeat,
    bench_ratelimit,
);
criterion_main!(benches);
