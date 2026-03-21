//! Shared error types for majra.

use thiserror::Error;

/// Top-level error type for majra operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MajraError {
    /// Queue operation error.
    #[error("queue: {0}")]
    Queue(String),

    /// Pub/sub operation error.
    #[error("pubsub: {0}")]
    PubSub(String),

    /// Relay operation error.
    #[error("relay: {0}")]
    Relay(String),

    /// IPC framing error.
    #[error("ipc: {0}")]
    Ipc(#[from] IpcError),

    /// Heartbeat tracking error.
    #[error("heartbeat: {0}")]
    Heartbeat(String),

    /// Barrier synchronisation error.
    #[error("barrier: {0}")]
    Barrier(String),

    /// Dependency graph contains a cycle.
    #[error("dag cycle detected: {0}")]
    DagCycle(String),

    /// Resource capacity exceeded.
    #[error("capacity exceeded: {0}")]
    CapacityExceeded(String),

    /// Illegal job state transition (e.g. Completed → Running).
    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),

    /// Required resource is not available.
    #[error("resource unavailable: {0}")]
    ResourceUnavailable(String),

    /// SQLite persistence error (behind `sqlite` feature).
    #[cfg(feature = "sqlite")]
    #[error("persistence: {0}")]
    Persistence(String),
}

/// IPC-specific errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IpcError {
    /// Frame exceeds the maximum allowed size.
    #[error("frame too large: {size} bytes (max {max})")]
    FrameTooLarge {
        /// Actual frame size in bytes.
        size: u32,
        /// Maximum allowed frame size in bytes.
        max: u32,
    },

    /// The peer closed the connection.
    #[error("connection closed")]
    ConnectionClosed,

    /// Underlying I/O error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialisation/deserialisation error.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Convenience alias for `Result<T, MajraError>`.
pub type Result<T> = std::result::Result<T, MajraError>;

/// Helper for converting any error into `MajraError::Persistence`.
#[cfg(feature = "sqlite")]
pub(crate) fn persistence_err(e: impl std::fmt::Display) -> MajraError {
    MajraError::Persistence(e.to_string())
}
