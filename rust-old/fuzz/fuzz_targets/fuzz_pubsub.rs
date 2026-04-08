#![no_main]

use libfuzzer_sys::fuzz_target;
use majra::pubsub::{matches_pattern, PubSub};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }

    // Split data into pattern and topic.
    let split = (data[0] as usize).min(data.len() - 1).max(1);
    let pattern = String::from_utf8_lossy(&data[1..split]);
    let topic = String::from_utf8_lossy(&data[split..]);

    // Pattern matching should never panic.
    let _ = matches_pattern(&pattern, &topic);

    // Pub/sub operations should never panic.
    let hub = PubSub::new();
    let _rx = hub.subscribe(&pattern);
    hub.publish(&topic, serde_json::Value::Null);
    hub.unsubscribe_all(&pattern);
});
