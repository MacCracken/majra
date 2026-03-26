//! Three-tier pub/sub example — DirectChannel, HashedChannel, TypedPubSub.
//!
//! Run: `cargo run --example pubsub_tiers --features pubsub`

use majra::pubsub::{DirectChannel, HashedChannel, TopicHash, TypedPubSub};

#[tokio::main]
async fn main() {
    // ── Tier 1: DirectChannel (73M msg/s) ────────────────────────
    // Raw broadcast — no topic routing, no timestamp.
    // Best for: telemetry, metrics, hot-path events.

    let direct = DirectChannel::<String>::new(256);
    let mut rx1 = direct.subscribe();

    direct.publish("gpu-temp: 72°C".to_string());
    let msg = rx1.recv().await.unwrap();
    println!("[DirectChannel] {msg}");

    // ── Tier 2: HashedChannel (16M msg/s) ────────────────────────
    // Hashed topic routing + coarse monotonic timestamp.
    // Best for: event buses, inter-service messaging.

    let hashed = HashedChannel::<serde_json::Value>::new(256);
    let topic = TopicHash::new("events/training/progress");
    let mut rx2 = hashed.subscribe(topic);

    hashed.publish(topic, serde_json::json!({"epoch": 5, "loss": 0.42}));
    let msg = rx2.recv().await.unwrap();
    println!(
        "[HashedChannel] hash={} ts={}ns payload={}",
        msg.topic_hash.value(),
        msg.timestamp_ns,
        msg.payload
    );

    // ── Tier 3: TypedPubSub (1.1M msg/s) ────────────────────────
    // Full MQTT-style wildcards, filters, replay, auto-cleanup.
    // Best for: user-facing pub/sub, audit logging, webhook routing.

    #[derive(Clone, Debug)]
    #[allow(dead_code)]
    struct TrainEvent {
        model: String,
        epoch: u32,
        loss: f64,
    }

    let hub = TypedPubSub::<TrainEvent>::new();

    // Wildcard subscriber — matches all training events.
    let mut rx_all = hub.subscribe("train/#");

    // Exact subscriber — only llama events.
    let mut rx_llama = hub.subscribe("train/llama");

    // Filtered subscriber — only low-loss events.
    let mut rx_good = hub.subscribe_filtered("train/#", |e: &TrainEvent| e.loss < 0.5);

    hub.publish(
        "train/llama",
        TrainEvent {
            model: "llama-70b".into(),
            epoch: 10,
            loss: 0.31,
        },
    );

    hub.publish(
        "train/mistral",
        TrainEvent {
            model: "mistral-7b".into(),
            epoch: 5,
            loss: 0.82,
        },
    );

    // rx_all gets both.
    let m1 = rx_all.recv().await.unwrap();
    let m2 = rx_all.recv().await.unwrap();
    println!(
        "[TypedPubSub wildcard] {}: loss={}",
        m1.payload.model, m1.payload.loss
    );
    println!(
        "[TypedPubSub wildcard] {}: loss={}",
        m2.payload.model, m2.payload.loss
    );

    // rx_llama gets only llama.
    let m = rx_llama.recv().await.unwrap();
    println!(
        "[TypedPubSub exact] {}: loss={}",
        m.payload.model, m.payload.loss
    );
    assert!(rx_llama.try_recv().is_err()); // no mistral

    // rx_good gets only loss < 0.5.
    let m = rx_good.recv().await.unwrap();
    println!(
        "[TypedPubSub filtered] {}: loss={}",
        m.payload.model, m.payload.loss
    );
    assert!(rx_good.try_recv().is_err()); // mistral filtered out

    println!(
        "\nStats: direct={} hashed={} typed={}",
        direct.messages_published(),
        hashed.messages_published(),
        hub.messages_published()
    );
}
