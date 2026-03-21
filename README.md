# Majra

> مجرا (Arabic/Persian: conduit, channel) — Distributed queue & multiplex engine for Rust

Majra provides shared messaging primitives for the [AGNOS](https://github.com/MacCracken) ecosystem, eliminating duplicate pub/sub, queue, relay, and heartbeat implementations across [AgnosAI](https://github.com/MacCracken/agnosai), [daimon](https://github.com/agnostos/daimon), [SecureYeoman](https://github.com/MacCracken/secureyeoman), and [sutra](https://github.com/MacCracken/sutra).

**Pure Rust, async-native** — built on tokio, zero-copy where possible.

## Features

| Module | Feature | Description |
|--------|---------|-------------|
| **pubsub** | `pubsub` | Topic-based pub/sub with MQTT-style `*`/`#` wildcard matching |
| **queue** | `queue` | Multi-tier priority queue (5 levels) with DAG dependency scheduling |
| **relay** | `relay` | Sequenced, deduplicated inter-node message relay |
| **ipc** | `ipc` | Length-prefixed framing over Unix domain sockets |
| **heartbeat** | `heartbeat` | TTL-based node health: Online → Suspect → Offline |
| **ratelimit** | `ratelimit` | Per-key token bucket rate limiter |
| **barrier** | `barrier` | N-way barrier synchronisation with deadlock recovery |

Default features: `pubsub`, `queue`, `relay`, `heartbeat`.

## Quick Start

```toml
[dependencies]
majra = "0.21"
```

### Pub/Sub

```rust
use majra::pubsub::PubSub;

let hub = PubSub::new();
let mut rx = hub.subscribe("crew/*/status");

hub.publish("crew/abc/status", serde_json::json!({"done": true}));

let msg = rx.recv().await.unwrap();
assert_eq!(msg.topic, "crew/abc/status");
```

### Priority Queue

```rust
use majra::queue::{PriorityQueue, QueueItem, Priority};

let mut q = PriorityQueue::new();
q.enqueue(QueueItem::new(Priority::Low, "background-task"));
q.enqueue(QueueItem::new(Priority::Critical, "urgent-task"));

assert_eq!(q.dequeue().unwrap().payload, "urgent-task");
```

### DAG Scheduling

```rust
use majra::queue::{Dag, DagScheduler};
use std::collections::{HashMap, HashSet};

let dag = Dag {
    edges: HashMap::from([
        ("build".into(), vec![]),
        ("test".into(), vec!["build".into()]),
        ("deploy".into(), vec!["test".into()]),
    ]),
};
let sched = DagScheduler::new(&dag).unwrap();

let ready = sched.ready(&HashSet::new());
assert_eq!(ready, vec!["build"]);
```

### Heartbeat Tracking

```rust
use majra::heartbeat::HeartbeatTracker;

let mut tracker = HeartbeatTracker::default();
tracker.register("node-1", serde_json::json!({"gpu": true}));
tracker.heartbeat("node-1");

let online = tracker.online();
```

### Message Relay

```rust
use majra::relay::Relay;

let relay = Relay::with_defaults("node-1");
let mut rx = relay.subscribe();

relay.broadcast("announce", serde_json::json!({"joined": true}));
```

## Ecosystem

Majra unifies patterns from three battle-tested implementations:

| Source | What it contributed |
|--------|-------------------|
| **AgnosAI** | Priority DAG scheduling, pub/sub wildcards, relay dedup, barrier sync |
| **SecureYeoman** | A2A heartbeat, token bucket rate limiting, broadcast fan-out |
| **daimon** | Topic routing, fleet relay, IPC framing |

## License

AGPL-3.0-only
