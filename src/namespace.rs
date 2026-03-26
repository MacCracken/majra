//! Tenant-scoped wrappers for majra primitives.
//!
//! Provides [`Namespace`]-prefixed access to pub/sub, relay, and rate limiting
//! so multiple tenants can share a single process without cross-contamination.
//!
//! ```
//! use majra::namespace::Namespace;
//!
//! let ns = Namespace::new("tenant-42");
//! assert_eq!(ns.topic("events/created"), "tenant-42/events/created");
//! assert_eq!(ns.key("api_requests"), "tenant-42:api_requests");
//! ```

use std::fmt;

/// A namespace scope that prefixes topics, keys, and node IDs.
///
/// All majra primitives are process-global by design — callers compose them
/// freely. `Namespace` adds a thin isolation layer by rewriting identifiers:
///
/// - **Topics** (pub/sub, relay): `{namespace}/{topic}`
/// - **Keys** (rate limiter, queue): `{namespace}:{key}`
/// - **Node IDs** (relay, fleet): `{namespace}:{node_id}`
///
/// Consumers create one `Namespace` per tenant and pass prefixed identifiers
/// into the shared primitives.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Namespace {
    prefix: String,
}

impl Namespace {
    /// Create a new namespace with the given prefix.
    ///
    /// The prefix should not contain `/` or `:` — these are used as delimiters.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            prefix: prefix.into(),
        }
    }

    /// The raw prefix string.
    #[inline]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// Prefix a topic (for pub/sub and relay). Uses `/` delimiter.
    ///
    /// `events/created` → `tenant-42/events/created`
    #[must_use]
    pub fn topic(&self, topic: &str) -> String {
        format!("{}/{topic}", self.prefix)
    }

    /// Prefix a key (for rate limiter, queue names). Uses `:` delimiter.
    ///
    /// `api_requests` → `tenant-42:api_requests`
    #[must_use]
    pub fn key(&self, key: &str) -> String {
        format!("{}:{key}", self.prefix)
    }

    /// Prefix a node ID (for relay, fleet). Uses `:` delimiter.
    ///
    /// `gpu-node-1` → `tenant-42:gpu-node-1`
    #[must_use]
    pub fn node_id(&self, node_id: &str) -> String {
        format!("{}:{node_id}", self.prefix)
    }

    /// Strip this namespace's prefix from a topic, returning the bare topic.
    ///
    /// Returns `None` if the topic does not belong to this namespace.
    #[must_use]
    pub fn strip_topic<'a>(&self, topic: &'a str) -> Option<&'a str> {
        topic
            .strip_prefix(self.prefix.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
    }

    /// Strip this namespace's prefix from a key.
    ///
    /// Returns `None` if the key does not belong to this namespace.
    #[must_use]
    pub fn strip_key<'a>(&self, key: &'a str) -> Option<&'a str> {
        key.strip_prefix(self.prefix.as_str())
            .and_then(|rest| rest.strip_prefix(':'))
    }

    /// Build a subscription pattern scoped to this namespace.
    ///
    /// `events/*` → `tenant-42/events/*`
    #[must_use]
    pub fn pattern(&self, pattern: &str) -> String {
        format!("{}/{pattern}", self.prefix)
    }

    /// Build a wildcard pattern matching all topics in this namespace.
    ///
    /// Returns `tenant-42/#` for MQTT-style matching.
    #[must_use]
    pub fn wildcard(&self) -> String {
        format!("{}/#", self.prefix)
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_prefixing() {
        let ns = Namespace::new("tenant-42");
        assert_eq!(ns.topic("events/created"), "tenant-42/events/created");
        assert_eq!(ns.topic("a/b/c"), "tenant-42/a/b/c");
    }

    #[test]
    fn key_prefixing() {
        let ns = Namespace::new("tenant-42");
        assert_eq!(ns.key("api_requests"), "tenant-42:api_requests");
        assert_eq!(ns.key("user:123"), "tenant-42:user:123");
    }

    #[test]
    fn node_id_prefixing() {
        let ns = Namespace::new("tenant-42");
        assert_eq!(ns.node_id("gpu-node-1"), "tenant-42:gpu-node-1");
    }

    #[test]
    fn pattern_prefixing() {
        let ns = Namespace::new("org-1");
        assert_eq!(ns.pattern("events/*"), "org-1/events/*");
        assert_eq!(ns.pattern("workflow/#"), "org-1/workflow/#");
    }

    #[test]
    fn wildcard() {
        let ns = Namespace::new("org-1");
        assert_eq!(ns.wildcard(), "org-1/#");
    }

    #[test]
    fn strip_topic() {
        let ns = Namespace::new("tenant-42");
        assert_eq!(
            ns.strip_topic("tenant-42/events/created"),
            Some("events/created")
        );
        assert_eq!(ns.strip_topic("other/events/created"), None);
        assert_eq!(ns.strip_topic("tenant-42"), None);
    }

    #[test]
    fn strip_key() {
        let ns = Namespace::new("tenant-42");
        assert_eq!(ns.strip_key("tenant-42:api_requests"), Some("api_requests"));
        assert_eq!(ns.strip_key("other:api_requests"), None);
    }

    #[test]
    fn display() {
        let ns = Namespace::new("org-1");
        assert_eq!(format!("{ns}"), "org-1");
    }

    #[cfg(feature = "pubsub")]
    #[tokio::test]
    async fn namespaced_pubsub_isolation() {
        use crate::pubsub::PubSub;

        let hub = PubSub::new();
        let ns_a = Namespace::new("tenant-a");
        let ns_b = Namespace::new("tenant-b");

        let mut rx_a = hub.subscribe(&ns_a.pattern("events/#"));
        let mut rx_b = hub.subscribe(&ns_b.pattern("events/#"));

        // Publish to tenant-a's topic.
        hub.publish(&ns_a.topic("events/created"), serde_json::json!({"a": 1}));

        // tenant-a should receive it.
        let msg = rx_a.recv().await.unwrap();
        assert_eq!(ns_a.strip_topic(&msg.topic), Some("events/created"));

        // tenant-b should NOT receive it.
        assert!(rx_b.try_recv().is_err());
    }

    #[cfg(feature = "ratelimit")]
    #[test]
    fn namespaced_ratelimit_isolation() {
        use crate::ratelimit::RateLimiter;

        let limiter = RateLimiter::new(1.0, 1); // 1 token/sec, burst 1
        let ns_a = Namespace::new("tenant-a");
        let ns_b = Namespace::new("tenant-b");

        // tenant-a exhausts its token.
        assert!(limiter.check(&ns_a.key("api")));
        assert!(!limiter.check(&ns_a.key("api")));

        // tenant-b should still have its own token.
        assert!(limiter.check(&ns_b.key("api")));
    }
}
