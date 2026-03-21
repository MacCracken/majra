//! Multi-tier priority queue with DAG dependency scheduling.
//!
//! The queue supports two modes:
//! 1. **Priority FIFO** — 5-tier priority levels, dequeue pops from highest tier first.
//! 2. **DAG scheduling** — tasks respect a dependency graph; `ready_tasks()` returns
//!    only those whose predecessors have completed.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MajraError;

/// Task priority tiers (highest = 4, lowest = 0).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[repr(u8)]
pub enum Priority {
    Background = 0,
    Low = 1,
    #[default]
    Normal = 2,
    High = 3,
    Critical = 4,
}

/// A unique task identifier.
pub type TaskId = Uuid;

/// A schedulable work item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem<T> {
    pub id: TaskId,
    pub priority: Priority,
    pub payload: T,
}

impl<T> QueueItem<T> {
    pub fn new(priority: Priority, payload: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            priority,
            payload,
        }
    }
}

/// A DAG specification: nodes and their dependency edges.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dag {
    /// Node key → list of keys this node depends on.
    pub edges: HashMap<String, Vec<String>>,
}

/// Multi-tier priority queue.
pub struct PriorityQueue<T> {
    tiers: [VecDeque<QueueItem<T>>; 5],
}

impl<T> PriorityQueue<T> {
    pub fn new() -> Self {
        Self {
            tiers: Default::default(),
        }
    }

    /// Push an item into its priority tier.
    pub fn enqueue(&mut self, item: QueueItem<T>) {
        self.tiers[item.priority as usize].push_back(item);
    }

    /// Pop the highest-priority item.
    pub fn dequeue(&mut self) -> Option<QueueItem<T>> {
        for tier in self.tiers.iter_mut().rev() {
            if let Some(item) = tier.pop_front() {
                return Some(item);
            }
        }
        None
    }

    /// Total items across all tiers.
    pub fn len(&self) -> usize {
        self.tiers.iter().map(VecDeque::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.tiers.iter().all(VecDeque::is_empty)
    }
}

impl<T> Default for PriorityQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// DAG-aware scheduler that tracks dependencies and emits ready items.
pub struct DagScheduler {
    /// Reverse edges: node → set of nodes it depends on.
    dependencies: HashMap<String, HashSet<String>>,
}

impl DagScheduler {
    /// Load a DAG, validating that it is acyclic.
    pub fn new(dag: &Dag) -> crate::error::Result<Self> {
        // Validate acyclicity via topological sort.
        Self::topological_sort(dag)?;

        let mut dependencies: HashMap<String, HashSet<String>> = HashMap::new();

        for (node, deps) in &dag.edges {
            dependencies
                .entry(node.clone())
                .or_default()
                .extend(deps.iter().cloned());

            // Ensure every referenced dep node exists in the map.
            for dep in deps {
                dependencies.entry(dep.clone()).or_default();
            }
        }

        Ok(Self { dependencies })
    }

    /// Return node keys whose dependencies have all been completed.
    pub fn ready(&self, completed: &HashSet<String>) -> Vec<String> {
        let mut ready = Vec::new();
        for (node, deps) in &self.dependencies {
            if !completed.contains(node) && deps.iter().all(|d| completed.contains(d)) {
                ready.push(node.clone());
            }
        }
        ready.sort();
        ready
    }

    /// Kahn's algorithm — returns topological order or error on cycle.
    pub fn topological_sort(dag: &Dag) -> crate::error::Result<Vec<String>> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

        // Collect all nodes.
        for (node, deps) in &dag.edges {
            in_degree.entry(node.as_str()).or_insert(0);
            adjacency.entry(node.as_str()).or_default();
            for dep in deps {
                in_degree.entry(dep.as_str()).or_insert(0);
                adjacency
                    .entry(dep.as_str())
                    .or_default()
                    .push(node.as_str());
                *in_degree.entry(node.as_str()).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|&(_, deg)| *deg == 0)
            .map(|(&k, _)| k)
            .collect();

        // Deterministic output.
        let mut queue_sorted: Vec<&str> = queue.drain(..).collect();
        queue_sorted.sort();
        queue.extend(queue_sorted);

        let mut order = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node.to_string());
            if let Some(neighbors) = adjacency.get(node) {
                let mut next = Vec::new();
                for &n in neighbors {
                    let deg = in_degree.get_mut(n).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        next.push(n);
                    }
                }
                next.sort();
                queue.extend(next);
            }
        }

        if order.len() != in_degree.len() {
            return Err(MajraError::DagCycle(
                "dependency graph contains a cycle".into(),
            ));
        }

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering() {
        let mut q = PriorityQueue::new();
        q.enqueue(QueueItem::new(Priority::Low, "low"));
        q.enqueue(QueueItem::new(Priority::Critical, "crit"));
        q.enqueue(QueueItem::new(Priority::Normal, "norm"));

        assert_eq!(q.dequeue().unwrap().payload, "crit");
        assert_eq!(q.dequeue().unwrap().payload, "norm");
        assert_eq!(q.dequeue().unwrap().payload, "low");
        assert!(q.is_empty());
    }

    #[test]
    fn fifo_within_tier() {
        let mut q = PriorityQueue::new();
        q.enqueue(QueueItem::new(Priority::Normal, "first"));
        q.enqueue(QueueItem::new(Priority::Normal, "second"));

        assert_eq!(q.dequeue().unwrap().payload, "first");
        assert_eq!(q.dequeue().unwrap().payload, "second");
    }

    #[test]
    fn dag_ready_tasks() {
        let dag = Dag {
            edges: HashMap::from([
                ("a".into(), vec![]),
                ("b".into(), vec!["a".into()]),
                ("c".into(), vec!["a".into()]),
                ("d".into(), vec!["b".into(), "c".into()]),
            ]),
        };
        let sched = DagScheduler::new(&dag).unwrap();

        let ready = sched.ready(&HashSet::new());
        assert_eq!(ready, vec!["a"]);

        let ready = sched.ready(&HashSet::from(["a".into()]));
        assert_eq!(ready, vec!["b", "c"]);

        let ready = sched.ready(&HashSet::from(["a".into(), "b".into(), "c".into()]));
        assert_eq!(ready, vec!["d"]);
    }

    #[test]
    fn dag_cycle_detected() {
        let dag = Dag {
            edges: HashMap::from([
                ("a".into(), vec!["b".into()]),
                ("b".into(), vec!["a".into()]),
            ]),
        };
        assert!(DagScheduler::new(&dag).is_err());
    }

    #[test]
    fn topological_sort_linear() {
        let dag = Dag {
            edges: HashMap::from([
                ("a".into(), vec![]),
                ("b".into(), vec!["a".into()]),
                ("c".into(), vec!["b".into()]),
            ]),
        };
        let order = DagScheduler::topological_sort(&dag).unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn queue_len() {
        let mut q: PriorityQueue<&str> = PriorityQueue::new();
        assert_eq!(q.len(), 0);
        q.enqueue(QueueItem::new(Priority::High, "x"));
        assert_eq!(q.len(), 1);
    }
}
