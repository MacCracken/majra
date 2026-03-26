# Migrating SecureYeoman to majra

> SecureYeoman provides A2A (Agent-to-Agent) peer discovery, authentication,
> and secure messaging. This guide covers replacing SecureYeoman's internal
> relay, heartbeat, barrier, and IPC implementations with majra primitives.

## Modules to adopt

| SecureYeoman component | majra replacement | Feature flag |
|-----------------------|-------------------|-------------|
| Peer message relay | `Relay` | `relay` |
| Peer health tracking | `ConcurrentHeartbeatTracker` | `heartbeat` |
| Connection pooling | `ConnectionPool` | `relay` |
| Sync barriers (multi-agent) | `AsyncBarrierSet` | `barrier` |
| Local agent IPC | `IpcServer` / `IpcConnection` | `ipc` |
| Per-peer rate limiting | `RateLimiter` | `ratelimit` |

## Step 1: Replace the peer relay

SecureYeoman's message forwarding becomes a `Relay`:

```rust
use majra::relay::Relay;

let relay = Relay::new("yeoman-node-1");

// Send to a specific peer.
let seq = relay.send("peer-abc", "auth/challenge", json!({
    "nonce": "...",
    "timestamp": Utc::now(),
}));

// Broadcast to all peers.
relay.broadcast("discovery/announce", json!({
    "capabilities": ["a2a-v2", "streaming"],
}));

// Process incoming messages (dedup built in).
if let Some(incoming) = relay.receive(wire_msg) {
    // Handle the deduplicated message.
}

// Subscribe to all relay traffic.
let mut rx = relay.subscribe();
```

### Key differences from SecureYeoman's internal relay

- Automatic dedup by sender + sequence number (no manual tracking)
- Atomic sequence counters (no lock contention)
- Built-in stats (`relay.stats()`) replace manual counters
- `DashMap`-backed — lock-free concurrent access

## Step 2: Replace peer health tracking

```rust
use majra::heartbeat::{ConcurrentHeartbeatTracker, HeartbeatConfig, EvictionPolicy};

let (eviction_tx, mut eviction_rx) = tokio::sync::mpsc::unbounded_channel();
let tracker = ConcurrentHeartbeatTracker::new(HeartbeatConfig {
    suspect_after: Duration::from_secs(15),
    offline_after: Duration::from_secs(45),
    eviction_policy: Some(EvictionPolicy {
        offline_cycles: 3,
        eviction_tx: Some(eviction_tx),
    }),
});

// Register authenticated peers.
tracker.register("peer-abc", json!({
    "public_key": "...",
    "capabilities": ["a2a-v2"],
}));

// Periodic sweep.
tracker.update_statuses();

// Listen for evicted peers.
while let Some(evicted_peer) = eviction_rx.recv().await {
    // Remove from routing table, close connection, etc.
}
```

## Step 3: Replace connection pooling

```rust
use majra::transport::{ConnectionPool, Transport, TransportFactory};

struct TlsTransportFactory { /* ... */ }

#[async_trait]
impl TransportFactory for TlsTransportFactory {
    async fn connect(&self, endpoint: &str) -> Result<Box<dyn Transport>, MajraError> {
        // Establish TLS connection to peer.
    }
}

let pool = ConnectionPool::new(Arc::new(TlsTransportFactory::new()), 4);

// Acquire a connection (reuses existing if available).
let transport = pool.acquire("peer-abc:8443").await?;
transport.send(&relay_msg).await?;
```

## Step 4: Replace multi-agent sync barriers

```rust
use majra::barrier::AsyncBarrierSet;

let barriers = AsyncBarrierSet::new();

// Create a barrier for a multi-agent task.
barriers.create("task-xyz", ["agent-1", "agent-2", "agent-3"].into());

// Each agent arrives and waits.
barriers.arrive_and_wait("task-xyz", "agent-1").await?;

// If an agent dies, force-release the barrier.
barriers.force("task-xyz", "dead-agent");
```

## Step 5: Replace local IPC

```rust
use majra::ipc::{IpcServer, IpcConnection};

// Server (yeoman daemon).
let server = IpcServer::bind(Path::new("/run/yeoman/agent.sock"))?;
let mut conn = server.accept().await?;
let msg = conn.recv().await?;
conn.send(&json!({"status": "ok"})).await?;

// Client (agent process).
let mut client = IpcConnection::connect(Path::new("/run/yeoman/agent.sock")).await?;
client.send(&json!({"action": "register"})).await?;
```

## Step 6: Add per-peer rate limiting

```rust
use majra::ratelimit::RateLimiter;

let limiter = RateLimiter::new(100.0, 200); // 100 req/s, burst 200

if limiter.check("peer-abc") {
    // Process request.
} else {
    // Reject: rate limited.
}

// Periodic cleanup of idle peers.
limiter.evict_stale(Duration::from_secs(300));
```

## Cargo.toml

```toml
[dependencies]
majra = { version = "1", features = ["relay", "heartbeat", "barrier", "ipc", "ratelimit"] }
```
