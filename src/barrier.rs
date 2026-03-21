//! N-way barrier synchronisation with deadlock recovery.
//!
//! A barrier waits for N participants to arrive before releasing all of them.
//! Supports `force()` to unblock a barrier when a participant is known dead.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

/// Result of arriving at a barrier.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum BarrierResult {
    /// Still waiting for more participants.
    Waiting { arrived: usize, expected: usize },
    /// All participants have arrived.
    Released,
    /// Unknown barrier name.
    Unknown,
}

/// State for a single named barrier.
#[derive(Debug)]
struct BarrierState {
    expected: HashSet<String>,
    arrived: HashSet<String>,
    was_forced: bool,
}

/// Manages multiple named barriers.
pub struct BarrierSet {
    barriers: HashMap<String, BarrierState>,
}

/// Persistent record of barrier completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BarrierRecord {
    pub name: String,
    pub participants: Vec<String>,
    pub forced: bool,
}

impl BarrierSet {
    pub fn new() -> Self {
        Self {
            barriers: HashMap::new(),
        }
    }

    /// Create a barrier that waits for the given set of participants.
    pub fn create(&mut self, name: impl Into<String>, participants: HashSet<String>) {
        let name = name.into();
        self.barriers.insert(
            name,
            BarrierState {
                expected: participants,
                arrived: HashSet::new(),
                was_forced: false,
            },
        );
    }

    /// Record a participant's arrival at a barrier.
    pub fn arrive(&mut self, barrier_name: &str, participant: &str) -> BarrierResult {
        let Some(state) = self.barriers.get_mut(barrier_name) else {
            return BarrierResult::Unknown;
        };

        state.arrived.insert(participant.to_string());

        if state.arrived.is_superset(&state.expected) {
            BarrierResult::Released
        } else {
            BarrierResult::Waiting {
                arrived: state.arrived.len(),
                expected: state.expected.len(),
            }
        }
    }

    /// Force a barrier to release, removing a dead participant from the expected set.
    ///
    /// Returns `Released` if the barrier is now satisfied, `Waiting` otherwise,
    /// or `Unknown` if the barrier doesn't exist.
    pub fn force(&mut self, barrier_name: &str, dead_participant: &str) -> BarrierResult {
        let Some(state) = self.barriers.get_mut(barrier_name) else {
            return BarrierResult::Unknown;
        };

        state.expected.remove(dead_participant);
        state.arrived.remove(dead_participant);
        state.was_forced = true;

        if state.arrived.is_superset(&state.expected) {
            BarrierResult::Released
        } else {
            BarrierResult::Waiting {
                arrived: state.arrived.len(),
                expected: state.expected.len(),
            }
        }
    }

    /// Remove a completed barrier and return a record.
    pub fn complete(&mut self, barrier_name: &str) -> Option<BarrierRecord> {
        let state = self.barriers.remove(barrier_name)?;
        Some(BarrierRecord {
            name: barrier_name.to_string(),
            participants: state.arrived.into_iter().collect(),
            forced: state.was_forced,
        })
    }

    /// Number of active barriers.
    pub fn len(&self) -> usize {
        self.barriers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.barriers.is_empty()
    }
}

impl Default for BarrierSet {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Thread-safe async barrier set
// ---------------------------------------------------------------------------

/// Backward-compatible alias — use [`AsyncBarrierSet`] directly.
pub type ConcurrentBarrierSet = AsyncBarrierSet;

/// Async state for a single named barrier.
struct AsyncBarrierState {
    expected: HashSet<String>,
    arrived: HashSet<String>,
    notify: Arc<Notify>,
    was_forced: bool,
}

/// Thread-safe barrier set with async support.
///
/// Supports both sync `arrive()` (returns [`BarrierResult`]) and async
/// `arrive_and_wait()` (blocks until the barrier releases).
/// Also available as the type alias [`ConcurrentBarrierSet`].
pub struct AsyncBarrierSet {
    barriers: DashMap<String, AsyncBarrierState>,
}

impl AsyncBarrierSet {
    pub fn new() -> Self {
        Self {
            barriers: DashMap::new(),
        }
    }

    /// Create a barrier that waits for the given set of participants.
    pub fn create(&self, name: impl Into<String>, participants: HashSet<String>) {
        self.barriers.insert(
            name.into(),
            AsyncBarrierState {
                expected: participants,
                arrived: HashSet::new(),
                notify: Arc::new(Notify::new()),
                was_forced: false,
            },
        );
    }

