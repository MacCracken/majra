//! N-way barrier synchronisation with deadlock recovery.
//!
//! A barrier waits for N participants to arrive before releasing all of them.
//! Supports `force()` to unblock a barrier when a participant is known dead.

use std::collections::{HashMap, HashSet};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

/// Result of arriving at a barrier.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        let forced = !state.arrived.is_superset(&state.expected);
        Some(BarrierRecord {
            name: barrier_name.to_string(),
            participants: state.arrived.into_iter().collect(),
            forced,
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
// Thread-safe variant
// ---------------------------------------------------------------------------

/// Thread-safe barrier set backed by [`DashMap`].
///
/// All methods take `&self` and are safe for concurrent use.
pub struct ConcurrentBarrierSet {
    barriers: DashMap<String, BarrierState>,
}

impl ConcurrentBarrierSet {
    pub fn new() -> Self {
        Self {
            barriers: DashMap::new(),
        }
    }

    /// Create a barrier that waits for the given set of participants.
    pub fn create(&self, name: impl Into<String>, participants: HashSet<String>) {
        self.barriers.insert(
            name.into(),
            BarrierState {
                expected: participants,
                arrived: HashSet::new(),
            },
        );
    }

    /// Record a participant's arrival at a barrier.
    pub fn arrive(&self, barrier_name: &str, participant: &str) -> BarrierResult {
        let Some(mut state) = self.barriers.get_mut(barrier_name) else {
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

    /// Force a barrier to release by removing a dead participant.
    pub fn force(&self, barrier_name: &str, dead_participant: &str) -> BarrierResult {
        let Some(mut state) = self.barriers.get_mut(barrier_name) else {
            return BarrierResult::Unknown;
        };

        state.expected.remove(dead_participant);
        state.arrived.remove(dead_participant);

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
    pub fn complete(&self, barrier_name: &str) -> Option<BarrierRecord> {
        let (_, state) = self.barriers.remove(barrier_name)?;
        let forced = !state.arrived.is_superset(&state.expected);
        Some(BarrierRecord {
            name: barrier_name.to_string(),
            participants: state.arrived.into_iter().collect(),
            forced,
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

impl Default for ConcurrentBarrierSet {
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
}
