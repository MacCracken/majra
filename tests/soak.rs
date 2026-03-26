#![allow(unused_imports, dead_code)]
//! Load and soak tests for majra primitives.
//!
//! These tests run sustained workloads and report latency percentiles and
//! memory growth. They are ignored by default — run with:
//!
//! ```sh
//! cargo test --all-features --test soak -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

/// Collect latencies and report percentiles.
struct LatencyHistogram {
    samples: Vec<Duration>,
}

impl LatencyHistogram {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(100_000),
        }
    }

    fn record(&mut self, d: Duration) {
        self.samples.push(d);
    }

    fn report(&mut self, label: &str) {
        self.samples.sort();
        let n = self.samples.len();
        if n == 0 {
            println!("[{label}] no samples");
            return;
        }
        let p50 = self.samples[n * 50 / 100];
        let p95 = self.samples[n * 95 / 100];
        let p99 = self.samples[n * 99 / 100];
        let max = self.samples[n - 1];
        let total: Duration = self.samples.iter().sum();
        let avg = total / n as u32;
        let throughput = n as f64 / total.as_secs_f64();

        println!("[{label}] n={n}  throughput={throughput:.0} ops/s");
        println!("  avg={avg:?}  p50={p50:?}  p95={p95:?}  p99={p99:?}  max={max:?}");
    }
}

/// Get current process RSS in bytes (Linux only, returns 0 elsewhere).
fn rss_bytes() -> usize {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/self/statm")
            .ok()
            .and_then(|s| s.split_whitespace().nth(1)?.parse::<usize>().ok())
            .map(|pages| pages * 4096)
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

fn fmt_bytes(b: usize) -> String {
    if b == 0 {
        "N/A".into()
    } else if b < 1024 * 1024 {
        format!("{:.1} KB", b as f64 / 1024.0)
    } else {
        format!("{:.1} MB", b as f64 / (1024.0 * 1024.0))
    }
}

// ---------------------------------------------------------------------------
// Managed queue soak — sustained enqueue/dequeue with state transitions
// ---------------------------------------------------------------------------

#[cfg(feature = "queue")]
#[tokio::test]
#[ignore]
async fn soak_managed_queue() {
    use majra::queue::{ManagedQueue, ManagedQueueConfig, Priority, ResourcePool};

    const OPS: usize = 50_000;

    let config = ManagedQueueConfig {
        max_concurrency: 100,
        finished_ttl: Duration::from_secs(60),
    };
    let queue = Arc::new(ManagedQueue::<serde_json::Value>::new(config));

    let rss_before = rss_bytes();

    // Enqueue phase.
    let mut enqueue_hist = LatencyHistogram::new();
    let mut ids = Vec::with_capacity(OPS);

    for i in 0..OPS {
        let priority = match i % 5 {
            0 => Priority::Critical,
            1 => Priority::High,
            2 => Priority::Normal,
            3 => Priority::Low,
            _ => Priority::Background,
        };
        let start = Instant::now();
        let id = queue
            .enqueue(priority, serde_json::json!({"job": i}), None)
            .await;
        enqueue_hist.record(start.elapsed());
        ids.push(id);
    }
    enqueue_hist.report("enqueue");

    // Dequeue phase.
    let pool = ResourcePool {
        gpu_count: u32::MAX,
        vram_mb: u64::MAX,
    };
    let mut dequeue_hist = LatencyHistogram::new();

    for _ in 0..OPS {
        let start = Instant::now();
        let item = queue.dequeue(&pool).await.expect("should dequeue");
        dequeue_hist.record(start.elapsed());
        queue.complete(item.id).unwrap();
    }
    dequeue_hist.report("dequeue+complete");

    // Verify all completed.
    assert_eq!(queue.running_count(), 0);
    let queued = queue.queued_count().await;
    assert_eq!(queued, 0, "expected 0 queued, got {queued}");

    let rss_after = rss_bytes();
    println!(
        "[memory] before={} after={} delta={}",
        fmt_bytes(rss_before),
        fmt_bytes(rss_after),
        fmt_bytes(rss_after.saturating_sub(rss_before))
    );
}

// ---------------------------------------------------------------------------
// PubSub soak — sustained publish with multiple subscribers
// ---------------------------------------------------------------------------

#[cfg(feature = "pubsub")]
#[tokio::test]
#[ignore]
async fn soak_pubsub_throughput() {
    use majra::pubsub::PubSub;

    const MESSAGES: usize = 100_000;
    const SUBSCRIBERS: usize = 10;

    let hub = Arc::new(PubSub::new());

    // Create subscribers.
    let mut receivers = Vec::with_capacity(SUBSCRIBERS);
    for _ in 0..SUBSCRIBERS {
        receivers.push(hub.subscribe("events/#"));
    }

    let rss_before = rss_bytes();

    let mut publish_hist = LatencyHistogram::new();
    for i in 0..MESSAGES {
        let start = Instant::now();
        hub.publish(
            &format!("events/node-{}/status", i % 100),
            serde_json::json!({"seq": i, "status": "ok"}),
        );
        publish_hist.record(start.elapsed());
    }
    publish_hist.report("publish (10 subs)");

    // Drain one receiver to check delivery. Broadcast channels drop oldest
    // when lagging, so we count both received and lagged messages.
    let mut received = 0usize;
    let mut lagged = 0usize;
    loop {
        match receivers[0].try_recv() {
            Ok(_) => received += 1,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                lagged += n as usize;
            }
            Err(_) => break,
        }
    }
    let total_delivery = received + lagged;
    assert!(
        total_delivery > 0,
        "expected message delivery activity, got 0"
    );
    println!(
        "[pubsub] receiver[0] received={received} lagged={lagged} total={total_delivery}/{MESSAGES}"
    );

    let rss_after = rss_bytes();
    println!(
        "[memory] before={} after={} delta={}",
        fmt_bytes(rss_before),
        fmt_bytes(rss_after),
        fmt_bytes(rss_after.saturating_sub(rss_before))
    );
}