    /// Record a participant's arrival and wait until the barrier releases.
    ///
    /// Returns `Ok(())` when all participants have arrived,
    /// or `Err` if the barrier does not exist.
    pub async fn arrive_and_wait(
        &self,
        barrier_name: &str,
        participant: &str,
    ) -> Result<(), crate::error::MajraError> {
        let notify = {
            let mut state = self.barriers.get_mut(barrier_name).ok_or_else(|| {
                crate::error::MajraError::Barrier(format!("unknown barrier: {barrier_name}"))
            })?;

            state.arrived.insert(participant.to_string());

            if state.arrived.is_superset(&state.expected) {
                // We are the last — notify everyone.
                state.notify.notify_waiters();
                return Ok(());
            }

            state.notify.clone()
        };

        // Wait for the last participant to notify us.
        notify.notified().await;
        Ok(())
    }

    /// Record a participant's arrival without waiting.
    /// Returns `BarrierResult` for callers that don't want to block.
    pub fn arrive(&self, barrier_name: &str, participant: &str) -> BarrierResult {
        let Some(mut state) = self.barriers.get_mut(barrier_name) else {
            return BarrierResult::Unknown;
        };

        state.arrived.insert(participant.to_string());

        if state.arrived.is_superset(&state.expected) {
            state.notify.notify_waiters();
            BarrierResult::Released
        } else {
            BarrierResult::Waiting {
                arrived: state.arrived.len(),
                expected: state.expected.len(),
            }
        }
    }

    /// Force a barrier to release by removing a dead participant.
    pub fn force(&self, barrier_name: &str, dead_participant: &str) -> BarrierResult {
        let Some(mut state) = self.barriers.get_mut(barrier_name) else {
            return BarrierResult::Unknown;
        };

        state.expected.remove(dead_participant);
        state.arrived.remove(dead_participant);
        state.was_forced = true;

        if state.arrived.is_superset(&state.expected) {
            state.notify.notify_waiters();
            BarrierResult::Released
        } else {
            BarrierResult::Waiting {
                arrived: state.arrived.len(),
                expected: state.expected.len(),
            }
        }
    }

    /// Remove a completed barrier and return a record.
    pub fn complete(&self, barrier_name: &str) -> Option<BarrierRecord> {
        let (_, state) = self.barriers.remove(barrier_name)?;
        Some(BarrierRecord {
            name: barrier_name.to_string(),
            participants: state.arrived.into_iter().collect(),
            forced: state.was_forced,
        })
    }

