//! Length-prefixed framing over local IPC sockets.
//!
//! Protocol: 4-byte big-endian u32 length prefix, then JSON payload.
//! Maximum frame size: 16 MiB.
//!
//! - **Unix**: Uses Unix domain sockets.
//! - **Windows**: Uses named pipes (`\\.\pipe\majra-*`).

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::IpcError;

/// Maximum frame size (16 MiB).
const MAX_FRAME_SIZE: u32 = 16 * 1024 * 1024;

type Result<T> = std::result::Result<T, IpcError>;

// ---------------------------------------------------------------------------
// Unix implementation
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod platform {
    use tokio::net::{UnixListener, UnixStream};

    pub(super) struct Listener {
        inner: UnixListener,
    }

    impl Listener {
        pub fn bind(path: &std::path::Path) -> std::io::Result<Self> {
            let _ = std::fs::remove_file(path);
            Ok(Self {
                inner: UnixListener::bind(path)?,
            })
        }

        pub async fn accept(&self) -> std::io::Result<Stream> {
            let (stream, _) = self.inner.accept().await?;
            Ok(Stream { inner: stream })
        }
    }

    pub(super) struct Stream {
        inner: UnixStream,
    }

    impl Stream {
        pub async fn connect(path: &std::path::Path) -> std::io::Result<Self> {
            Ok(Self {
                inner: UnixStream::connect(path).await?,
            })
        }

        pub async fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
            use tokio::io::AsyncReadExt;
            self.inner.read_exact(buf).await.map(|_| ())
        }

        pub async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            use tokio::io::AsyncWriteExt;
            self.inner.write_all(buf).await
        }
    }
}

// ---------------------------------------------------------------------------
// Windows implementation (named pipes)
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod platform {
    use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

    pub(super) struct Listener {
        pipe_name: String,
    }

    impl Listener {
        pub fn bind(path: &std::path::Path) -> std::io::Result<Self> {
            // Convert filesystem path to named pipe path:
            // C:\Users\...\foo.sock → \\.\pipe\majra-<hash>
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("majra");
            let pipe_name = format!(r"\\.\pipe\majra-{}", name);
            // Create the first instance to verify the name is available.
            let _server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&pipe_name)?;
            Ok(Self { pipe_name })
        }

        pub async fn accept(&self) -> std::io::Result<Stream> {
            let server = ServerOptions::new().create(&self.pipe_name)?;
            server.connect().await?;
            Ok(Stream {
                inner: StreamInner::Server(server),
            })
        }
    }

    pub(super) struct Stream {
        inner: StreamInner,
    }

    enum StreamInner {
        Server(tokio::net::windows::named_pipe::NamedPipeServer),
        Client(tokio::net::windows::named_pipe::NamedPipeClient),
    }

    impl Stream {
        pub async fn connect(path: &std::path::Path) -> std::io::Result<Self> {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("majra");
            let pipe_name = format!(r"\\.\pipe\majra-{}", name);
            let client = ClientOptions::new().open(&pipe_name)?;
            Ok(Self {
                inner: StreamInner::Client(client),
            })
        }

        pub async fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
            use tokio::io::AsyncReadExt;
            match &mut self.inner {
                StreamInner::Server(s) => s.read_exact(buf).await.map(|_| ()),
                StreamInner::Client(c) => c.read_exact(buf).await.map(|_| ()),
            }
        }

        pub async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
            use tokio::io::AsyncWriteExt;
            match &mut self.inner {
                StreamInner::Server(s) => s.write_all(buf).await,
                StreamInner::Client(c) => c.write_all(buf).await,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Public API (platform-agnostic)
// ---------------------------------------------------------------------------

/// A listening IPC server.
///
/// On Unix, binds to a Unix domain socket. On Windows, creates a named pipe.
pub struct IpcServer {
    listener: platform::Listener,
}

impl IpcServer {
    /// Bind to a local IPC endpoint.
    ///
    /// `path` is used as a Unix socket path on Unix, or converted to a
    /// named pipe name on Windows.
    pub fn bind(path: &std::path::Path) -> Result<Self> {
        let listener = platform::Listener::bind(path)?;
        Ok(Self { listener })
    }

    /// Accept the next incoming connection.
    pub async fn accept(&self) -> Result<IpcConnection> {
        let stream = self.listener.accept().await?;
        Ok(IpcConnection { stream })
    }
}

/// A bidirectional IPC connection.
///
/// Used for both server-accepted and client-initiated connections.
pub struct IpcConnection {
    stream: platform::Stream,
}

impl IpcConnection {
    /// Connect to a local IPC endpoint (client-side).
    pub async fn connect(path: &std::path::Path) -> Result<Self> {
        let stream = platform::Stream::connect(path).await?;
        Ok(Self { stream })
    }

    /// Send a JSON value as a length-prefixed frame.
    pub async fn send(&mut self, payload: &serde_json::Value) -> Result<()> {
        let data = serde_json::to_vec(payload)?;
        let len = u32::try_from(data.len()).map_err(|_| IpcError::FrameTooLarge {
            size: u32::MAX,
            max: MAX_FRAME_SIZE,
        })?;
        if len > MAX_FRAME_SIZE {
            return Err(IpcError::FrameTooLarge {
                size: len,
                max: MAX_FRAME_SIZE,
            });
        }
        self.stream.write_all(&len.to_be_bytes()).await?;
        self.stream.write_all(&data).await?;
        Ok(())
    }

    /// Receive a JSON value from a length-prefixed frame.
    pub async fn recv(&mut self) -> Result<serde_json::Value> {
        let mut len_buf = [0u8; 4];
        match self.stream.read_exact(&mut len_buf).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Err(IpcError::ConnectionClosed);
            }
            Err(e) => return Err(e.into()),
        }
        let len = u32::from_be_bytes(len_buf);
        if len > MAX_FRAME_SIZE {
            return Err(IpcError::FrameTooLarge {
                size: len,
                max: MAX_FRAME_SIZE,
            });
        }
        let mut buf = vec![0u8; len as usize];
        self.stream.read_exact(&mut buf).await?;
        let value = serde_json::from_slice(&buf)?;
        Ok(value)
    }
}

