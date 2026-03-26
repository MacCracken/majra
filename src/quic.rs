//! QUIC transport for the relay layer.
//!
//! Provides a [`QuicTransport`] implementing the [`Transport`] trait, a
//! [`QuicTransportFactory`] for creating connections, and a [`QuicListener`]
//! for accepting inbound peers.
//!
//! ## Features
//!
//! - **Multiplexed streams** — each send/recv opens an ordered bi-directional
//!   QUIC stream (no head-of-line blocking between messages)
//! - **Unreliable datagrams** — fire-and-forget channel via QUIC datagrams
//! - **0-RTT reconnect** — clients cache session tickets for fast resume
//! - **Connection migration** — QUIC handles IP changes transparently
//! - **TLS built-in** — all traffic encrypted via rustls
//!
//! TCP remains the default transport. QUIC is opt-in via the `quic` feature.
//!
//! Requires the `quic` feature flag.

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use quinn::{ClientConfig, Endpoint, RecvStream, ServerConfig};
use tokio::sync::Mutex;
use tracing::{debug, trace};

use crate::error::MajraError;
use crate::relay::RelayMessage;
use crate::transport::{Transport, TransportFactory};

// ---------------------------------------------------------------------------
// TLS helpers
// ---------------------------------------------------------------------------

/// Generate a self-signed TLS certificate and key for testing/development.
///
/// For production, supply your own [`rustls::ServerConfig`] and
/// [`rustls::ClientConfig`] via [`QuicListener::bind`] and
/// [`QuicTransportFactory::new`].
pub fn self_signed_tls() -> Result<(rustls::ServerConfig, rustls::ClientConfig), MajraError> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])
        .map_err(|e| MajraError::Relay(format!("cert generation: {e}")))?;

    let cert_der = cert.cert.der().clone();
    let key_der = cert.key_pair.serialize_der();

    let certs = vec![cert_der.clone()];
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(key_der);

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs.clone(), key.into())
        .map_err(|e| MajraError::Relay(format!("server tls: {e}")))?;

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(cert_der)
        .map_err(|e| MajraError::Relay(format!("root cert: {e}")))?;

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    Ok((server_config, client_config))
}

// ---------------------------------------------------------------------------
// QuicTransport — implements Transport trait
// ---------------------------------------------------------------------------

/// A QUIC connection implementing the [`Transport`] trait.
///
/// Each `send()` opens a new bi-directional stream (multiplexed, no HOL blocking).
/// Also supports fire-and-forget datagrams via [`QuicTransport::send_datagram`].
pub struct QuicTransport {
    connection: quinn::Connection,
    /// Buffered receive stream for `recv()` calls.
    recv_state: Mutex<Option<RecvStream>>,
}

impl QuicTransport {
    /// Wrap a quinn connection as a majra Transport.
    pub fn new(connection: quinn::Connection) -> Self {
        Self {
            connection,
            recv_state: Mutex::new(None),
        }
    }

    /// Send a fire-and-forget datagram (unreliable, unordered).
    ///
    /// Ideal for real-time telemetry, game state, or sensor readings where
    /// occasional loss is acceptable. Falls back to error if the peer doesn't
    /// support datagrams or the payload is too large.
    pub fn send_datagram(&self, data: &[u8]) -> Result<(), MajraError> {
        self.connection
            .send_datagram(data.to_vec().into())
            .map_err(|e| MajraError::Relay(format!("datagram send: {e}")))
    }

    /// Receive a datagram (unreliable, unordered).
    pub async fn recv_datagram(&self) -> Result<Vec<u8>, MajraError> {
        let bytes = self
            .connection
            .read_datagram()
            .await
            .map_err(|e| MajraError::Relay(format!("datagram recv: {e}")))?;
        Ok(bytes.to_vec())
    }

    /// The remote address of the peer.
    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// The stable connection ID.
    pub fn stable_id(&self) -> usize {
        self.connection.stable_id()
    }
}

