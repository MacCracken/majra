#![no_main]

use libfuzzer_sys::fuzz_target;
use majra::heartbeat::{HeartbeatConfig, HeartbeatTracker};
use std::time::Duration;

fuzz_target!(|data: &[u8]| {
    if data.len() < 3 {
        return;
    }

    let suspect_ms = (data[0] as u64).max(1) * 10;
    let offline_ms = (data[1] as u64).max(1) * 10 + suspect_ms;

    let config = HeartbeatConfig {
        suspect_after: Duration::from_millis(suspect_ms),
        offline_after: Duration::from_millis(offline_ms),
        eviction_policy: None,
    };
    let mut tracker = HeartbeatTracker::new(config);

    // Perform operations driven by fuzzer input — should never panic.
    for byte in &data[2..] {
        let id = format!("node-{}", byte % 8);
        match byte % 4 {
            0 => tracker.register(&id, serde_json::Value::Null),
            1 => {
                tracker.heartbeat(&id);
            }
            2 => {
                tracker.deregister(&id);
            }
            _ => {
                tracker.update_statuses();
            }
        }
    }
});
