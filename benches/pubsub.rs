use criterion::{Criterion, criterion_group, criterion_main};
use majra::pubsub::{
    DirectChannel, HashedChannel, PubSub, TopicHash, TypedPubSub, matches_pattern,
};

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
        let pattern = "0/*/2/#".to_string();
        b.iter(|| matches_pattern(&pattern, &topic))
    });
}

fn bench_untyped_pubsub(c: &mut Criterion) {
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

fn bench_dual_pipe(c: &mut Criterion) {
    #[derive(Clone)]
    struct Evt {
        _v: i32,
    }

    // Fast path: exact-topic subscriber, O(1) DashMap lookup.
    c.bench_function("typed_exact_topic_publish", |b| {
        let hub = TypedPubSub::<Evt>::new();
        let _rx = hub.subscribe("events/data"); // exact — no wildcards
        b.iter(|| {
            hub.publish("events/data", Evt { _v: 1 });
        });
    });

    // Slow path: wildcard subscriber, pattern iteration.
    c.bench_function("typed_wildcard_topic_publish", |b| {
        let hub = TypedPubSub::<Evt>::new();
        let _rx = hub.subscribe("events/#"); // wildcard — requires iteration
        b.iter(|| {
            hub.publish("events/data", Evt { _v: 1 });
        });
    });

    // Mixed: 10 exact + 10 wildcard subscribers, publish to exact topic.
    c.bench_function("typed_mixed_10exact_10wild_publish", |b| {
        let hub = TypedPubSub::<Evt>::new();
        let _exact: Vec<_> = (0..10)
            .map(|i| hub.subscribe(&format!("events/topic-{i}")))
            .collect();
        let _wild: Vec<_> = (0..10)
            .map(|i| hub.subscribe(&format!("events/group-{i}/#")))
            .collect();
        b.iter(|| {
            hub.publish("events/topic-5", Evt { _v: 1 });
        });
    });

    // Throughput: publish 1000 messages to exact topic.
    c.bench_function("typed_exact_1000_publish", |b| {
        let hub = TypedPubSub::<Evt>::new();
        let _rx = hub.subscribe("events/data");
        b.iter(|| {
            for i in 0..1000 {
                hub.publish("events/data", Evt { _v: i });
            }
        });
    });

    // Throughput: publish 1000 messages with wildcard.
    c.bench_function("typed_wildcard_1000_publish", |b| {
        let hub = TypedPubSub::<Evt>::new();
        let _rx = hub.subscribe("events/#");
        b.iter(|| {
            for i in 0..1000 {
                hub.publish("events/data", Evt { _v: i });
            }
        });
    });

    // Scaling: 100 exact subscribers, publish matches 1.
    c.bench_function("typed_exact_100subs_publish_1match", |b| {
        let hub = TypedPubSub::<Evt>::new();
        let _subs: Vec<_> = (0..100)
            .map(|i| hub.subscribe(&format!("events/topic-{i}")))
            .collect();
        b.iter(|| {
            hub.publish("events/topic-50", Evt { _v: 1 });
        });
    });

    // Scaling: 100 wildcard subscribers, publish matches some.
    c.bench_function("typed_wild_100subs_publish", |b| {
        let hub = TypedPubSub::<Evt>::new();
        let _subs: Vec<_> = (0..100)
            .map(|i| hub.subscribe(&format!("events/group-{i}/#")))
            .collect();
        b.iter(|| {
            hub.publish("events/group-50/data", Evt { _v: 1 });
        });
    });
}

fn bench_direct_channel(c: &mut Criterion) {
    // DirectChannel: raw broadcast, no topic routing or timestamp.
    c.bench_function("direct_channel_publish_1sub", |b| {
        let ch = DirectChannel::<i32>::new(256);
        let _rx = ch.subscribe();
        b.iter(|| {
            ch.publish(42);
        });
    });

    c.bench_function("direct_channel_publish_10sub", |b| {
        let ch = DirectChannel::<i32>::new(256);
        let _rxs: Vec<_> = (0..10).map(|_| ch.subscribe()).collect();
        b.iter(|| {
            ch.publish(42);
        });
    });

    c.bench_function("direct_channel_1000_publish_1sub", |b| {
        let ch = DirectChannel::<i32>::new(4096);
        let _rx = ch.subscribe();
        b.iter(|| {
            for i in 0..1000 {
                ch.publish(i);
            }
        });
    });
}

fn bench_hashed_channel(c: &mut Criterion) {
    let topic = TopicHash::new("events/data");

    c.bench_function("hashed_channel_publish_1sub", |b| {
        let ch = HashedChannel::<i32>::new(256);
        let _rx = ch.subscribe(topic);
        b.iter(|| {
            ch.publish(topic, 42);
        });
    });

    c.bench_function("hashed_channel_publish_10topics_1match", |b| {
        let ch = HashedChannel::<i32>::new(256);
        for i in 0..10 {
            let _rx = ch.subscribe(TopicHash::new(&format!("events/topic-{i}")));
        }
        let target = TopicHash::new("events/topic-5");
        b.iter(|| {
            ch.publish(target, 42);
        });
    });

    c.bench_function("hashed_channel_1000_publish_1sub", |b| {
        let ch = HashedChannel::<i32>::new(4096);
        let _rx = ch.subscribe(topic);
        b.iter(|| {
            for i in 0..1000 {
                ch.publish(topic, i);
            }
        });
    });
}

criterion_group!(
    benches,
    bench_pattern_matching,
    bench_untyped_pubsub,
    bench_typed_pubsub,
    bench_dual_pipe,
    bench_direct_channel,
    bench_hashed_channel,
);
criterion_main!(benches);