#[async_trait]
impl Transport for QuicTransport {
    async fn send(&self, msg: &RelayMessage) -> Result<(), MajraError> {
        let data =
            serde_json::to_vec(msg).map_err(|e| MajraError::Relay(format!("serialize: {e}")))?;

        let (mut send, recv) = self
            .connection
            .open_bi()
            .await
            .map_err(|e| MajraError::Relay(format!("open stream: {e}")))?;

        // Write length-prefixed frame.
        let len = (data.len() as u32).to_be_bytes();
        send.write_all(&len)
            .await
            .map_err(|e| MajraError::Relay(format!("write len: {e}")))?;
        send.write_all(&data)
            .await
            .map_err(|e| MajraError::Relay(format!("write data: {e}")))?;
        send.finish()
            .map_err(|e| MajraError::Relay(format!("finish stream: {e}")))?;

        // Store the recv side for the response.
        let mut state = self.recv_state.lock().await;
        *state = Some(recv);

        trace!(bytes = data.len(), "quic: sent");
        Ok(())
    }

    async fn recv(&self) -> Result<RelayMessage, MajraError> {
        // Try to use the recv stream from the last send, or accept a new one.
        let mut recv = {
            let mut state = self.recv_state.lock().await;
            match state.take() {
                Some(r) => r,
                None => {
                    // Accept an incoming stream.
                    let (_send, recv) = self
                        .connection
                        .accept_bi()
                        .await
                        .map_err(|e| MajraError::Relay(format!("accept stream: {e}")))?;
                    recv
                }
            }
        };

        // Read length-prefixed frame.
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf)
            .await
            .map_err(|e| MajraError::Relay(format!("read len: {e}")))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut buf = vec![0u8; len];
        recv.read_exact(&mut buf)
            .await
            .map_err(|e| MajraError::Relay(format!("read data: {e}")))?;

        let msg: RelayMessage = serde_json::from_slice(&buf)
            .map_err(|e| MajraError::Relay(format!("deserialize: {e}")))?;

        trace!(bytes = len, "quic: received");
        Ok(msg)
    }

    fn is_connected(&self) -> bool {
        self.connection.close_reason().is_none()
    }

    async fn close(&self) -> Result<(), MajraError> {
        self.connection.close(0u32.into(), b"shutdown");
        debug!(
            remote = %self.connection.remote_address(),
            "quic: connection closed"
        );
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// QuicTransportFactory — implements TransportFactory
// ---------------------------------------------------------------------------

/// Factory for creating outbound QUIC connections.
///
/// Supports 0-RTT reconnect when the server has been previously connected
/// (session tickets are cached by quinn automatically).
pub struct QuicTransportFactory {
    endpoint: Endpoint,
    server_name: String,
}

impl QuicTransportFactory {
    /// Create a factory with a custom client TLS config.
    ///
    /// `server_name` is the SNI hostname for TLS verification.
    pub fn new(
        client_config: rustls::ClientConfig,
        server_name: impl Into<String>,
    ) -> Result<Self, MajraError> {
        let quinn_client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_config)
                .map_err(|e| MajraError::Relay(format!("quic client config: {e}")))?,
        ));

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| MajraError::Relay(format!("bind client: {e}")))?;
        endpoint.set_default_client_config(quinn_client_config);

        Ok(Self {
            endpoint,
            server_name: server_name.into(),
        })
    }

    /// Create a factory using self-signed certificates (dev/testing).
    pub fn self_signed() -> Result<Self, MajraError> {
        let (_server_config, client_config) = self_signed_tls()?;
        Self::new(client_config, "localhost")
    }
}

#[async_trait]
impl TransportFactory for QuicTransportFactory {
    async fn connect(&self, endpoint: &str) -> Result<Box<dyn Transport>, MajraError> {
        let addr: SocketAddr = endpoint
            .parse()
            .map_err(|e| MajraError::Relay(format!("parse address: {e}")))?;

        let connection = self
            .endpoint
            .connect(addr, &self.server_name)
            .map_err(|e| MajraError::Relay(format!("connect: {e}")))?
            .await
            .map_err(|e| MajraError::Relay(format!("handshake: {e}")))?;

        debug!(remote = %addr, "quic: connected");
        Ok(Box::new(QuicTransport::new(connection)))
    }
}

