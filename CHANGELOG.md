# Changelog

## [0.21.0] - 2026-03-21

### Added
- `envelope` — Universal message envelope with Target routing (Node/Topic/Broadcast)
- `pubsub` — Topic-based pub/sub with MQTT-style `*`/`#` wildcard matching
- `queue` — Multi-tier priority queue (5 levels) with DAG dependency scheduling (Kahn's algorithm)
- `relay` — Sequenced, deduplicated inter-node message relay with atomic counters
- `ipc` — Length-prefixed framing (4-byte u32 + JSON) over Unix domain sockets
- `heartbeat` — TTL-based node health tracking with Online → Suspect → Offline FSM
- `ratelimit` — Per-key token bucket rate limiter with lazy refill
- `barrier` — N-way barrier synchronisation with force/deadlock recovery
- `error` — Shared error types (MajraError, IpcError)
- Feature-gated modules: default = pubsub + queue + relay + heartbeat
