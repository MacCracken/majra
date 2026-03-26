//! Pluggable transport layer for the relay.
//!
//! Provides a [`Transport`] trait for abstracting over different connection
//! types (Unix sockets, TCP, future gRPC), and a [`ConnectionPool`] for
//! multiplexing connections across topics.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{debug, trace};

use crate::error::MajraError;
use crate::relay::RelayMessage;

/// A transport connection for sending and receiving relay messages.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Send a message over this transport.
    async fn send(&self, msg: &RelayMessage) -> Result<(), MajraError>;

    /// Receive the next message from this transport.
    async fn recv(&self) -> Result<RelayMessage, MajraError>;

    /// Check if the transport is still connected.
    fn is_connected(&self) -> bool;

    /// Close the transport.
    async fn close(&self) -> Result<(), MajraError>;
}

/// Factory for creating transport connections.
#[async_trait]
pub trait TransportFactory: Send + Sync + 'static {
    /// Create a new transport connection to the given endpoint.
    async fn connect(&self, endpoint: &str) -> Result<Box<dyn Transport>, MajraError>;
}

/// A pooled transport entry with last-use tracking.
struct PooledTransport {
    transport: Arc<dyn Transport>,
    last_used: Instant,
}

/// Multiplexed connection pool for relay transports.
///
/// Reuses connections to the same endpoint and supports concurrent access.
/// Stale connections (idle beyond a configurable TTL) are evicted automatically
/// on `acquire`, or on demand via [`ConnectionPool::evict_stale`].
pub struct ConnectionPool {
    factory: Arc<dyn TransportFactory>,
    connections: Mutex<HashMap<String, Vec<PooledTransport>>>,
    max_per_endpoint: usize,
}

impl ConnectionPool {
    /// Create a new pool with the given factory and max connections per endpoint.
    pub fn new(factory: Arc<dyn TransportFactory>, max_per_endpoint: usize) -> Self {
        Self {
            factory,
            connections: Mutex::new(HashMap::new()),
            max_per_endpoint,
        }
    }

    /// Get or create a transport connection to the given endpoint.
    pub async fn acquire(&self, endpoint: &str) -> Result<Arc<dyn Transport>, MajraError> {
        let key = endpoint.to_string();
        let mut conns = self.connections.lock().await;
        let pool = conns.entry(key.clone()).or_default();

        // Reuse an existing connected transport.
        pool.retain(|pt| pt.transport.is_connected());
        if let Some(pt) = pool.last_mut() {
            pt.last_used = Instant::now();
            trace!(endpoint, "transport: reusing connection");
            return Ok(pt.transport.clone());
        }

        // No connected transports. Drop the lock before doing async I/O.
        drop(conns);

        debug!(endpoint, "transport: creating new connection");
        let transport: Arc<dyn Transport> = Arc::from(self.factory.connect(endpoint).await?);

        // Re-acquire and insert.
        let mut conns = self.connections.lock().await;
        let pool = conns.entry(key).or_default();
        pool.retain(|pt| pt.transport.is_connected());

        if pool.len() >= self.max_per_endpoint {
            // Another task filled the pool while we were connecting.
            if let Err(e) = transport.close().await {
                debug!(endpoint, error = %e, "transport: failed to close excess connection");
            }
            return pool.last().map(|pt| pt.transport.clone()).ok_or_else(|| {
                MajraError::CapacityExceeded(format!(
                    "max connections ({}) reached for {endpoint}",
                    self.max_per_endpoint
                ))
            });
        }

        pool.push(PooledTransport {
            transport: transport.clone(),
            last_used: Instant::now(),
        });
        Ok(transport)
    }

    /// Close all connections to a specific endpoint.
    pub async fn close_endpoint(&self, endpoint: &str) {
        let mut conns = self.connections.lock().await;
        if let Some(pool) = conns.remove(endpoint) {
            debug!(endpoint, count = pool.len(), "transport: closing endpoint");
            for pt in pool {
                if let Err(e) = pt.transport.close().await {
                    debug!(endpoint, error = %e, "transport: close failed");
                }
            }
        }
    }

    /// Close all connections.
    pub async fn close_all(&self) {
        let mut conns = self.connections.lock().await;
        let total: usize = conns.values().map(Vec::len).sum();
        debug!(total, "transport: closing all connections");
        for (endpoint, pool) in conns.drain() {
            for pt in pool {
                if let Err(e) = pt.transport.close().await {
                    debug!(%endpoint, error = %e, "transport: close failed");
                }
            }
        }
    }

    /// Evict connections that have been idle longer than `max_idle`.
    ///
    /// Disconnected connections are also removed. Returns the number of
    /// connections evicted.
    pub async fn evict_stale(&self, max_idle: std::time::Duration) -> usize {
        let now = Instant::now();
        let mut conns = self.connections.lock().await;
        let mut evicted = 0usize;

        for (endpoint, pool) in conns.iter_mut() {
            let before = pool.len();
            let mut to_close = Vec::new();
            pool.retain(|pt| {
                if !pt.transport.is_connected() || now.duration_since(pt.last_used) > max_idle {
                    to_close.push(pt.transport.clone());
                    false
                } else {
                    true
                }
            });
            let removed = before - pool.len();
            if removed > 0 {
                debug!(%endpoint, removed, "transport: evicted stale connections");
            }
            evicted += removed;

            for transport in to_close {
                if let Err(e) = transport.close().await {
                    debug!(%endpoint, error = %e, "transport: close stale failed");
                }
            }
        }

        // Remove empty endpoint entries.
        conns.retain(|_, pool| !pool.is_empty());
        evicted
    }

