use criterion::{Criterion, criterion_group, criterion_main};
use majra::pubsub::{PubSub, TypedPubSub, matches_pattern};

fn bench_pattern_matching(c: &mut Criterion) {
    c.bench_function("matches_pattern exact", |b| {
        b.iter(|| matches_pattern("crew/abc/status", "crew/abc/status"))
    });

    c.bench_function("matches_pattern wildcard *", |b| {
        b.iter(|| matches_pattern("crew/*/status", "crew/abc/status"))
    });

    c.bench_function("matches_pattern wildcard #", |b| {
        b.iter(|| matches_pattern("crew/#", "crew/abc/tasks/1/status"))
    });

    c.bench_function("matches_pattern no_match", |b| {
        b.iter(|| matches_pattern("crew/abc/status", "fleet/node-1/heartbeat"))
    });

    c.bench_function("matches_pattern deep (20 segments)", |b| {
        let topic = (0..20).map(|i| i.to_string()).collect::<Vec<_>>().join("/");
        let pattern = format!("0/*/2/#");
        b.iter(|| matches_pattern(&pattern, &topic))
    });
}

fn bench_untyped_pubsub(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("pubsub_publish_10_subscribers", |b| {
        let hub = PubSub::new();
        let _receivers: Vec<_> = (0..10)
            .map(|i| hub.subscribe(&format!("events/{i}/#")))
            .collect();
        b.iter(|| {
            hub.publish("events/5/data", serde_json::json!({"v": 1}));
        });
    });

    c.bench_function("pubsub_publish_no_match", |b| {
        let hub = PubSub::new();
        let _rx = hub.subscribe("events/specific/topic");
        b.iter(|| {
            hub.publish("other/topic", serde_json::json!(null));
        });
    });

    c.bench_function("pubsub_subscribe_unsubscribe_cycle", |b| {
        let hub = PubSub::new();
        b.iter(|| {
            let _rx = hub.subscribe("temp/pattern/#");
            hub.unsubscribe_all("temp/pattern/#");
        });
    });
}

fn bench_typed_pubsub(c: &mut Criterion) {
    #[derive(Clone)]
    struct Event {
        value: i32,
    }

    c.bench_function("typed_pubsub_publish_fanout_10", |b| {
        let hub = TypedPubSub::<Event>::new();
        let _receivers: Vec<_> = (0..10).map(|_| hub.subscribe("events/#")).collect();
        b.iter(|| {
            hub.publish("events/data", Event { value: 42 });
        });
    });

    c.bench_function("typed_pubsub_publish_with_filter", |b| {
        let hub = TypedPubSub::<Event>::new();
        let _rx = hub.subscribe_filtered("events/#", |e: &Event| e.value > 50);
        b.iter(|| {
            hub.publish("events/data", Event { value: 42 }); // filtered out
        });
    });

    c.bench_function("typed_pubsub_publish_with_replay", |b| {
        use majra::pubsub::TypedPubSubConfig;
        let hub = TypedPubSub::<Event>::with_config(TypedPubSubConfig {
            replay_capacity: 100,
            ..Default::default()
        });
        let _rx = hub.subscribe("events/#");
        b.iter(|| {
            hub.publish("events/data", Event { value: 1 });
        });
    });
}

criterion_group!(
    benches,
    bench_pattern_matching,
    bench_untyped_pubsub,
    bench_typed_pubsub,
);
criterion_main!(benches);
