#![no_main]

use libfuzzer_sys::fuzz_target;
use majra::queue::{Priority, PriorityQueue, QueueItem};

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    let mut q = PriorityQueue::new();

    for chunk in data.chunks(2) {
        let priority = match chunk[0] % 5 {
            0 => Priority::Background,
            1 => Priority::Low,
            2 => Priority::Normal,
            3 => Priority::High,
            _ => Priority::Critical,
        };
        let payload = chunk.get(1).copied().unwrap_or(0);
        q.enqueue(QueueItem::new(priority, payload));
    }

    // Drain the queue — should always succeed without panic.
    while q.dequeue().is_some() {}
    assert!(q.is_empty());
});