    /// Number of endpoints with active connections.
    pub async fn endpoint_count(&self) -> usize {
        self.connections.lock().await.len()
    }

    /// Total active connections across all endpoints.
    pub async fn connection_count(&self) -> usize {
        let conns = self.connections.lock().await;
        conns.values().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    /// Mock transport for testing.
    struct MockTransport {
        connected: AtomicBool,
        sends: AtomicU64,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                connected: AtomicBool::new(true),
                sends: AtomicU64::new(0),
            }
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn send(&self, _msg: &RelayMessage) -> Result<(), MajraError> {
            self.sends.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        async fn recv(&self) -> Result<RelayMessage, MajraError> {
            Err(MajraError::Relay("mock: no messages".into()))
        }

        fn is_connected(&self) -> bool {
            self.connected.load(Ordering::Relaxed)
        }

        async fn close(&self) -> Result<(), MajraError> {
            self.connected.store(false, Ordering::Relaxed);
            Ok(())
        }
    }

    struct MockFactory;

    #[async_trait]
    impl TransportFactory for MockFactory {
        async fn connect(&self, _endpoint: &str) -> Result<Box<dyn Transport>, MajraError> {
            Ok(Box::new(MockTransport::new()))
        }
    }

    #[tokio::test]
    async fn pool_acquire_creates_connection() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        let transport = pool.acquire("node-1").await.unwrap();
        assert!(transport.is_connected());
        assert_eq!(pool.endpoint_count().await, 1);
        assert_eq!(pool.connection_count().await, 1);
    }

    #[tokio::test]
    async fn pool_reuses_connection() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        let t1 = pool.acquire("node-1").await.unwrap();
        let t2 = pool.acquire("node-1").await.unwrap();

        // Should be the same connection (Arc points to same allocation).
        assert!(Arc::ptr_eq(&t1, &t2));
        assert_eq!(pool.connection_count().await, 1);
    }

    #[tokio::test]
    async fn pool_multiple_endpoints() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        pool.acquire("node-1").await.unwrap();
        pool.acquire("node-2").await.unwrap();

        assert_eq!(pool.endpoint_count().await, 2);
        assert_eq!(pool.connection_count().await, 2);
    }

    #[tokio::test]
    async fn pool_close_endpoint() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        let t = pool.acquire("node-1").await.unwrap();
        assert!(t.is_connected());

        pool.close_endpoint("node-1").await;
        assert_eq!(pool.endpoint_count().await, 0);
    }

    #[tokio::test]
    async fn pool_close_all() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        pool.acquire("node-1").await.unwrap();
        pool.acquire("node-2").await.unwrap();

        pool.close_all().await;
        assert_eq!(pool.endpoint_count().await, 0);
    }

    #[tokio::test]
    async fn pool_send_through_transport() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        let transport = pool.acquire("node-1").await.unwrap();

        let msg = RelayMessage {
            seq: 1,
            from: "local".into(),
            to: "node-1".into(),
            topic: "test".into(),
            payload: serde_json::Value::Null,
            timestamp: chrono::Utc::now(),
            correlation_id: None,
            is_reply: false,
        };

        transport.send(&msg).await.unwrap();
    }

    #[tokio::test]
    async fn pool_removes_disconnected() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        let t1 = pool.acquire("node-1").await.unwrap();

        // Close the transport (simulates disconnect).
        t1.close().await.unwrap();
        assert!(!t1.is_connected());

        // Next acquire should create a fresh connection.
        let t2 = pool.acquire("node-1").await.unwrap();
        assert!(t2.is_connected());
        assert!(!Arc::ptr_eq(&t1, &t2));
    }

    #[tokio::test]
    async fn pool_evict_stale_removes_idle_connections() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        pool.acquire("node-1").await.unwrap();
        pool.acquire("node-2").await.unwrap();
        assert_eq!(pool.connection_count().await, 2);

        // Zero duration evicts everything.
        let evicted = pool.evict_stale(std::time::Duration::ZERO).await;
        assert_eq!(evicted, 2);
        assert_eq!(pool.connection_count().await, 0);
        assert_eq!(pool.endpoint_count().await, 0);
    }

    #[tokio::test]
    async fn pool_evict_stale_keeps_fresh() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        pool.acquire("node-1").await.unwrap();

        // Large TTL keeps everything.
        let evicted = pool.evict_stale(std::time::Duration::from_secs(3600)).await;
        assert_eq!(evicted, 0);
        assert_eq!(pool.connection_count().await, 1);
    }

    #[tokio::test]
    async fn pool_evict_stale_removes_disconnected() {
        let pool = ConnectionPool::new(Arc::new(MockFactory), 4);
        let t = pool.acquire("node-1").await.unwrap();
        t.close().await.unwrap();

        // Even with a huge TTL, disconnected connections are evicted.
        let evicted = pool.evict_stale(std::time::Duration::from_secs(3600)).await;
        assert_eq!(evicted, 1);
        assert_eq!(pool.connection_count().await, 0);
    }
}