// ---------------------------------------------------------------------------
// Typed PubSub soak — filtered delivery at scale
// ---------------------------------------------------------------------------

#[cfg(feature = "pubsub")]
#[tokio::test]
#[ignore]
async fn soak_typed_pubsub_filtered() {
    use majra::pubsub::TypedPubSub;

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    struct Event {
        node: String,
        value: i64,
    }

    const MESSAGES: usize = 100_000;

    let hub = TypedPubSub::<Event>::new();

    // Unfiltered subscriber.
    let mut rx_all = hub.subscribe("events/#");
    // Filtered: only high values.
    let mut rx_high = hub.subscribe_filtered("events/#", |e: &Event| e.value > 500);

    let mut hist = LatencyHistogram::new();
    for i in 0..MESSAGES {
        let start = Instant::now();
        hub.publish(
            &format!("events/node-{}", i % 50),
            Event {
                node: format!("node-{}", i % 50),
                value: (i % 1000) as i64,
            },
        );
        hist.record(start.elapsed());
    }
    hist.report("typed publish (1 unfiltered + 1 filtered sub)");

    let mut all_received = 0usize;
    let mut all_lagged = 0usize;
    loop {
        match rx_all.try_recv() {
            Ok(_) => all_received += 1,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                all_lagged += n as usize;
            }
            Err(_) => break,
        }
    }

    let mut high_received = 0usize;
    let mut high_lagged = 0usize;
    loop {
        match rx_high.try_recv() {
            Ok(_) => high_received += 1,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                high_lagged += n as usize;
            }
            Err(_) => break,
        }
    }

    println!(
        "[typed pubsub] all: received={all_received} lagged={all_lagged}  high: received={high_received} lagged={high_lagged}"
    );
    assert!(
        all_received + all_lagged > 0,
        "expected delivery activity on all_sub"
    );
    // With filtering, the high subscriber should see fewer messages.
    assert!(
        high_received + high_lagged <= all_received + all_lagged,
        "filter should not increase delivery count"
    );
}

// ---------------------------------------------------------------------------
// Rate limiter soak — sustained check() throughput under contention
// ---------------------------------------------------------------------------

