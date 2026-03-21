//! Example: Using ManagedQueue for GPU-aware job scheduling.
//!
//! Demonstrates resource-filtered dequeue, max concurrency enforcement,
//! lifecycle state management, and event subscriptions.

use majra::queue::{
    JobState, ManagedQueue, ManagedQueueConfig, Priority, ResourcePool, ResourceReq,
};
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Configure: max 2 concurrent jobs, evict finished jobs after 60s.
    let config = ManagedQueueConfig {
        max_concurrency: 2,
        finished_ttl: Duration::from_secs(60),
    };
    let queue = ManagedQueue::new(config);

    // Subscribe to events.
    let mut events = queue.subscribe_events();

    // Enqueue jobs with different priorities and resource requirements.
    let big_job = queue
        .enqueue(
            Priority::High,
            "train-llama-70b".to_string(),
            Some(ResourceReq {
                gpu_count: 4,
                vram_mb: 80_000,
            }),
        )
        .await;

    let small_job = queue
        .enqueue(
            Priority::Normal,
            "train-phi-3".to_string(),
            Some(ResourceReq {
                gpu_count: 1,
                vram_mb: 8_000,
            }),
        )
        .await;

    let no_gpu_job = queue
        .enqueue(Priority::Low, "index-dataset".to_string(), None)
        .await;

    println!("Queued {} jobs", queue.queued_count().await);

    // A small GPU pool — can only run the small job and no-gpu job.
    let pool = ResourcePool {
        gpu_count: 1,
        vram_mb: 16_000,
    };

    // Dequeue what fits.
    if let Some(item) = queue.dequeue(&pool).await {
        println!("Started: {} ({})", item.payload, item.id);
        assert_eq!(item.state, JobState::Running);
    }

    if let Some(item) = queue.dequeue(&pool).await {
        println!("Started: {} ({})", item.payload, item.id);
    }

    // Third dequeue blocked by max_concurrency=2.
    assert!(queue.dequeue(&pool).await.is_none());
    println!("Max concurrency reached ({} running)", queue.running_count());

    // Complete one job.
    queue.complete(small_job).unwrap();
    println!("Completed small_job");

    // Now we can dequeue again (but big_job needs 4 GPUs, won't fit).
    // The no-gpu job should be dequeued instead.
    if let Some(item) = queue.dequeue(&pool).await {
        println!("Started: {} ({})", item.payload, item.id);
        queue.complete(item.id).unwrap();
    }

    // Drain events.
    while let Ok(event) = events.try_recv() {
        println!("Event: {event:?}");
    }

    // Cancel the big job that never got resources.
    queue.cancel(big_job).await.unwrap();
    println!("Cancelled big_job");

    println!(
        "Final state: {} queued, {} running, {} total tracked",
        queue.queued_count().await,
        queue.running_count(),
        queue.job_count(),
    );
}