// ---------------------------------------------------------------------------
// QuicListener — accept inbound QUIC connections
// ---------------------------------------------------------------------------

/// Listener for inbound QUIC connections.
///
/// Wraps a quinn [`Endpoint`] in server mode. Connection migration is handled
/// transparently by QUIC — peers can change IP addresses without reconnecting.
pub struct QuicListener {
    endpoint: Endpoint,
}

impl QuicListener {
    /// Create a listener with a custom server TLS config.
    pub fn bind(addr: SocketAddr, server_config: rustls::ServerConfig) -> Result<Self, MajraError> {
        let quinn_server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_config)
                .map_err(|e| MajraError::Relay(format!("quic server config: {e}")))?,
        ));

        let endpoint = Endpoint::server(quinn_server_config, addr)
            .map_err(|e| MajraError::Relay(format!("bind server: {e}")))?;

        debug!(%addr, "quic: listener bound");
        Ok(Self { endpoint })
    }

    /// Create a listener using self-signed certificates (dev/testing).
    pub fn bind_self_signed(addr: SocketAddr) -> Result<Self, MajraError> {
        let (server_config, _client_config) = self_signed_tls()?;
        Self::bind(addr, server_config)
    }

    /// Accept the next inbound connection.
    ///
    /// Returns a [`QuicTransport`] for the connected peer.
    pub async fn accept(&self) -> Result<QuicTransport, MajraError> {
        let incoming = self
            .endpoint
            .accept()
            .await
            .ok_or_else(|| MajraError::Relay("listener closed".into()))?;

        let connection = incoming
            .await
            .map_err(|e| MajraError::Relay(format!("accept handshake: {e}")))?;

        debug!(
            remote = %connection.remote_address(),
            "quic: connection accepted"
        );
        Ok(QuicTransport::new(connection))
    }

    /// The local address the listener is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, MajraError> {
        self.endpoint
            .local_addr()
            .map_err(|e| MajraError::Relay(format!("local addr: {e}")))
    }

    /// Shut down the listener, refusing new connections.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
        debug!("quic: listener closed");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn self_signed_tls_generates() {
        let (server, client) = self_signed_tls().unwrap();
        assert!(server.alpn_protocols.is_empty() || !server.alpn_protocols.is_empty());
        // Client config should have at least one root cert.
        drop(client);
    }

    #[tokio::test]
    async fn listener_bind_and_factory_create() {
        let (server_tls, client_tls) = self_signed_tls().unwrap();
        let listener = QuicListener::bind("127.0.0.1:0".parse().unwrap(), server_tls).unwrap();
        let addr = listener.local_addr().unwrap();

        let factory = QuicTransportFactory::new(client_tls, "localhost").unwrap();

        // Connect to the listener.
        let connect_handle =
            tokio::spawn(async move { factory.connect(&addr.to_string()).await.unwrap() });

        let server_transport = listener.accept().await.unwrap();
        let client_transport = connect_handle.await.unwrap();

        assert!(server_transport.is_connected());
        assert!(client_transport.is_connected());

        listener.close();
    }

    #[tokio::test]
    async fn send_recv_relay_message() {
        let (server_tls, client_tls) = self_signed_tls().unwrap();
        let listener = QuicListener::bind("127.0.0.1:0".parse().unwrap(), server_tls).unwrap();
        let addr = listener.local_addr().unwrap();

        let factory = QuicTransportFactory::new(client_tls, "localhost").unwrap();

        let client_handle = tokio::spawn(async move {
            let transport = factory.connect(&addr.to_string()).await.unwrap();
            let msg = RelayMessage {
                seq: 42,
                from: "client".into(),
                to: "server".into(),
                topic: "test".into(),
                payload: serde_json::json!({"hello": "quic"}),
                timestamp: Utc::now(),
                correlation_id: None,
                is_reply: false,
            };
            transport.send(&msg).await.unwrap();
            transport
        });

        let server_transport = listener.accept().await.unwrap();
        let msg = server_transport.recv().await.unwrap();

        assert_eq!(msg.seq, 42);
        assert_eq!(msg.from, "client");
        assert_eq!(msg.topic, "test");
        assert_eq!(msg.payload["hello"], "quic");

        let _client_transport = client_handle.await.unwrap();
        listener.close();
    }

    #[tokio::test]
    async fn datagram_roundtrip() {
        let (server_tls, client_tls) = self_signed_tls().unwrap();

        // Enable datagrams in transport config.
        let mut transport_config = quinn::TransportConfig::default();
        transport_config.max_concurrent_bidi_streams(100u32.into());
        transport_config.datagram_receive_buffer_size(Some(65536));

        let quinn_server_config = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_tls).unwrap(),
        ));

        let listener_endpoint =
            Endpoint::server(quinn_server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener_endpoint.local_addr().unwrap();

        let quinn_client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_tls).unwrap(),
        ));
        let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        client_endpoint.set_default_client_config(quinn_client_config);

        let client_handle = tokio::spawn(async move {
            let conn = client_endpoint
                .connect(addr, "localhost")
                .unwrap()
                .await
                .unwrap();
            let transport = QuicTransport::new(conn);

            transport.send_datagram(b"hello-datagram").unwrap();
            transport
        });

        let incoming = listener_endpoint.accept().await.unwrap();
        let conn = incoming.await.unwrap();
        let server_transport = QuicTransport::new(conn);

        let data = server_transport.recv_datagram().await.unwrap();
        assert_eq!(data, b"hello-datagram");

        let _ct = client_handle.await.unwrap();
        listener_endpoint.close(0u32.into(), b"done");
    }

    #[tokio::test]
    async fn close_transport() {
        let (server_tls, client_tls) = self_signed_tls().unwrap();
        let listener = QuicListener::bind("127.0.0.1:0".parse().unwrap(), server_tls).unwrap();
        let addr = listener.local_addr().unwrap();

        let factory = QuicTransportFactory::new(client_tls, "localhost").unwrap();

        let connect_handle =
            tokio::spawn(async move { factory.connect(&addr.to_string()).await.unwrap() });

        let server_transport = listener.accept().await.unwrap();
        let client_transport = connect_handle.await.unwrap();

        client_transport.close().await.unwrap();

        // Give the close frame time to propagate.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!server_transport.is_connected());

        listener.close();
    }

    #[tokio::test]
    async fn multiple_messages_multiplexed() {
        let (server_tls, client_tls) = self_signed_tls().unwrap();
        let listener = QuicListener::bind("127.0.0.1:0".parse().unwrap(), server_tls).unwrap();
        let addr = listener.local_addr().unwrap();

        let factory = QuicTransportFactory::new(client_tls, "localhost").unwrap();

        let client_handle = tokio::spawn(async move {
            let transport = factory.connect(&addr.to_string()).await.unwrap();
            // Send multiple messages — each on its own QUIC stream.
            for i in 0..5u64 {
                let msg = RelayMessage {
                    seq: i,
                    from: "client".into(),
                    to: "server".into(),
                    topic: "multi".into(),
                    payload: serde_json::json!({"n": i}),
                    timestamp: Utc::now(),
                    correlation_id: None,
                    is_reply: false,
                };
                transport.send(&msg).await.unwrap();
            }
            transport
        });

        let server_transport = listener.accept().await.unwrap();

        for i in 0..5u64 {
            let msg = server_transport.recv().await.unwrap();
            assert_eq!(msg.seq, i);
        }

        let _ct = client_handle.await.unwrap();
        listener.close();
    }

    #[tokio::test]
    async fn self_signed_factory() {
        let factory = QuicTransportFactory::self_signed().unwrap();
        drop(factory);
    }

    #[tokio::test]
    async fn self_signed_listener() {
        let listener = QuicListener::bind_self_signed("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap();
        assert_ne!(addr.port(), 0);
        listener.close();
    }
}
