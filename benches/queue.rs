use criterion::{Criterion, criterion_group, criterion_main};
use majra::queue::{Priority, PriorityQueue, QueueItem};

fn bench_enqueue_dequeue(c: &mut Criterion) {
    c.bench_function("enqueue+dequeue 1000 items", |b| {
        b.iter(|| {
            let mut q = PriorityQueue::new();
            for i in 0..1000u32 {
                let prio = match i % 5 {
                    0 => Priority::Background,
                    1 => Priority::Low,
                    2 => Priority::Normal,
                    3 => Priority::High,
                    _ => Priority::Critical,
                };
                q.enqueue(QueueItem::new(prio, i));
            }
            while q.dequeue().is_some() {}
        })
    });
}

criterion_group!(benches, bench_enqueue_dequeue);
criterion_main!(benches);
