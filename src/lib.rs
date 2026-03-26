//! # Majra
//!
//! مجرا (Arabic/Persian: conduit, channel) — Distributed queue & multiplex engine.
//!
//! Shared messaging primitives for the [AGNOS](https://github.com/MacCracken) ecosystem,
//! eliminating duplicate pub/sub, queue, relay, and heartbeat implementations across
//! [AgnosAI](https://github.com/MacCracken/agnosai),
//! [daimon](https://github.com/agnostos/daimon),
//! [SecureYeoman](https://github.com/MacCracken/secureyeoman), and
//! [sutra](https://github.com/MacCracken/sutra).
//!
//! **Pure Rust, async-native** — built on tokio, zero-copy where possible.
//!
//! ## Feature flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `pubsub` | yes | Topic-based pub/sub with MQTT-style wildcard matching |
//! | `queue` | yes | Multi-tier priority queue with DAG dependency scheduling |
//! | `relay` | yes | Sequenced, deduplicated inter-node message relay |
//! | `ipc` | no | Length-prefixed framing over Unix domain sockets |
//! | `heartbeat` | yes | TTL-based health tracking with Online→Suspect→Offline FSM |
//! | `ratelimit` | no | Per-key token bucket rate limiter |
//! | `barrier` | no | N-way barrier synchronisation with deadlock recovery |
//! | `sqlite` | no | SQLite persistence for managed queue |
//! | `fleet` | no | Distributed job queue with work-stealing |
//! | `redis-backend` | no | Redis-backed cross-process pub/sub and queues |
//! | `prometheus` | no | Built-in Prometheus metrics exporter |
//! | `quic` | no | QUIC transport with multiplexed streams and datagrams |
//! | `ipc-encrypted` | no | AES-256-GCM encrypted IPC channels |
//! | `postgres` | no | PostgreSQL persistence for workflow storage |
//! | `ws` | no | WebSocket bridge for pub/sub fan-out |
//! | `full` | — | Enables all features |

pub mod envelope;
pub mod error;
pub mod metrics;
pub mod namespace;
#[allow(dead_code)]
pub(crate) mod util;

#[cfg(feature = "logging")]
pub mod logging;

#[cfg(feature = "pubsub")]
pub mod pubsub;

#[cfg(feature = "queue")]
pub mod queue;

#[cfg(feature = "relay")]
pub mod relay;

#[cfg(feature = "relay")]
pub mod transport;

#[cfg(feature = "ipc")]
pub mod ipc;

#[cfg(feature = "ipc-encrypted")]
pub mod ipc_encrypted;

#[cfg(feature = "heartbeat")]
pub mod heartbeat;

#[cfg(feature = "ratelimit")]
pub mod ratelimit;

#[cfg(feature = "barrier")]
pub mod barrier;

#[cfg(feature = "dag")]
pub mod dag;

#[cfg(feature = "fleet")]
pub mod fleet;

#[cfg(feature = "redis-backend")]
pub mod redis_backend;

#[cfg(feature = "ws")]
pub mod ws;

#[cfg(all(feature = "postgres", feature = "dag"))]
pub mod postgres_backend;

#[cfg(feature = "quic")]
pub mod quic;

// ---------------------------------------------------------------------------
// Compile-time Send + Sync assertions
// ---------------------------------------------------------------------------

#[cfg(test)]
mod sync_assertions {
    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_types_are_send_sync() {
        // Envelope & error
        _assert_send_sync::<super::envelope::Envelope>();

        // PubSub
        #[cfg(feature = "pubsub")]
        _assert_send_sync::<super::pubsub::PubSub>();

        // Queue
        #[cfg(feature = "queue")]
        {
            _assert_send_sync::<super::queue::ConcurrentPriorityQueue<String>>();
            _assert_send_sync::<super::queue::ManagedQueue<String>>();
        }

        // Relay
        #[cfg(feature = "relay")]
        _assert_send_sync::<super::relay::Relay>();

        // Heartbeat
        #[cfg(feature = "heartbeat")]
        _assert_send_sync::<super::heartbeat::ConcurrentHeartbeatTracker>();

        // Rate limiter
        #[cfg(feature = "ratelimit")]
        _assert_send_sync::<super::ratelimit::RateLimiter>();

        // Barrier
        #[cfg(feature = "barrier")]
        _assert_send_sync::<super::barrier::AsyncBarrierSet>();

        // Fleet
        #[cfg(feature = "fleet")]
        _assert_send_sync::<super::fleet::FleetQueue<String>>();
    }
}
