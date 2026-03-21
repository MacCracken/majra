//! Shared error types for majra.

use thiserror::Error;

/// Top-level error type for majra operations.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MajraError {
    #[error("queue: {0}")]
    Queue(String),

    #[error("pubsub: {0}")]
    PubSub(String),

    #[error("relay: {0}")]
    Relay(String),

    #[error("ipc: {0}")]
    Ipc(#[from] IpcError),

    #[error("heartbeat: {0}")]
    Heartbeat(String),

    #[error("barrier: {0}")]
    Barrier(String),

    #[error("dag cycle detected: {0}")]
    DagCycle(String),

    #[error("capacity exceeded: {0}")]
    CapacityExceeded(String),

    #[error("invalid state transition: {0}")]
    InvalidStateTransition(String),

    #[error("resource unavailable: {0}")]
    ResourceUnavailable(String),

    #[cfg(feature = "sqlite")]
    #[error("persistence: {0}")]
    Persistence(String),
}

/// IPC-specific errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum IpcError {
    #[error("frame too large: {size} bytes (max {max})")]
    FrameTooLarge { size: u32, max: u32 },

    #[error("connection closed")]
    ConnectionClosed,

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, MajraError>;

#[cfg(feature = "sqlite")]
impl From<rusqlite::Error> for MajraError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Persistence(e.to_string())
    }
}

/// Helper for converting any error into `MajraError::Persistence`.
#[cfg(feature = "sqlite")]
pub(crate) fn persistence_err(e: impl std::fmt::Display) -> MajraError {
    MajraError::Persistence(e.to_string())
}
