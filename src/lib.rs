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
//! | `full` | — | Enables all features |

pub mod envelope;
pub mod error;

#[cfg(feature = "pubsub")]
pub mod pubsub;

#[cfg(feature = "queue")]
pub mod queue;

#[cfg(feature = "relay")]
pub mod relay;

#[cfg(feature = "ipc")]
pub mod ipc;

#[cfg(feature = "heartbeat")]
pub mod heartbeat;

#[cfg(feature = "ratelimit")]
pub mod ratelimit;

#[cfg(feature = "barrier")]
pub mod barrier;
