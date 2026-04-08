//! WebSocket bridge for in-process pub/sub.
//!
//! Bridges [`crate::pubsub::PubSub`] topics to WebSocket clients. Each
//! connected client subscribes to a pattern and receives matching messages
//! as JSON text frames.
//!
//! ## Feature flag
//!
//! Requires the `ws` feature (implies `pubsub`).
//!
//! ## Architecture
//!
//! ```text
//! PubSub hub ──► WsBridge::run(addr)
//!                    │
//!                    ├─ accepts WS connections
//!                    ├─ client sends: {"subscribe": "events/#"}
//!                    ├─ bridge subscribes to PubSub pattern
//!                    └─ bridge forwards matching TopicMessages as JSON
//! ```

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, trace, warn};

use crate::pubsub::PubSub;

/// Configuration for the WebSocket bridge.
#[derive(Debug, Clone)]
pub struct WsBridgeConfig {
    /// Maximum concurrent WebSocket connections.
    pub max_connections: usize,
}

impl Default for WsBridgeConfig {
    fn default() -> Self {
        Self {
            max_connections: 1024,
        }
    }
}

/// WebSocket bridge that fans out pub/sub messages to WebSocket clients.
///
/// Clients connect and send a JSON message `{"subscribe": "pattern"}` to
/// subscribe to a topic pattern. All matching messages are forwarded as
/// JSON text frames.
pub struct WsBridge {
    hub: Arc<PubSub>,
    config: WsBridgeConfig,
}

impl WsBridge {
    /// Create a new bridge backed by the given pub/sub hub.
    pub fn new(hub: Arc<PubSub>, config: WsBridgeConfig) -> Self {
        Self { hub, config }
    }

    /// Run the WebSocket server on the given address.
    ///
    /// This function runs forever, accepting connections and spawning
    /// a task per client. Cancel via the returned [`tokio::task::JoinHandle`]
    /// or by dropping the runtime.
    pub async fn run(
        &self,
        addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let listener = TcpListener::bind(addr).await?;
        info!(%addr, "ws-bridge: listening");

        let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        loop {
            let (stream, peer) = listener.accept().await?;
            let current = active.load(std::sync::atomic::Ordering::Relaxed);
            if current >= self.config.max_connections {
                debug!(%peer, "ws-bridge: rejected (max connections)");
                drop(stream);
                continue;
            }

            active.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let hub = self.hub.clone();
            let active_clone = active.clone();

            tokio::spawn(async move {
                if let Err(e) = handle_client(hub, stream, peer).await {
                    debug!(%peer, error = %e, "ws-bridge: client error");
                }
                active_clone.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                debug!(%peer, "ws-bridge: client disconnected");
            });
        }
    }

    /// Run the bridge and return a handle that can be used to stop it.
    pub fn spawn(
        self: Arc<Self>,
        addr: SocketAddr,
    ) -> tokio::task::JoinHandle<Result<(), Box<dyn std::error::Error + Send + Sync>>> {
        tokio::spawn(async move { self.run(addr).await })
    }
}

async fn handle_client(
    hub: Arc<PubSub>,
    stream: TcpStream,
    peer: SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    debug!(%peer, "ws-bridge: client connected");

    // Wait for the subscription message.
    let pattern = loop {
        match ws_rx.next().await {
            Some(Ok(Message::Text(text))) => {
                if let Ok(msg) = serde_json::from_str::<serde_json::Value>(&text)
                    && let Some(pattern) = msg.get("subscribe").and_then(|v| v.as_str())
                {
                    break pattern.to_string();
                }
                // Ignore non-subscribe messages during handshake.
            }
            Some(Ok(Message::Close(_))) | None => return Ok(()),
            Some(Err(e)) => return Err(e.into()),
            _ => continue,
        }
    };

    debug!(%peer, %pattern, "ws-bridge: subscribed");
    let mut rx = hub.subscribe(&pattern);

    // Forward messages.
    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(topic_msg) => {
                        let json = serde_json::json!({
                            "topic": topic_msg.topic,
                            "payload": topic_msg.payload,
                            "timestamp": topic_msg.timestamp.to_rfc3339(),
                        });
                        if let Err(e) = ws_tx.send(Message::Text(json.to_string())).await {
                            trace!(%peer, error = %e, "ws-bridge: send failed");
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(%peer, lagged = n, "ws-bridge: client lagging, skipped messages");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = ws_rx.next() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(data))) => {
                        let _ = ws_tx.send(Message::Pong(data)).await;
                    }
                    Some(Err(_)) => break,
                    _ => {} // Ignore other messages while streaming.
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = WsBridgeConfig::default();
        assert_eq!(config.max_connections, 1024);
    }

    #[test]
    fn bridge_construction() {
        let hub = Arc::new(PubSub::new());
        let _bridge = WsBridge::new(hub, WsBridgeConfig::default());
    }
}