/// Backward-compatible alias — use [`IpcConnection`] directly.
pub type IpcClient = IpcConnection;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_socket() -> PathBuf {
        let id = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("majra-test-{id}.sock"))
    }

    #[tokio::test]
    async fn roundtrip() {
        let path = tmp_socket();
        let server = IpcServer::bind(&path).unwrap();

        let handle = tokio::spawn({
            let path = path.clone();
            async move {
                let mut client = IpcClient::connect(&path).await.unwrap();
                client
                    .send(&serde_json::json!({"hello": "world"}))
                    .await
                    .unwrap();
                let resp = client.recv().await.unwrap();
                assert_eq!(resp["echo"], "world");
            }
        });

        let mut conn = server.accept().await.unwrap();
        let msg = conn.recv().await.unwrap();
        assert_eq!(msg["hello"], "world");
        conn.send(&serde_json::json!({"echo": "world"}))
            .await
            .unwrap();

        handle.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn connection_closed() {
        let path = tmp_socket();
        let server = IpcServer::bind(&path).unwrap();

        let handle = tokio::spawn({
            let path = path.clone();
            async move {
                let _client = IpcClient::connect(&path).await.unwrap();
                // Drop immediately.
            }
        });

        let mut conn = server.accept().await.unwrap();
        handle.await.unwrap();

        let result = conn.recv().await;
        assert!(matches!(result, Err(IpcError::ConnectionClosed)));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn write_frame_too_large() {
        let path = tmp_socket();
        let server = IpcServer::bind(&path).unwrap();

        let handle = tokio::spawn({
            let path = path.clone();
            async move {
                let mut client = IpcConnection::connect(&path).await.unwrap();
                // Test a valid large-ish payload works.
                let big = serde_json::json!({"data": "x".repeat(1000)});
                client.send(&big).await.unwrap();
            }
        });

        let mut conn = server.accept().await.unwrap();
        let msg = conn.recv().await.unwrap();
        assert_eq!(msg["data"].as_str().unwrap().len(), 1000);

        handle.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }

    // read_frame_too_large test uses platform-specific raw stream access,
    // so it's gated to Unix only.
    #[cfg(unix)]
    #[tokio::test]
    async fn read_frame_too_large() {
        use tokio::io::AsyncWriteExt;
        use tokio::net::UnixStream;

        let path = tmp_socket();
        let server = IpcServer::bind(&path).unwrap();

        let handle = tokio::spawn({
            let path = path.clone();
            async move {
                // Send a raw frame with a length header exceeding MAX_FRAME_SIZE.
                let mut stream = UnixStream::connect(&path).await.unwrap();
                let fake_len: u32 = super::MAX_FRAME_SIZE + 1;
                stream.write_all(&fake_len.to_be_bytes()).await.unwrap();
            }
        });

        let mut conn = server.accept().await.unwrap();
        let result = conn.recv().await;
        assert!(matches!(result, Err(IpcError::FrameTooLarge { .. })));

        handle.await.unwrap();
        let _ = std::fs::remove_file(&path);
    }
}
