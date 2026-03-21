//! Structured logging and tracing initialisation.
//!
//! Provides [`init`] to set up a `tracing-subscriber` with env-based filtering
//! via the `MAJRA_LOG` environment variable. Requires the `logging` feature.
//!
//! # Log levels
//!
//! Set `MAJRA_LOG` to control verbosity:
//!
//! | Value | Shows |
//! |-------|-------|
//! | `error` | Errors only |
//! | `warn` | Warnings and above |
//! | `info` | Lifecycle events (register, dequeue, complete) |
//! | `debug` | Detailed operations (heartbeat transitions, dedup) |
//! | `trace` | Per-message publish/receive (high volume) |
//!
//! Supports per-module filtering: `MAJRA_LOG=majra::queue=debug,majra::relay=trace`
//!
//! # Example
//!
//! ```rust,no_run
//! // At the start of your application:
//! majra::logging::init();
//!
//! // Or with a specific default level:
//! majra::logging::init_with_level("debug");
//! ```

/// Initialise majra logging with the `MAJRA_LOG` environment variable.
///
/// Falls back to `info` if `MAJRA_LOG` is not set.
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn init() {
    init_with_level("info");
}

/// Initialise majra logging with a specific default level.
///
/// The `MAJRA_LOG` environment variable overrides `default_level` if set.
/// Safe to call multiple times — subsequent calls are no-ops.
pub fn init_with_level(default_level: &str) {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    let filter =
        EnvFilter::try_from_env("MAJRA_LOG").unwrap_or_else(|_| EnvFilter::new(default_level));

    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_target(true).with_thread_ids(true))
        .with(filter)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_does_not_panic() {
        init();
        // Second call is a no-op.
        init();
    }

    #[test]
    fn init_with_level_does_not_panic() {
        init_with_level("trace");
        init_with_level("error");
    }
}