#[cfg(feature = "ratelimit")]
#[test]
#[ignore]
fn soak_rate_limiter_contention() {
    use majra::ratelimit::RateLimiter;

    const KEYS: usize = 1000;
    const CHECKS_PER_KEY: usize = 100;
    const THREADS: usize = 8;

    let limiter = Arc::new(RateLimiter::new(1000.0, 50));

    let rss_before = rss_bytes();
    let start = Instant::now();

    let mut handles = Vec::new();
    for t in 0..THREADS {
        let l = limiter.clone();
        handles.push(std::thread::spawn(move || {
            let mut hist = LatencyHistogram::new();
            let key_start = t * KEYS / THREADS;
            let key_end = (t + 1) * KEYS / THREADS;
            for key_idx in key_start..key_end {
                let key = format!("key-{key_idx}");
                for _ in 0..CHECKS_PER_KEY {
                    let s = Instant::now();
                    let _ = l.check(&key);
                    hist.record(s.elapsed());
                }
            }
            hist
        }));
    }

    let mut combined = LatencyHistogram::new();
    for h in handles {
        let hist = h.join().unwrap();
        combined.samples.extend(hist.samples);
    }
    combined.report(&format!(
        "rate_limiter ({THREADS} threads, {KEYS} keys, {CHECKS_PER_KEY} checks/key)"
    ));

    let elapsed = start.elapsed();
    println!("[total] {} ops in {:?}", KEYS * CHECKS_PER_KEY, elapsed);

    let rss_after = rss_bytes();
    println!(
        "[memory] before={} after={} delta={}",
        fmt_bytes(rss_before),
        fmt_bytes(rss_after),
        fmt_bytes(rss_after.saturating_sub(rss_before))
    );

    // Verify stats coherence.
    let stats = limiter.stats();
    assert_eq!(
        stats.total_allowed + stats.total_rejected,
        (KEYS * CHECKS_PER_KEY) as u64,
        "allowed + rejected should equal total checks"
    );
}

// ---------------------------------------------------------------------------
// Heartbeat soak — high-frequency heartbeats under contention
// ---------------------------------------------------------------------------

#[cfg(feature = "heartbeat")]
#[test]
#[ignore]
fn soak_heartbeat_contention() {
    use majra::heartbeat::{ConcurrentHeartbeatTracker, HeartbeatConfig};

    const NODES: usize = 500;
    const HEARTBEATS_PER_NODE: usize = 200;
    const THREADS: usize = 8;

    let config = HeartbeatConfig {
        suspect_after: Duration::from_secs(30),
        offline_after: Duration::from_secs(90),
        eviction_policy: None,
    };
    let tracker = Arc::new(ConcurrentHeartbeatTracker::new(config));

    // Register all nodes.
    for i in 0..NODES {
        tracker.register(format!("node-{i}"), serde_json::json!({"idx": i}));
    }

    let rss_before = rss_bytes();

    let mut handles = Vec::new();
    for t in 0..THREADS {
        let tr = tracker.clone();
        handles.push(std::thread::spawn(move || {
            let mut hist = LatencyHistogram::new();
            let node_start = t * NODES / THREADS;
            let node_end = (t + 1) * NODES / THREADS;
            for _ in 0..HEARTBEATS_PER_NODE {
                for idx in node_start..node_end {
                    let s = Instant::now();
                    tr.heartbeat(&format!("node-{idx}"));
                    hist.record(s.elapsed());
                }
            }
            hist
        }));
    }

    let mut combined = LatencyHistogram::new();
    for h in handles {
        let hist = h.join().unwrap();
        combined.samples.extend(hist.samples);
    }
    combined.report(&format!(
        "heartbeat ({THREADS} threads, {NODES} nodes, {HEARTBEATS_PER_NODE} hb/node)"
    ));

    // Verify all nodes still online.
    let stats = tracker.fleet_stats();
    assert_eq!(stats.total_nodes, NODES);
    assert_eq!(stats.online, NODES);

    let rss_after = rss_bytes();
    println!(
        "[memory] before={} after={} delta={}",
        fmt_bytes(rss_before),
        fmt_bytes(rss_after),
        fmt_bytes(rss_after.saturating_sub(rss_before))
    );
}

// ---------------------------------------------------------------------------
// Relay soak — sustained message relay with dedup under contention
// ---------------------------------------------------------------------------