    /// Number of active barriers.
    pub fn len(&self) -> usize {
        self.barriers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.barriers.is_empty()
    }
}

impl Default for AsyncBarrierSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn participants(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    // --- BarrierSet (original) ---

    #[test]
    fn basic_barrier() {
        let mut set = BarrierSet::new();
        set.create("sync-1", participants(&["a", "b", "c"]));

        assert_eq!(
            set.arrive("sync-1", "a"),
            BarrierResult::Waiting {
                arrived: 1,
                expected: 3
            }
        );
        assert_eq!(
            set.arrive("sync-1", "b"),
            BarrierResult::Waiting {
                arrived: 2,
                expected: 3
            }
        );
        assert_eq!(set.arrive("sync-1", "c"), BarrierResult::Released);
    }

    #[test]
    fn unknown_barrier() {
        let mut set = BarrierSet::new();
        assert_eq!(set.arrive("nope", "a"), BarrierResult::Unknown);
    }

    #[test]
    fn force_removes_dead_participant() {
        let mut set = BarrierSet::new();
        set.create("sync-1", participants(&["a", "b"]));

        set.arrive("sync-1", "a");
        // b is dead — force it out.
        assert_eq!(set.force("sync-1", "b"), BarrierResult::Released);
    }

    #[test]
    fn duplicate_arrival_is_idempotent() {
        let mut set = BarrierSet::new();
        set.create("sync-1", participants(&["a", "b"]));

        set.arrive("sync-1", "a");
        set.arrive("sync-1", "a");
        assert_eq!(
            set.arrive("sync-1", "a"),
            BarrierResult::Waiting {
                arrived: 1,
                expected: 2
            }
        );
    }

    #[test]
    fn complete_removes_barrier() {
        let mut set = BarrierSet::new();
        set.create("sync-1", participants(&["a"]));
        set.arrive("sync-1", "a");

        let record = set.complete("sync-1").unwrap();
        assert_eq!(record.name, "sync-1");
        assert!(set.is_empty());
        assert!(set.complete("sync-1").is_none());
    }

    // --- ConcurrentBarrierSet ---

    #[test]
    fn concurrent_basic_barrier() {
        let set = ConcurrentBarrierSet::new();
        set.create("sync-1", participants(&["a", "b", "c"]));

        assert_eq!(
            set.arrive("sync-1", "a"),
            BarrierResult::Waiting {
                arrived: 1,
                expected: 3
            }
        );
        assert_eq!(
            set.arrive("sync-1", "b"),
            BarrierResult::Waiting {
                arrived: 2,
                expected: 3
            }
        );
        assert_eq!(set.arrive("sync-1", "c"), BarrierResult::Released);
    }

    #[test]
    fn concurrent_unknown_barrier() {
        let set = ConcurrentBarrierSet::new();
        assert_eq!(set.arrive("nope", "a"), BarrierResult::Unknown);
    }

    #[test]
    fn concurrent_force() {
        let set = ConcurrentBarrierSet::new();
        set.create("sync-1", participants(&["a", "b"]));

        set.arrive("sync-1", "a");
        assert_eq!(set.force("sync-1", "b"), BarrierResult::Released);
    }

    #[test]
    fn concurrent_complete() {
        let set = ConcurrentBarrierSet::new();
        set.create("sync-1", participants(&["a"]));
        set.arrive("sync-1", "a");

        let record = set.complete("sync-1").unwrap();
        assert_eq!(record.name, "sync-1");
        assert!(set.is_empty());
    }

    #[test]
    fn concurrent_multi_thread_arrive() {
        use std::sync::Arc;
        use std::thread;

        let set = Arc::new(ConcurrentBarrierSet::new());
        let names: Vec<String> = (0..8).map(|i| format!("p-{i}")).collect();
        set.create("sync-mt", names.iter().cloned().collect());

        let mut handles = Vec::new();
        for name in &names {
            let s = set.clone();
            let n = name.clone();
            handles.push(thread::spawn(move || s.arrive("sync-mt", &n)));
        }

        let results: Vec<BarrierResult> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        // Exactly one thread should see Released.
        let released = results
            .iter()
            .filter(|r| **r == BarrierResult::Released)
            .count();
        assert_eq!(released, 1);
    }

    // --- AsyncBarrierSet ---

    #[tokio::test]
    async fn async_arrive_and_wait() {
        let set = Arc::new(AsyncBarrierSet::new());
        set.create("sync-1", participants(&["a", "b", "c"]));

        let mut handles = Vec::new();
        for name in ["a", "b", "c"] {
            let s = set.clone();
            let n = name.to_string();
            handles.push(tokio::spawn(async move {
                s.arrive_and_wait("sync-1", &n).await.unwrap();
            }));
        }

        // All three should complete (not deadlock).
        for h in handles {
            h.await.unwrap();
        }
    }

    #[tokio::test]
    async fn async_arrive_and_wait_with_delay() {
        let set = Arc::new(AsyncBarrierSet::new());
        set.create("sync-1", participants(&["a", "b"]));

        let s = set.clone();
        let waiter = tokio::spawn(async move {
            s.arrive_and_wait("sync-1", "a").await.unwrap();
        });

        // Small delay before second participant arrives.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        set.arrive("sync-1", "b");

        // Waiter should now be released.
        waiter.await.unwrap();
    }

    #[tokio::test]
    async fn async_unknown_barrier_error() {
        let set = AsyncBarrierSet::new();
        let result = set.arrive_and_wait("nope", "a").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn async_force_releases_waiters() {
        let set = Arc::new(AsyncBarrierSet::new());
        set.create("sync-1", participants(&["a", "b"]));

        let s = set.clone();
        let waiter = tokio::spawn(async move {
            s.arrive_and_wait("sync-1", "a").await.unwrap();
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        // Force remove b — should release the barrier.
        set.force("sync-1", "b");

        waiter.await.unwrap();
    }

    #[test]
    fn async_non_blocking_arrive() {
        let set = AsyncBarrierSet::new();
        set.create("sync-1", participants(&["a", "b"]));

        assert_eq!(
            set.arrive("sync-1", "a"),
            BarrierResult::Waiting {
                arrived: 1,
                expected: 2
            }
        );
        assert_eq!(set.arrive("sync-1", "b"), BarrierResult::Released);
    }
}