#[cfg(feature = "relay")]
#[test]
#[ignore]
fn soak_relay_dedup() {
    use majra::relay::{Relay, RelayMessage};

    const SENDERS: usize = 16;
    const MESSAGES_PER_SENDER: usize = 5000;
    const DUPLICATE_RATIO: usize = 3; // Each message sent 3x.

    let relay = Arc::new(Relay::new("receiver"));

    let rss_before = rss_bytes();

    let mut handles = Vec::new();
    for sender_idx in 0..SENDERS {
        let r = relay.clone();
        handles.push(std::thread::spawn(move || {
            let from = format!("sender-{sender_idx}");
            let mut hist = LatencyHistogram::new();
            for seq in 1..=MESSAGES_PER_SENDER as u64 {
                for _ in 0..DUPLICATE_RATIO {
                    let msg = RelayMessage {
                        seq,
                        from: from.clone(),
                        to: "receiver".into(),
                        topic: "soak".into(),
                        payload: serde_json::Value::Null,
                        timestamp: chrono::Utc::now(),
                    };
                    let s = Instant::now();
                    let _ = r.receive(msg);
                    hist.record(s.elapsed());
                }
            }
            hist
        }));
    }

    let mut combined = LatencyHistogram::new();
    for h in handles {
        let hist = h.join().unwrap();
        combined.samples.extend(hist.samples);
    }
    combined.report(&format!(
        "relay receive ({SENDERS} senders, {MESSAGES_PER_SENDER} msgs, {DUPLICATE_RATIO}x dupes)"
    ));

    let stats = relay.stats();
    let expected_unique = (SENDERS * MESSAGES_PER_SENDER) as u64;
    let expected_dupes = (SENDERS * MESSAGES_PER_SENDER * (DUPLICATE_RATIO - 1)) as u64;
    assert_eq!(stats.messages_received, expected_unique);
    assert_eq!(stats.duplicates_dropped, expected_dupes);
    println!(
        "[relay] received={} dupes_dropped={}",
        stats.messages_received, stats.duplicates_dropped
    );

    let rss_after = rss_bytes();
    println!(
        "[memory] before={} after={} delta={}",
        fmt_bytes(rss_before),
        fmt_bytes(rss_after),
        fmt_bytes(rss_after.saturating_sub(rss_before))
    );
}

// ---------------------------------------------------------------------------
// IPC soak — sustained throughput over real Unix socket
// ---------------------------------------------------------------------------

#[cfg(feature = "ipc")]
#[tokio::test]
#[ignore]
async fn soak_ipc_throughput() {
    use majra::ipc::{IpcConnection, IpcServer};

    const MESSAGES: usize = 50_000;

    let path = std::env::temp_dir().join(format!("majra-soak-{}.sock", uuid::Uuid::new_v4()));

    let path_clone = path.clone();
    let server_handle = tokio::spawn(async move {
        let server = IpcServer::bind(&path_clone).unwrap();
        // Echo server.
        while let Ok(mut conn) = server.accept().await {
            tokio::spawn(async move {
                while let Ok(msg) = conn.recv().await {
                    if conn.send(&msg).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut client = IpcConnection::connect(&path).await.unwrap();
    let payload = serde_json::json!({
        "type": "telemetry",
        "gpu_util": 85.5,
        "vram_used": 6144,
        "batch": [1, 2, 3, 4, 5]
    });

    let rss_before = rss_bytes();
    let start = Instant::now();

    let mut hist = LatencyHistogram::new();
    for _ in 0..MESSAGES {
        let s = Instant::now();
        client.send(&payload).await.unwrap();
        let _resp = client.recv().await.unwrap();
        hist.record(s.elapsed());
    }

    let elapsed = start.elapsed();
    hist.report(&format!("ipc roundtrip ({MESSAGES} messages)"));
    println!(
        "[ipc] total={elapsed:?}  throughput={:.0} msg/s",
        MESSAGES as f64 / elapsed.as_secs_f64()
    );

    let rss_after = rss_bytes();
    println!(
        "[memory] before={} after={} delta={}",
        fmt_bytes(rss_before),
        fmt_bytes(rss_after),
        fmt_bytes(rss_after.saturating_sub(rss_before))
    );

    server_handle.abort();
    let _ = std::fs::remove_file(&path);
}
