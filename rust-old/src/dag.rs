//! DAG-based workflow engine with db-agnostic storage.
//!
//! Builds on the existing [`crate::queue::DagScheduler`] for cycle detection,
//! adding tier-based parallel scheduling, trigger modes, error policies,
//! retry with backoff, execution tracking, and pluggable storage.
//!
//! ## Feature flag
//!
//! Requires the `dag` feature (which implies `queue`).
//!
//! ## Architecture
//!
//! ```text
//! WorkflowDefinition ──▶ WorkflowEngine::execute()
//!                            │
//!                            ├─ validate (cycle check via DagScheduler)
//!                            ├─ topological_sort_tiers (Kahn's with TriggerMode)
//!                            ├─ create WorkflowRun in storage
//!                            └─ walk tiers
//!                                 └─ for each step (batched, parallel)
//!                                      ├─ retry loop
//!                                      ├─ StepExecutor::execute()
//!                                      ├─ error policy handling
//!                                      └─ update storage + context
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::error::MajraError;
use crate::metrics::{MajraMetrics, NoopMetrics};
use crate::queue::Dag;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// How a step triggers relative to its dependencies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TriggerMode {
    /// Step runs when **all** dependencies have completed.
    #[default]
    All,
    /// Step runs when **any single** dependency has completed.
    Any,
}

/// What happens when a step fails after exhausting retries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ErrorPolicy {
    /// Abort the entire workflow run.
    #[default]
    Fail,
    /// Mark the step as failed but continue downstream steps.
    Continue,
    /// Mark the step as skipped and continue.
    Skip,
    /// Execute the named fallback step, then continue.
    Fallback(String),
}

/// Status of a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum WorkflowRunStatus {
    /// Run is created but not yet started.
    Pending,
    /// Run is actively executing.
    Running,
    /// All steps completed successfully.
    Completed,
    /// Run aborted due to a step failure with `Fail` policy.
    Failed,
    /// Run was cancelled externally.
    Cancelled,
}

impl std::fmt::Display for WorkflowRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Status of an individual step execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StepRunStatus {
    /// Step is queued for execution.
    Pending,
    /// Step is currently executing.
    Running,
    /// Step completed successfully.
    Completed,
    /// Step failed after exhausting retries.
    Failed,
    /// Step was skipped (condition, trigger mode, or error policy).
    Skipped,
}

impl std::fmt::Display for StepRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

// ---------------------------------------------------------------------------
// Structs
// ---------------------------------------------------------------------------

/// Retry configuration for a workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of attempts (1 = no retry, max 10).
    pub max_attempts: u32,
    /// Base backoff between retries. Scaled by attempt number.
    pub backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            backoff: Duration::from_secs(1),
        }
    }
}

/// A single step (node) in a workflow DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    /// Unique identifier within the workflow.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// IDs of steps this step depends on.
    pub depends_on: Vec<String>,
    /// When to trigger relative to dependencies.
    #[serde(default)]
    pub trigger_mode: TriggerMode,
    /// Opaque configuration passed to the step executor.
    #[serde(default)]
    pub config: serde_json::Value,
    /// Error handling policy.
    #[serde(default)]
    pub error_policy: ErrorPolicy,
    /// Retry configuration.
    #[serde(default)]
    pub retry_policy: RetryPolicy,
}

/// A complete workflow definition (the DAG).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    /// Unique workflow identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Steps forming the DAG.
    pub steps: Vec<WorkflowStep>,
    /// Whether this workflow is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Monotonic version number.
    #[serde(default = "default_one")]
    pub version: u32,
    /// Who created this definition.
    #[serde(default = "default_system")]
    pub created_by: String,
    /// Creation timestamp (millis since epoch).
    pub created_at: i64,
    /// Last update timestamp (millis since epoch).
    pub updated_at: i64,
}

fn default_true() -> bool {
    true
}
fn default_one() -> u32 {
    1
}
fn default_system() -> String {
    "system".into()
}

/// A single execution of a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// Unique run identifier.
    pub id: String,
    /// Workflow definition ID.
    pub workflow_id: String,
    /// Workflow name (denormalized).
    pub workflow_name: String,
    /// Current run status.
    pub status: WorkflowRunStatus,
    /// Input data for this run.
    pub input: Option<serde_json::Value>,
    /// Aggregated output from all steps.
    pub output: Option<serde_json::Value>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Who or what triggered this run.
    pub triggered_by: String,
    /// Creation timestamp (millis since epoch).
    pub created_at: i64,
    /// When execution started.
    pub started_at: Option<i64>,
    /// When execution completed.
    pub completed_at: Option<i64>,
}

/// A single execution record for one step within a workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRun {
    /// Unique step run identifier.
    pub id: String,
    /// Parent workflow run ID.
    pub run_id: String,
    /// Step ID from the definition.
    pub step_id: String,
    /// Step name (denormalized).
    pub step_name: String,
    /// Current step status.
    pub status: StepRunStatus,
    /// Input passed to the executor.
    pub input: Option<serde_json::Value>,
    /// Output from the executor.
    pub output: Option<serde_json::Value>,
    /// Error message if failed.
    pub error: Option<String>,
    /// Which attempt produced this result (1-based).
    pub attempt: u32,
    /// When execution started.
    pub started_at: Option<i64>,
    /// When execution completed.
    pub completed_at: Option<i64>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: Option<i64>,
}

/// Result of a single step execution, stored in the workflow context.
#[derive(Debug, Clone)]
pub struct StepResult {
    /// Step output (if completed).
    pub output: Option<serde_json::Value>,
    /// Terminal status.
    pub status: StepRunStatus,
}

/// Accumulates step outputs during workflow execution so downstream steps
/// can reference upstream results.
#[derive(Debug, Clone, Default)]
pub struct WorkflowContext {
    /// Step ID → result.
    pub step_results: HashMap<String, StepResult>,
    /// Workflow-level input.
    pub input: serde_json::Value,
}

/// Configuration for the workflow engine.
#[derive(Debug, Clone)]
pub struct WorkflowEngineConfig {
    /// Maximum steps to execute in parallel within a single tier.
    pub max_parallel_steps: usize,
}

impl Default for WorkflowEngineConfig {
    fn default() -> Self {
        Self {
            max_parallel_steps: 20,
        }
    }
}

// ---------------------------------------------------------------------------
// Traits
// ---------------------------------------------------------------------------

/// Database-agnostic storage for workflow definitions, runs, and step runs.
#[async_trait]
pub trait WorkflowStorage: Send + Sync {
    // ── Definitions ──────────────────────────────────────────────

    /// Persist a new workflow definition.
    async fn create_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()>;

    /// Retrieve a workflow definition by ID.
    async fn get_definition(&self, id: &str) -> crate::error::Result<Option<WorkflowDefinition>>;

    /// Update an existing workflow definition.
    async fn update_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()>;

    /// Delete a workflow definition. Returns `true` if it existed.
    async fn delete_definition(&self, id: &str) -> crate::error::Result<bool>;

    /// List definitions with pagination.
    async fn list_definitions(
        &self,
        limit: usize,
        offset: usize,
    ) -> crate::error::Result<Vec<WorkflowDefinition>>;

    // ── Runs ─────────────────────────────────────────────────────

    /// Create a new workflow run record.
    async fn create_run(&self, run: &WorkflowRun) -> crate::error::Result<()>;

    /// Retrieve a workflow run by ID.
    async fn get_run(&self, id: &str) -> crate::error::Result<Option<WorkflowRun>>;

    /// Update a workflow run (status, output, timestamps).
    async fn update_run(&self, run: &WorkflowRun) -> crate::error::Result<()>;

    /// List runs, optionally filtered by workflow ID.
    async fn list_runs(
        &self,
        workflow_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> crate::error::Result<Vec<WorkflowRun>>;

    // ── Step Runs ────────────────────────────────────────────────

    /// Create a step run record.
    async fn create_step_run(&self, step_run: &StepRun) -> crate::error::Result<()>;

    /// Update a step run (status, output, error, timestamps).
    async fn update_step_run(&self, step_run: &StepRun) -> crate::error::Result<()>;

    /// Get all step runs for a given workflow run.
    async fn get_step_runs_for_run(&self, run_id: &str) -> crate::error::Result<Vec<StepRun>>;
}

/// User-provided logic to execute a single workflow step.
///
/// majra does **not** define step types (agent, tool, webhook, etc.) — that
/// is the consumer's domain. majra provides scheduling, retry, error handling,
/// and storage. The consumer implements this trait to run steps.
#[async_trait]
pub trait StepExecutor: Send + Sync {
    /// Execute a step with the given config and workflow context.
    ///
    /// Returns the step output on success, or an error description on failure.
    async fn execute(
        &self,
        step: &WorkflowStep,
        context: &WorkflowContext,
    ) -> Result<serde_json::Value, String>;
}

// ---------------------------------------------------------------------------
// In-memory storage
// ---------------------------------------------------------------------------

/// In-memory workflow storage backed by [`DashMap`].
///
/// Suitable for testing and single-process use. No persistence across restarts.
/// Set `max_runs` to cap the number of stored runs — once the limit is reached,
/// the oldest terminal run is evicted automatically on `create_run`.
#[derive(Debug)]
pub struct InMemoryWorkflowStorage {
    definitions: DashMap<String, WorkflowDefinition>,
    runs: DashMap<String, WorkflowRun>,
    step_runs: DashMap<String, StepRun>,
    /// Maximum number of run records to keep (0 = unbounded).
    max_runs: usize,
}

impl Default for InMemoryWorkflowStorage {
    fn default() -> Self {
        Self {
            definitions: DashMap::new(),
            runs: DashMap::new(),
            step_runs: DashMap::new(),
            max_runs: 0,
        }
    }
}

impl InMemoryWorkflowStorage {
    /// Create a new empty in-memory storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a maximum number of stored runs.
    ///
    /// When the limit is reached, the oldest terminal (completed/failed/cancelled)
    /// run is evicted on each new `create_run`. Set to `0` for unbounded.
    pub fn with_max_runs(max_runs: usize) -> Self {
        Self {
            max_runs,
            ..Default::default()
        }
    }

    /// Evict completed and failed workflow runs older than `max_age`.
    ///
    /// Also removes associated step runs. Only terminal runs
    /// (`Completed`, `Failed`, `Cancelled`) are eligible for eviction.
    ///
    /// Returns the number of runs evicted.
    pub fn evict_older_than(&self, max_age: Duration) -> usize {
        let max_age_ms = i64::try_from(max_age.as_millis()).unwrap_or(i64::MAX);
        let cutoff_ms = chrono::Utc::now()
            .timestamp_millis()
            .saturating_sub(max_age_ms);

        let stale_run_ids: Vec<String> = self
            .runs
            .iter()
            .filter(|entry| {
                let run = entry.value();
                matches!(
                    run.status,
                    WorkflowRunStatus::Completed
                        | WorkflowRunStatus::Failed
                        | WorkflowRunStatus::Cancelled
                ) && run.completed_at.is_some_and(|t| t < cutoff_ms)
            })
            .map(|entry| entry.key().clone())
            .collect();

        let count = stale_run_ids.len();
        for run_id in &stale_run_ids {
            self.runs.remove(run_id);
            // Remove associated step runs.
            let step_ids: Vec<String> = self
                .step_runs
                .iter()
                .filter(|e| e.value().run_id == *run_id)
                .map(|e| e.key().clone())
                .collect();
            for step_id in &step_ids {
                self.step_runs.remove(step_id);
            }
        }

        if count > 0 {
            tracing::debug!(count, "dag: evicted old workflow runs");
        }
        count
    }

    /// Number of workflow runs currently stored.
    #[inline]
    pub fn run_count(&self) -> usize {
        self.runs.len()
    }

    /// Number of step run records currently stored.
    #[inline]
    pub fn step_run_count(&self) -> usize {
        self.step_runs.len()
    }
}

#[async_trait]
impl WorkflowStorage for InMemoryWorkflowStorage {
    async fn create_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()> {
        self.definitions.insert(def.id.clone(), def.clone());
        Ok(())
    }

    async fn get_definition(&self, id: &str) -> crate::error::Result<Option<WorkflowDefinition>> {
        Ok(self.definitions.get(id).map(|v| v.value().clone()))
    }

    async fn update_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()> {
        self.definitions.insert(def.id.clone(), def.clone());
        Ok(())
    }

    async fn delete_definition(&self, id: &str) -> crate::error::Result<bool> {
        Ok(self.definitions.remove(id).is_some())
    }

    async fn list_definitions(
        &self,
        limit: usize,
        offset: usize,
    ) -> crate::error::Result<Vec<WorkflowDefinition>> {
        let all: Vec<WorkflowDefinition> =
            self.definitions.iter().map(|e| e.value().clone()).collect();
        Ok(all.into_iter().skip(offset).take(limit).collect())
    }

    async fn create_run(&self, run: &WorkflowRun) -> crate::error::Result<()> {
        // Auto-evict oldest terminal run if at capacity.
        if self.max_runs > 0 && self.runs.len() >= self.max_runs {
            let oldest = self
                .runs
                .iter()
                .filter(|e| {
                    matches!(
                        e.value().status,
                        WorkflowRunStatus::Completed
                            | WorkflowRunStatus::Failed
                            | WorkflowRunStatus::Cancelled
                    )
                })
                .min_by_key(|e| e.value().completed_at.unwrap_or(i64::MAX))
                .map(|e| e.key().clone());
            if let Some(old_id) = oldest {
                // Remove associated step runs.
                let step_ids: Vec<String> = self
                    .step_runs
                    .iter()
                    .filter(|e| e.value().run_id == old_id)
                    .map(|e| e.key().clone())
                    .collect();
                for sid in &step_ids {
                    self.step_runs.remove(sid);
                }
                self.runs.remove(&old_id);
                tracing::debug!(evicted = %old_id, "dag: evicted oldest run (capacity)");
            }
        }
        self.runs.insert(run.id.clone(), run.clone());
        Ok(())
    }

    async fn get_run(&self, id: &str) -> crate::error::Result<Option<WorkflowRun>> {
        Ok(self.runs.get(id).map(|v| v.value().clone()))
    }

    async fn update_run(&self, run: &WorkflowRun) -> crate::error::Result<()> {
        self.runs.insert(run.id.clone(), run.clone());
        Ok(())
    }

    async fn list_runs(
        &self,
        workflow_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> crate::error::Result<Vec<WorkflowRun>> {
        let all: Vec<WorkflowRun> = self
            .runs
            .iter()
            .filter(|e| {
                workflow_id
                    .map(|wid| e.value().workflow_id == wid)
                    .unwrap_or(true)
            })
            .map(|e| e.value().clone())
            .collect();
        Ok(all.into_iter().skip(offset).take(limit).collect())
    }

    async fn create_step_run(&self, step_run: &StepRun) -> crate::error::Result<()> {
        self.step_runs.insert(step_run.id.clone(), step_run.clone());
        Ok(())
    }

    async fn update_step_run(&self, step_run: &StepRun) -> crate::error::Result<()> {
        self.step_runs.insert(step_run.id.clone(), step_run.clone());
        Ok(())
    }

    async fn get_step_runs_for_run(&self, run_id: &str) -> crate::error::Result<Vec<StepRun>> {
        Ok(self
            .step_runs
            .iter()
            .filter(|e| e.value().run_id == run_id)
            .map(|e| e.value().clone())
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Workflow engine
// ---------------------------------------------------------------------------

/// DAG-based workflow execution engine.
///
/// Composes [`crate::queue::DagScheduler`] for cycle validation with
/// tier-based scheduling, trigger modes, retry policies, error handling,
/// and pluggable storage.
pub struct WorkflowEngine<S: WorkflowStorage, E: StepExecutor> {
    storage: Arc<S>,
    executor: Arc<E>,
    config: WorkflowEngineConfig,
    metrics: Arc<dyn MajraMetrics>,
    cancelled: Arc<DashMap<String, Arc<AtomicBool>>>,
}

impl<S: WorkflowStorage + 'static, E: StepExecutor + 'static> WorkflowEngine<S, E> {
    /// Create a new engine with default metrics.
    pub fn new(storage: Arc<S>, executor: Arc<E>, config: WorkflowEngineConfig) -> Self {
        Self {
            storage,
            executor,
            config,
            metrics: Arc::new(NoopMetrics),
            cancelled: Arc::new(DashMap::new()),
        }
    }

    /// Create a new engine with custom metrics.
    pub fn with_metrics(
        storage: Arc<S>,
        executor: Arc<E>,
        config: WorkflowEngineConfig,
        metrics: Arc<dyn MajraMetrics>,
    ) -> Self {
        Self {
            storage,
            executor,
            config,
            metrics,
            cancelled: Arc::new(DashMap::new()),
        }
    }

    /// Validate a workflow definition for structural correctness.
    ///
    /// Checks: acyclicity (via [`crate::queue::DagScheduler`]), referential
    /// integrity of `depends_on` and `Fallback` step IDs, no duplicate step IDs.
    pub fn validate(definition: &WorkflowDefinition) -> crate::error::Result<()> {
        let step_ids: HashSet<&str> = definition.steps.iter().map(|s| s.id.as_str()).collect();

        // Check for duplicate IDs.
        if step_ids.len() != definition.steps.len() {
            return Err(MajraError::WorkflowValidation("duplicate step IDs".into()));
        }

        // Check referential integrity of depends_on.
        for step in &definition.steps {
            for dep in &step.depends_on {
                if !step_ids.contains(dep.as_str()) {
                    return Err(MajraError::WorkflowValidation(format!(
                        "step '{}' depends on unknown step '{dep}'",
                        step.id
                    )));
                }
            }

            // Check fallback references.
            if let ErrorPolicy::Fallback(ref fb_id) = step.error_policy
                && !step_ids.contains(fb_id.as_str())
            {
                return Err(MajraError::WorkflowValidation(format!(
                    "step '{}' has fallback to unknown step '{fb_id}'",
                    step.id
                )));
            }
        }

        // Check acyclicity via existing DagScheduler.
        let dag = Dag {
            edges: definition
                .steps
                .iter()
                .map(|s| (s.id.clone(), s.depends_on.clone()))
                .collect(),
        };
        crate::queue::DagScheduler::new(&dag)?;

        Ok(())
    }

    /// Perform tier-based topological sort respecting trigger modes.
    ///
    /// Returns tiers (levels) where each tier contains steps that can execute
    /// in parallel. For `TriggerMode::All` steps, in-degree = number of deps.
    /// For `TriggerMode::Any` steps, in-degree = min(1, deps.len()).
    pub fn topological_sort_tiers(
        steps: &[WorkflowStep],
    ) -> crate::error::Result<Vec<Vec<String>>> {
        // Build in-degree map with trigger-mode semantics.
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();

        for step in steps {
            let required = match step.trigger_mode {
                TriggerMode::All => step.depends_on.len(),
                TriggerMode::Any => std::cmp::min(1, step.depends_on.len()),
            };
            in_degree.insert(step.id.as_str(), required);
            successors.entry(step.id.as_str()).or_default();

            for dep in &step.depends_on {
                successors
                    .entry(dep.as_str())
                    .or_default()
                    .push(step.id.as_str());
            }
        }

        let mut tiers = Vec::new();
        let mut visited = 0usize;

        // Seed: all steps with in-degree 0.
        let mut frontier: Vec<&str> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(k, _)| *k)
            .collect();
        frontier.sort(); // deterministic

        while !frontier.is_empty() {
            visited += frontier.len();
            tiers.push(frontier.iter().map(|s| s.to_string()).collect());

            let mut next_frontier = Vec::new();
            for &node in &frontier {
                if let Some(succs) = successors.get(node) {
                    for &succ in succs {
                        let deg = in_degree.get_mut(succ).ok_or_else(|| {
                            MajraError::WorkflowValidation(format!(
                                "internal: missing in-degree for '{succ}'"
                            ))
                        })?;
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            next_frontier.push(succ);
                        }
                    }
                }
            }
            next_frontier.sort();
            next_frontier.dedup();
            frontier = next_frontier;
        }

        if visited != steps.len() {
            return Err(MajraError::DagCycle(
                "workflow dependency graph contains a cycle".into(),
            ));
        }

        Ok(tiers)
    }

    /// Execute a workflow to completion.
    ///
    /// Creates a [`WorkflowRun`], walks tiers in order, executes steps with
    /// retry and error policies, and returns the completed run.
    #[instrument(skip(self, definition, input), fields(workflow_id = %definition.id))]
    pub async fn execute(
        &self,
        definition: &WorkflowDefinition,
        input: Option<serde_json::Value>,
        triggered_by: &str,
    ) -> crate::error::Result<WorkflowRun> {
        Self::validate(definition)?;
        let tiers = Self::topological_sort_tiers(&definition.steps)?;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let run_id = Uuid::new_v4().to_string();

        let mut run = WorkflowRun {
            id: run_id.clone(),
            workflow_id: definition.id.clone(),
            workflow_name: definition.name.clone(),
            status: WorkflowRunStatus::Running,
            input: input.clone(),
            output: None,
            error: None,
            triggered_by: triggered_by.into(),
            created_at: now_ms,
            started_at: Some(now_ms),
            completed_at: None,
        };
        self.storage.create_run(&run).await?;
        self.metrics.workflow_run_started(&definition.id);
        info!(%run_id, workflow = %definition.name, "workflow run started");

        // Cancellation token for this run.
        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancelled.insert(run_id.clone(), cancel_flag.clone());

        let step_map: HashMap<&str, &WorkflowStep> = definition
            .steps
            .iter()
            .map(|s| (s.id.as_str(), s))
            .collect();

        let mut ctx = WorkflowContext {
            step_results: HashMap::new(),
            input: input.unwrap_or(serde_json::Value::Null),
        };

        let mut run_error: Option<String> = None;

        'tiers: for tier in &tiers {
            // Check cancellation before each tier.
            if cancel_flag.load(Ordering::Acquire) {
                run.status = WorkflowRunStatus::Cancelled;
                run.completed_at = Some(chrono::Utc::now().timestamp_millis());
                self.storage.update_run(&run).await?;
                self.cancelled.remove(&run_id);
                info!(%run_id, "workflow run cancelled");
                return Ok(run);
            }

            // Batch steps within the tier.
            for batch_start in (0..tier.len()).step_by(self.config.max_parallel_steps) {
                let batch_end =
                    std::cmp::min(batch_start + self.config.max_parallel_steps, tier.len());
                let batch = &tier[batch_start..batch_end];

                let mut handles = Vec::with_capacity(batch.len());

                for step_id in batch {
                    let step = step_map[step_id.as_str()];

                    // Check trigger mode — for Any, skip if no dep completed.
                    if step.trigger_mode == TriggerMode::Any && !step.depends_on.is_empty() {
                        let any_completed = step.depends_on.iter().any(|dep_id| {
                            ctx.step_results
                                .get(dep_id.as_str())
                                .is_some_and(|r| r.status == StepRunStatus::Completed)
                        });
                        if !any_completed {
                            ctx.step_results.insert(
                                step_id.clone(),
                                StepResult {
                                    output: None,
                                    status: StepRunStatus::Skipped,
                                },
                            );
                            debug!(%step_id, "skipped: no dependency completed (trigger_mode=any)");
                            continue;
                        }
                    }

                    let executor = self.executor.clone();
                    let storage = self.storage.clone();
                    let metrics = self.metrics.clone();
                    let step_clone = step.clone();
                    let ctx_clone = ctx.clone();
                    let run_id_clone = run_id.clone();
                    let workflow_id = definition.id.clone();
                    let cancel = cancel_flag.clone();

                    handles.push(tokio::spawn(async move {
                        execute_step(
                            executor.as_ref(),
                            storage.as_ref(),
                            &metrics,
                            &step_clone,
                            &ctx_clone,
                            &run_id_clone,
                            &workflow_id,
                            &cancel,
                        )
                        .await
                    }));
                }

                // Collect results.
                for (i, handle) in handles.into_iter().enumerate() {
                    let step_id = &batch[i];
                    match handle.await {
                        Ok(Ok(result)) => {
                            ctx.step_results.insert(step_id.clone(), result);
                        }
                        Ok(Err(e)) => {
                            // Step failed with Fail policy — abort the run.
                            run_error = Some(e.to_string());
                            ctx.step_results.insert(
                                step_id.clone(),
                                StepResult {
                                    output: None,
                                    status: StepRunStatus::Failed,
                                },
                            );
                            break 'tiers;
                        }
                        Err(join_err) => {
                            run_error = Some(format!("step '{step_id}' panicked: {join_err}"));
                            ctx.step_results.insert(
                                step_id.clone(),
                                StepResult {
                                    output: None,
                                    status: StepRunStatus::Failed,
                                },
                            );
                            break 'tiers;
                        }
                    }
                }
            }
        }

        // Build output and finalize.
        let end_ms = chrono::Utc::now().timestamp_millis();
        let duration_ms = (end_ms - now_ms) as u64;

        if let Some(ref err) = run_error {
            run.status = WorkflowRunStatus::Failed;
            run.error = Some(err.clone());
            self.metrics
                .workflow_run_failed(&definition.id, duration_ms);
            warn!(%run_id, error = %err, "workflow run failed");
        } else {
            run.status = WorkflowRunStatus::Completed;
            // Aggregate step outputs.
            let mut output = serde_json::Map::new();
            for (step_id, result) in &ctx.step_results {
                if let Some(ref val) = result.output {
                    output.insert(step_id.clone(), val.clone());
                }
            }
            run.output = Some(serde_json::Value::Object(output));
            self.metrics
                .workflow_run_completed(&definition.id, duration_ms);
            info!(%run_id, duration_ms, "workflow run completed");
        }

        run.completed_at = Some(end_ms);
        self.storage.update_run(&run).await?;
        self.cancelled.remove(&run_id);

        Ok(run)
    }

    /// Cancel a running workflow. Steps already in flight will complete,
    /// but no new tiers will be started.
    pub async fn cancel(&self, run_id: &str) -> crate::error::Result<()> {
        if let Some(flag) = self.cancelled.get(run_id) {
            flag.store(true, Ordering::Release);
            debug!(%run_id, "workflow cancellation requested");
            Ok(())
        } else {
            Err(MajraError::WorkflowRunNotFound(run_id.into()))
        }
    }

    /// Resume a previously interrupted workflow run from storage.
    ///
    /// Loads the run and its step results from the storage backend,
    /// reconstructs the `WorkflowContext`, and re-executes only the
    /// steps that haven't completed yet. This provides crash recovery
    /// (durable execution) when using a persistent backend (SQLite or
    /// PostgreSQL).
    ///
    /// Returns `Err` if the run is not found, already completed, or
    /// the definition cannot be located.
    #[instrument(skip(self, definition), fields(run_id = %run_id))]
    pub async fn resume(
        &self,
        run_id: &str,
        definition: &WorkflowDefinition,
    ) -> crate::error::Result<WorkflowRun> {
        let mut run = self
            .storage
            .get_run(run_id)
            .await?
            .ok_or_else(|| MajraError::WorkflowRunNotFound(run_id.into()))?;

        if matches!(
            run.status,
            WorkflowRunStatus::Completed | WorkflowRunStatus::Cancelled
        ) {
            return Ok(run);
        }

        Self::validate(definition)?;
        let tiers = Self::topological_sort_tiers(&definition.steps)?;

        // Reconstruct context from persisted step runs.
        let step_runs = self.storage.get_step_runs_for_run(run_id).await?;
        let mut ctx = WorkflowContext {
            step_results: HashMap::new(),
            input: run.input.clone().unwrap_or(serde_json::Value::Null),
        };
        let mut completed_steps: HashSet<String> = HashSet::new();
        for sr in &step_runs {
            if matches!(sr.status, StepRunStatus::Completed | StepRunStatus::Skipped) {
                ctx.step_results.insert(
                    sr.step_id.clone(),
                    StepResult {
                        output: sr.output.clone(),
                        status: sr.status,
                    },
                );
                completed_steps.insert(sr.step_id.clone());
            }
        }

        info!(
            %run_id,
            completed = completed_steps.len(),
            total = definition.steps.len(),
            "resuming workflow run"
        );

        // Update run status to Running.
        run.status = WorkflowRunStatus::Running;
        self.storage.update_run(&run).await?;

        let cancel_flag = Arc::new(AtomicBool::new(false));
        self.cancelled
            .insert(run_id.to_string(), cancel_flag.clone());

        let step_map: HashMap<&str, &WorkflowStep> = definition
            .steps
            .iter()
            .map(|s| (s.id.as_str(), s))
            .collect();

        let mut run_error: Option<String> = None;
        let start_ms = chrono::Utc::now().timestamp_millis();

        'tiers: for tier in &tiers {
            if cancel_flag.load(Ordering::Acquire) {
                run.status = WorkflowRunStatus::Cancelled;
                run.completed_at = Some(chrono::Utc::now().timestamp_millis());
                self.storage.update_run(&run).await?;
                self.cancelled.remove(run_id);
                return Ok(run);
            }

            // Skip tiers where all steps are already completed.
            let pending: Vec<&String> = tier
                .iter()
                .filter(|id| !completed_steps.contains(id.as_str()))
                .collect();
            if pending.is_empty() {
                continue;
            }

            for step_id in &pending {
                let step = step_map[step_id.as_str()];

                let executor = self.executor.clone();
                let storage = self.storage.clone();
                let metrics = self.metrics.clone();
                let step_clone = step.clone();
                let ctx_clone = ctx.clone();
                let run_id_str = run_id.to_string();
                let workflow_id = definition.id.clone();
                let cancel = cancel_flag.clone();

                let handle = tokio::spawn(async move {
                    execute_step(
                        executor.as_ref(),
                        storage.as_ref(),
                        &metrics,
                        &step_clone,
                        &ctx_clone,
                        &run_id_str,
                        &workflow_id,
                        &cancel,
                    )
                    .await
                });

                match handle.await {
                    Ok(Ok(result)) => {
                        ctx.step_results.insert((*step_id).clone(), result);
                    }
                    Ok(Err(e)) => {
                        run_error = Some(e.to_string());
                        break 'tiers;
                    }
                    Err(join_err) => {
                        run_error = Some(format!("step '{step_id}' panicked: {join_err}"));
                        break 'tiers;
                    }
                }
            }
        }

        let end_ms = chrono::Utc::now().timestamp_millis();
        let duration_ms = (end_ms - start_ms) as u64;

        if let Some(ref err) = run_error {
            run.status = WorkflowRunStatus::Failed;
            run.error = Some(err.clone());
            self.metrics
                .workflow_run_failed(&definition.id, duration_ms);
        } else {
            run.status = WorkflowRunStatus::Completed;
            let mut output = serde_json::Map::new();
            for (step_id, result) in &ctx.step_results {
                if let Some(ref val) = result.output {
                    output.insert(step_id.clone(), val.clone());
                }
            }
            run.output = Some(serde_json::Value::Object(output));
            self.metrics
                .workflow_run_completed(&definition.id, duration_ms);
        }

        run.completed_at = Some(end_ms);
        self.storage.update_run(&run).await?;
        self.cancelled.remove(run_id);

        info!(%run_id, status = %run.status, "workflow run resumed and finished");
        Ok(run)
    }
}

/// Execute a single step with retry and error policy handling.
///
/// Returns `Ok(StepResult)` if the step completed, was skipped, or continued
/// under a non-Fail error policy. Returns `Err` only when `ErrorPolicy::Fail`
/// triggers, signalling the engine to abort the run.
#[allow(clippy::too_many_arguments)]
fn execute_step<'a, S: WorkflowStorage, E: StepExecutor>(
    executor: &'a E,
    storage: &'a S,
    metrics: &'a Arc<dyn MajraMetrics>,
    step: &'a WorkflowStep,
    ctx: &'a WorkflowContext,
    run_id: &'a str,
    workflow_id: &'a str,
    cancel: &'a AtomicBool,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = crate::error::Result<StepResult>> + Send + 'a>,
> {
    Box::pin(async move {
        let step_run_id = Uuid::new_v4().to_string();
        let start_ms = chrono::Utc::now().timestamp_millis();

        let mut step_run = StepRun {
            id: step_run_id.clone(),
            run_id: run_id.into(),
            step_id: step.id.clone(),
            step_name: step.name.clone(),
            status: StepRunStatus::Running,
            input: Some(step.config.clone()),
            output: None,
            error: None,
            attempt: 0,
            started_at: Some(start_ms),
            completed_at: None,
            duration_ms: None,
        };
        storage.create_step_run(&step_run).await?;
        metrics.workflow_step_started(workflow_id, &step.id);

        let max_attempts = step.retry_policy.max_attempts.clamp(1, 10);
        let mut last_error = String::new();

        for attempt in 1..=max_attempts {
            if cancel.load(Ordering::Acquire) {
                step_run.status = StepRunStatus::Skipped;
                step_run.attempt = attempt;
                let end_ms = chrono::Utc::now().timestamp_millis();
                step_run.completed_at = Some(end_ms);
                step_run.duration_ms = Some(end_ms - start_ms);
                storage.update_step_run(&step_run).await?;
                return Ok(StepResult {
                    output: None,
                    status: StepRunStatus::Skipped,
                });
            }

            step_run.attempt = attempt;

            match executor.execute(step, ctx).await {
                Ok(output) => {
                    let end_ms = chrono::Utc::now().timestamp_millis();
                    step_run.status = StepRunStatus::Completed;
                    step_run.output = Some(output.clone());
                    step_run.completed_at = Some(end_ms);
                    step_run.duration_ms = Some(end_ms - start_ms);
                    storage.update_step_run(&step_run).await?;
                    metrics.workflow_step_finished(
                        workflow_id,
                        &step.id,
                        "completed",
                        (end_ms - start_ms) as u64,
                    );
                    debug!(step_id = %step.id, attempt, "step completed");
                    return Ok(StepResult {
                        output: Some(output),
                        status: StepRunStatus::Completed,
                    });
                }
                Err(err) => {
                    last_error = err;
                    if attempt < max_attempts {
                        let backoff = step.retry_policy.backoff * attempt;
                        debug!(step_id = %step.id, attempt, next_in = ?backoff, "step failed, retrying");
                        tokio::time::sleep(backoff).await;
                    }
                }
            }
        }

        // All attempts exhausted — apply error policy.
        let end_ms = chrono::Utc::now().timestamp_millis();
        let duration = (end_ms - start_ms) as u64;

        match &step.error_policy {
            ErrorPolicy::Fail => {
                step_run.status = StepRunStatus::Failed;
                step_run.error = Some(last_error.clone());
                step_run.completed_at = Some(end_ms);
                step_run.duration_ms = Some(end_ms - start_ms);
                storage.update_step_run(&step_run).await?;
                metrics.workflow_step_finished(workflow_id, &step.id, "failed", duration);
                warn!(step_id = %step.id, error = %last_error, "step failed (policy=fail)");
                Err(MajraError::WorkflowStepFailed(format!(
                    "step '{}': {last_error}",
                    step.id
                )))
            }
            ErrorPolicy::Continue => {
                step_run.status = StepRunStatus::Failed;
                step_run.error = Some(last_error.clone());
                step_run.completed_at = Some(end_ms);
                step_run.duration_ms = Some(end_ms - start_ms);
                storage.update_step_run(&step_run).await?;
                metrics.workflow_step_finished(workflow_id, &step.id, "failed", duration);
                debug!(step_id = %step.id, "step failed (policy=continue)");
                Ok(StepResult {
                    output: None,
                    status: StepRunStatus::Failed,
                })
            }
            ErrorPolicy::Skip => {
                step_run.status = StepRunStatus::Skipped;
                step_run.error = Some(last_error.clone());
                step_run.completed_at = Some(end_ms);
                step_run.duration_ms = Some(end_ms - start_ms);
                storage.update_step_run(&step_run).await?;
                metrics.workflow_step_finished(workflow_id, &step.id, "skipped", duration);
                debug!(step_id = %step.id, "step failed (policy=skip)");
                Ok(StepResult {
                    output: None,
                    status: StepRunStatus::Skipped,
                })
            }
            ErrorPolicy::Fallback(fallback_id) => {
                step_run.status = StepRunStatus::Failed;
                step_run.error = Some(last_error.clone());
                step_run.completed_at = Some(end_ms);
                step_run.duration_ms = Some(end_ms - start_ms);
                storage.update_step_run(&step_run).await?;
                metrics.workflow_step_finished(workflow_id, &step.id, "failed", duration);
                debug!(step_id = %step.id, %fallback_id, "step failed, executing fallback");

                // Execute fallback inline — construct a minimal fallback step.
                // The fallback step uses default error/retry policies.
                let fallback_step = WorkflowStep {
                    id: format!("{}_fallback", step.id),
                    name: format!("{} (fallback)", step.name),
                    depends_on: vec![],
                    trigger_mode: TriggerMode::All,
                    config: serde_json::Value::Object(serde_json::Map::new()),
                    error_policy: ErrorPolicy::Continue,
                    retry_policy: RetryPolicy::default(),
                };

                let fb_result = execute_step(
                    executor,
                    storage,
                    metrics,
                    &fallback_step,
                    ctx,
                    run_id,
                    workflow_id,
                    cancel,
                )
                .await?;

                Ok(fb_result)
            }
        }
    })
}

// ---------------------------------------------------------------------------
// SQLite storage (feature-gated)
// ---------------------------------------------------------------------------

/// SQLite-backed workflow storage.
///
/// Uses WAL mode. Tables are separate from [`crate::queue::persistence::SqliteBackend`].
#[cfg(feature = "sqlite")]
pub struct SqliteWorkflowStorage {
    conn: std::sync::Mutex<rusqlite::Connection>,
}

#[cfg(feature = "sqlite")]
impl SqliteWorkflowStorage {
    /// Open or create a WAL-mode database at the given path.
    pub fn open(path: &std::path::Path) -> crate::error::Result<Self> {
        let conn = rusqlite::Connection::open(path).map_err(ws_err)?;
        Self::init(conn)
    }

    /// Open an in-memory database (for testing).
    pub fn in_memory() -> crate::error::Result<Self> {
        let conn = rusqlite::Connection::open_in_memory().map_err(ws_err)?;
        Self::init(conn)
    }

    fn init(conn: rusqlite::Connection) -> crate::error::Result<Self> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;

             CREATE TABLE IF NOT EXISTS workflow_definitions (
                 id TEXT PRIMARY KEY,
                 name TEXT NOT NULL,
                 description TEXT,
                 steps_json TEXT NOT NULL DEFAULT '[]',
                 enabled INTEGER NOT NULL DEFAULT 1,
                 version INTEGER NOT NULL DEFAULT 1,
                 created_by TEXT NOT NULL DEFAULT 'system',
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS workflow_runs (
                 id TEXT PRIMARY KEY,
                 workflow_id TEXT NOT NULL,
                 workflow_name TEXT NOT NULL,
                 status TEXT NOT NULL DEFAULT 'pending',
                 input_json TEXT,
                 output_json TEXT,
                 error TEXT,
                 triggered_by TEXT NOT NULL DEFAULT 'manual',
                 created_at INTEGER NOT NULL,
                 started_at INTEGER,
                 completed_at INTEGER
             );

             CREATE TABLE IF NOT EXISTS workflow_step_runs (
                 id TEXT PRIMARY KEY,
                 run_id TEXT NOT NULL,
                 step_id TEXT NOT NULL,
                 step_name TEXT NOT NULL,
                 status TEXT NOT NULL DEFAULT 'pending',
                 input_json TEXT,
                 output_json TEXT,
                 error TEXT,
                 attempt INTEGER NOT NULL DEFAULT 1,
                 started_at INTEGER,
                 completed_at INTEGER,
                 duration_ms INTEGER
             );

             CREATE INDEX IF NOT EXISTS idx_wf_runs_workflow ON workflow_runs(workflow_id);
             CREATE INDEX IF NOT EXISTS idx_wf_step_runs_run ON workflow_step_runs(run_id);",
        )
        .map_err(ws_err)?;
        Ok(Self {
            conn: std::sync::Mutex::new(conn),
        })
    }

    fn lock_conn(&self) -> crate::error::Result<std::sync::MutexGuard<'_, rusqlite::Connection>> {
        self.conn
            .lock()
            .map_err(|e| MajraError::WorkflowStorage(format!("mutex poisoned: {e}")))
    }
}

#[cfg(feature = "sqlite")]
fn ws_err(e: impl std::fmt::Display) -> MajraError {
    MajraError::WorkflowStorage(e.to_string())
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl WorkflowStorage for SqliteWorkflowStorage {
    async fn create_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()> {
        let conn = self.lock_conn()?;
        let steps_json = serde_json::to_string(&def.steps).map_err(ws_err)?;
        conn.execute(
            "INSERT OR REPLACE INTO workflow_definitions
             (id, name, description, steps_json, enabled, version, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                def.id,
                def.name,
                def.description,
                steps_json,
                def.enabled as i32,
                def.version,
                def.created_by,
                def.created_at,
                def.updated_at,
            ],
        )
        .map_err(ws_err)?;
        Ok(())
    }

    async fn get_definition(&self, id: &str) -> crate::error::Result<Option<WorkflowDefinition>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, steps_json, enabled, version, created_by, created_at, updated_at
                 FROM workflow_definitions WHERE id = ?1",
            )
            .map_err(ws_err)?;
        let mut rows = stmt
            .query_map(rusqlite::params![id], |row| {
                let steps_json: String = row.get(3)?;
                let enabled: i32 = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    steps_json,
                    enabled != 0,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })
            .map_err(ws_err)?;

        match rows.next() {
            Some(Ok((
                id,
                name,
                description,
                steps_json,
                enabled,
                version,
                created_by,
                created_at,
                updated_at,
            ))) => {
                let steps: Vec<WorkflowStep> = serde_json::from_str(&steps_json).map_err(ws_err)?;
                Ok(Some(WorkflowDefinition {
                    id,
                    name,
                    description,
                    steps,
                    enabled,
                    version,
                    created_by,
                    created_at,
                    updated_at,
                }))
            }
            Some(Err(e)) => Err(ws_err(e)),
            None => Ok(None),
        }
    }

    async fn update_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()> {
        self.create_definition(def).await
    }

    async fn delete_definition(&self, id: &str) -> crate::error::Result<bool> {
        let conn = self.lock_conn()?;
        let changed = conn
            .execute(
                "DELETE FROM workflow_definitions WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(ws_err)?;
        Ok(changed > 0)
    }

    async fn list_definitions(
        &self,
        limit: usize,
        offset: usize,
    ) -> crate::error::Result<Vec<WorkflowDefinition>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, steps_json, enabled, version, created_by, created_at, updated_at
                 FROM workflow_definitions LIMIT ?1 OFFSET ?2",
            )
            .map_err(ws_err)?;
        let rows = stmt
            .query_map(rusqlite::params![limit as i64, offset as i64], |row| {
                let steps_json: String = row.get(3)?;
                let enabled: i32 = row.get(4)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    steps_json,
                    enabled != 0,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })
            .map_err(ws_err)?;

        let mut result = Vec::new();
        for row in rows {
            let (
                id,
                name,
                description,
                steps_json,
                enabled,
                version,
                created_by,
                created_at,
                updated_at,
            ) = row.map_err(ws_err)?;
            let steps: Vec<WorkflowStep> = serde_json::from_str(&steps_json).map_err(ws_err)?;
            result.push(WorkflowDefinition {
                id,
                name,
                description,
                steps,
                enabled,
                version,
                created_by,
                created_at,
                updated_at,
            });
        }
        Ok(result)
    }

    async fn create_run(&self, run: &WorkflowRun) -> crate::error::Result<()> {
        let conn = self.lock_conn()?;
        let status = serde_json::to_string(&run.status).map_err(ws_err)?;
        let input_json = run
            .input
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(ws_err)?;
        let output_json = run
            .output
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(ws_err)?;
        conn.execute(
            "INSERT OR REPLACE INTO workflow_runs
             (id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                run.id,
                run.workflow_id,
                run.workflow_name,
                status,
                input_json,
                output_json,
                run.error,
                run.triggered_by,
                run.created_at,
                run.started_at,
                run.completed_at,
            ],
        )
        .map_err(ws_err)?;
        Ok(())
    }

    async fn get_run(&self, id: &str) -> crate::error::Result<Option<WorkflowRun>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at
                 FROM workflow_runs WHERE id = ?1",
            )
            .map_err(ws_err)?;
        let mut rows = stmt
            .query_map(rusqlite::params![id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, Option<i64>>(10)?,
                ))
            })
            .map_err(ws_err)?;

        match rows.next() {
            Some(Ok((
                id,
                workflow_id,
                workflow_name,
                status_str,
                input_str,
                output_str,
                error,
                triggered_by,
                created_at,
                started_at,
                completed_at,
            ))) => {
                let status: WorkflowRunStatus =
                    serde_json::from_str(&status_str).map_err(ws_err)?;
                let input = input_str
                    .map(|s| serde_json::from_str(&s))
                    .transpose()
                    .map_err(ws_err)?;
                let output = output_str
                    .map(|s| serde_json::from_str(&s))
                    .transpose()
                    .map_err(ws_err)?;
                Ok(Some(WorkflowRun {
                    id,
                    workflow_id,
                    workflow_name,
                    status,
                    input,
                    output,
                    error,
                    triggered_by,
                    created_at,
                    started_at,
                    completed_at,
                }))
            }
            Some(Err(e)) => Err(ws_err(e)),
            None => Ok(None),
        }
    }

    async fn update_run(&self, run: &WorkflowRun) -> crate::error::Result<()> {
        self.create_run(run).await
    }

    async fn list_runs(
        &self,
        workflow_id: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> crate::error::Result<Vec<WorkflowRun>> {
        let conn = self.lock_conn()?;
        let (sql, params_vec): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = match workflow_id {
            Some(wid) => (
                "SELECT id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at
                 FROM workflow_runs WHERE workflow_id = ?1 LIMIT ?2 OFFSET ?3",
                vec![
                    Box::new(wid.to_string()),
                    Box::new(limit as i64),
                    Box::new(offset as i64),
                ],
            ),
            None => (
                "SELECT id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at
                 FROM workflow_runs LIMIT ?1 OFFSET ?2",
                vec![Box::new(limit as i64), Box::new(offset as i64)],
            ),
        };

        let mut stmt = conn.prepare(sql).map_err(ws_err)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, Option<i64>>(10)?,
                ))
            })
            .map_err(ws_err)?;

        let mut result = Vec::new();
        for row in rows {
            let (
                id,
                workflow_id,
                workflow_name,
                status_str,
                input_str,
                output_str,
                error,
                triggered_by,
                created_at,
                started_at,
                completed_at,
            ) = row.map_err(ws_err)?;
            let status: WorkflowRunStatus = serde_json::from_str(&status_str).map_err(ws_err)?;
            let input = input_str
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(ws_err)?;
            let output = output_str
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(ws_err)?;
            result.push(WorkflowRun {
                id,
                workflow_id,
                workflow_name,
                status,
                input,
                output,
                error,
                triggered_by,
                created_at,
                started_at,
                completed_at,
            });
        }
        Ok(result)
    }

    async fn create_step_run(&self, sr: &StepRun) -> crate::error::Result<()> {
        let conn = self.lock_conn()?;
        let status = serde_json::to_string(&sr.status).map_err(ws_err)?;
        let input_json = sr
            .input
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(ws_err)?;
        let output_json = sr
            .output
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(ws_err)?;
        conn.execute(
            "INSERT OR REPLACE INTO workflow_step_runs
             (id, run_id, step_id, step_name, status, input_json, output_json, error, attempt, started_at, completed_at, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                sr.id,
                sr.run_id,
                sr.step_id,
                sr.step_name,
                status,
                input_json,
                output_json,
                sr.error,
                sr.attempt,
                sr.started_at,
                sr.completed_at,
                sr.duration_ms,
            ],
        )
        .map_err(ws_err)?;
        Ok(())
    }

    async fn update_step_run(&self, sr: &StepRun) -> crate::error::Result<()> {
        self.create_step_run(sr).await
    }

    async fn get_step_runs_for_run(&self, run_id: &str) -> crate::error::Result<Vec<StepRun>> {
        let conn = self.lock_conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, run_id, step_id, step_name, status, input_json, output_json, error, attempt, started_at, completed_at, duration_ms
                 FROM workflow_step_runs WHERE run_id = ?1",
            )
            .map_err(ws_err)?;
        let rows = stmt
            .query_map(rusqlite::params![run_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, Option<i64>>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                ))
            })
            .map_err(ws_err)?;

        let mut result = Vec::new();
        for row in rows {
            let (
                id,
                run_id,
                step_id,
                step_name,
                status_str,
                input_str,
                output_str,
                error,
                attempt,
                started_at,
                completed_at,
                duration_ms,
            ) = row.map_err(ws_err)?;
            let status: StepRunStatus = serde_json::from_str(&status_str).map_err(ws_err)?;
            let input = input_str
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(ws_err)?;
            let output = output_str
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(ws_err)?;
            result.push(StepRun {
                id,
                run_id,
                step_id,
                step_name,
                status,
                input,
                output,
                error,
                attempt,
                started_at,
                completed_at,
                duration_ms,
            });
        }
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- Helpers ---

    fn step(id: &str, deps: &[&str]) -> WorkflowStep {
        WorkflowStep {
            id: id.into(),
            name: id.into(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            trigger_mode: TriggerMode::All,
            config: serde_json::Value::Null,
            error_policy: ErrorPolicy::default(),
            retry_policy: RetryPolicy::default(),
        }
    }

    fn step_any(id: &str, deps: &[&str]) -> WorkflowStep {
        let mut s = step(id, deps);
        s.trigger_mode = TriggerMode::Any;
        s
    }

    fn definition(steps: Vec<WorkflowStep>) -> WorkflowDefinition {
        WorkflowDefinition {
            id: "wf-1".into(),
            name: "test-workflow".into(),
            description: None,
            steps,
            enabled: true,
            version: 1,
            created_by: "test".into(),
            created_at: 0,
            updated_at: 0,
        }
    }

    // --- Tier sort tests ---

    #[test]
    fn tier_sort_linear_chain() {
        let steps = vec![step("a", &[]), step("b", &["a"]), step("c", &["b"])];
        let tiers =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::topological_sort_tiers(&steps)
                .unwrap();
        assert_eq!(tiers, vec![vec!["a"], vec!["b"], vec!["c"]]);
    }

    #[test]
    fn tier_sort_diamond() {
        let steps = vec![
            step("a", &[]),
            step("b", &["a"]),
            step("c", &["a"]),
            step("d", &["b", "c"]),
        ];
        let tiers =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::topological_sort_tiers(&steps)
                .unwrap();
        assert_eq!(tiers, vec![vec!["a"], vec!["b", "c"], vec!["d"]]);
    }

    #[test]
    fn tier_sort_any_trigger() {
        // d depends on b and c with TriggerMode::Any → in-degree = 1
        // So d enters the frontier as soon as either b or c completes.
        let steps = vec![
            step("a", &[]),
            step("b", &["a"]),
            step("c", &["a"]),
            step_any("d", &["b", "c"]),
        ];
        let tiers =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::topological_sort_tiers(&steps)
                .unwrap();
        // d should be in the same tier as the first dep that triggers it.
        // With Kahn's: tier 0 = [a], tier 1 = [b, c] (both decrement d's in-degree=1, but first one sets it to 0)
        // After processing b (first alphabetically), d's in-degree goes to 0 → d enters next_frontier.
        // Then c also tries to decrement but d is already at 0. Dedup prevents double-add.
        // So tier 2 = [d].
        assert_eq!(tiers, vec![vec!["a"], vec!["b", "c"], vec!["d"]]);
    }

    #[test]
    fn tier_sort_independent() {
        let steps = vec![step("a", &[]), step("b", &[]), step("c", &[])];
        let tiers =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::topological_sort_tiers(&steps)
                .unwrap();
        assert_eq!(tiers, vec![vec!["a", "b", "c"]]);
    }

    #[test]
    fn tier_sort_cycle_detected() {
        let steps = vec![step("a", &["b"]), step("b", &["a"])];
        let result =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::topological_sort_tiers(&steps);
        assert!(result.is_err());
    }

    // --- Validation tests ---

    #[test]
    fn validate_ok() {
        let def = definition(vec![step("a", &[]), step("b", &["a"])]);
        assert!(WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::validate(&def).is_ok());
    }

    #[test]
    fn validate_duplicate_ids() {
        let def = definition(vec![step("a", &[]), step("a", &[])]);
        let err =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::validate(&def).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn validate_unknown_dep() {
        let def = definition(vec![step("a", &["missing"])]);
        let err =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::validate(&def).unwrap_err();
        assert!(err.to_string().contains("unknown step"));
    }

    #[test]
    fn validate_unknown_fallback() {
        let mut s = step("a", &[]);
        s.error_policy = ErrorPolicy::Fallback("missing".into());
        let def = definition(vec![s]);
        let err =
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::validate(&def).unwrap_err();
        assert!(err.to_string().contains("unknown step"));
    }

    // --- In-memory storage tests ---

    #[tokio::test]
    async fn inmemory_definition_crud() {
        let storage = InMemoryWorkflowStorage::new();
        let def = definition(vec![step("a", &[])]);

        storage.create_definition(&def).await.unwrap();
        let loaded = storage.get_definition("wf-1").await.unwrap().unwrap();
        assert_eq!(loaded.name, "test-workflow");

        let list = storage.list_definitions(10, 0).await.unwrap();
        assert_eq!(list.len(), 1);

        assert!(storage.delete_definition("wf-1").await.unwrap());
        assert!(storage.get_definition("wf-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn inmemory_run_crud() {
        let storage = InMemoryWorkflowStorage::new();
        let run = WorkflowRun {
            id: "run-1".into(),
            workflow_id: "wf-1".into(),
            workflow_name: "test".into(),
            status: WorkflowRunStatus::Pending,
            input: None,
            output: None,
            error: None,
            triggered_by: "test".into(),
            created_at: 0,
            started_at: None,
            completed_at: None,
        };

        storage.create_run(&run).await.unwrap();
        let loaded = storage.get_run("run-1").await.unwrap().unwrap();
        assert_eq!(loaded.status, WorkflowRunStatus::Pending);

        let runs = storage.list_runs(Some("wf-1"), 10, 0).await.unwrap();
        assert_eq!(runs.len(), 1);
        let runs = storage.list_runs(Some("other"), 10, 0).await.unwrap();
        assert_eq!(runs.len(), 0);
    }

    #[tokio::test]
    async fn inmemory_step_run_crud() {
        let storage = InMemoryWorkflowStorage::new();
        let sr = StepRun {
            id: "sr-1".into(),
            run_id: "run-1".into(),
            step_id: "step-a".into(),
            step_name: "a".into(),
            status: StepRunStatus::Completed,
            input: None,
            output: Some(serde_json::json!(42)),
            error: None,
            attempt: 1,
            started_at: Some(0),
            completed_at: Some(10),
            duration_ms: Some(10),
        };

        storage.create_step_run(&sr).await.unwrap();
        let loaded = storage.get_step_runs_for_run("run-1").await.unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].output, Some(serde_json::json!(42)));
    }

    // --- Engine execution tests ---

    /// Noop executor that returns the step config as output.
    struct NoopExecutor;

    #[async_trait]
    impl StepExecutor for NoopExecutor {
        async fn execute(
            &self,
            step: &WorkflowStep,
            _context: &WorkflowContext,
        ) -> Result<serde_json::Value, String> {
            Ok(step.config.clone())
        }
    }

    /// Executor that fails N times then succeeds.
    struct RetryExecutor {
        fail_count: std::sync::atomic::AtomicU32,
        target_fails: u32,
    }

    impl RetryExecutor {
        fn new(target_fails: u32) -> Self {
            Self {
                fail_count: std::sync::atomic::AtomicU32::new(0),
                target_fails,
            }
        }
    }

    #[async_trait]
    impl StepExecutor for RetryExecutor {
        async fn execute(
            &self,
            _step: &WorkflowStep,
            _context: &WorkflowContext,
        ) -> Result<serde_json::Value, String> {
            let count = self.fail_count.fetch_add(1, Ordering::Relaxed);
            if count < self.target_fails {
                Err(format!("fail #{}", count + 1))
            } else {
                Ok(serde_json::json!("recovered"))
            }
        }
    }

    /// Executor that always fails.
    struct FailExecutor;

    #[async_trait]
    impl StepExecutor for FailExecutor {
        async fn execute(
            &self,
            _step: &WorkflowStep,
            _context: &WorkflowContext,
        ) -> Result<serde_json::Value, String> {
            Err("always fails".into())
        }
    }

    #[tokio::test]
    async fn engine_simple_execution() {
        let storage = Arc::new(InMemoryWorkflowStorage::new());
        let executor = Arc::new(NoopExecutor);
        let engine =
            WorkflowEngine::new(storage.clone(), executor, WorkflowEngineConfig::default());

        let mut a = step("a", &[]);
        a.config = serde_json::json!(1);
        let mut b = step("b", &["a"]);
        b.config = serde_json::json!(2);
        let def = definition(vec![a, b]);

        let run = engine.execute(&def, None, "test").await.unwrap();
        assert_eq!(run.status, WorkflowRunStatus::Completed);
        assert!(run.output.is_some());

        let output = run.output.unwrap();
        assert_eq!(output["a"], 1);
        assert_eq!(output["b"], 2);

        // Verify step runs recorded.
        let step_runs = storage.get_step_runs_for_run(&run.id).await.unwrap();
        assert_eq!(step_runs.len(), 2);
    }

    #[tokio::test]
    async fn engine_retry_then_succeed() {
        let storage = Arc::new(InMemoryWorkflowStorage::new());
        let executor = Arc::new(RetryExecutor::new(2));
        let engine =
            WorkflowEngine::new(storage.clone(), executor, WorkflowEngineConfig::default());

        let mut s = step("a", &[]);
        s.retry_policy = RetryPolicy {
            max_attempts: 3,
            backoff: Duration::from_millis(1),
        };
        let def = definition(vec![s]);

        let run = engine.execute(&def, None, "test").await.unwrap();
        assert_eq!(run.status, WorkflowRunStatus::Completed);

        let step_runs = storage.get_step_runs_for_run(&run.id).await.unwrap();
        assert_eq!(step_runs[0].attempt, 3);
        assert_eq!(step_runs[0].status, StepRunStatus::Completed);
    }

    #[tokio::test]
    async fn engine_error_policy_fail() {
        let storage = Arc::new(InMemoryWorkflowStorage::new());
        let executor = Arc::new(FailExecutor);
        let engine =
            WorkflowEngine::new(storage.clone(), executor, WorkflowEngineConfig::default());

        let def = definition(vec![step("a", &[]), step("b", &["a"])]);

        let run = engine.execute(&def, None, "test").await.unwrap();
        assert_eq!(run.status, WorkflowRunStatus::Failed);
        assert!(run.error.is_some());
    }

    #[tokio::test]
    async fn engine_error_policy_continue() {
        let storage = Arc::new(InMemoryWorkflowStorage::new());
        let executor = Arc::new(FailExecutor);
        let engine =
            WorkflowEngine::new(storage.clone(), executor, WorkflowEngineConfig::default());

        let mut a = step("a", &[]);
        a.error_policy = ErrorPolicy::Continue;
        let b = step("b", &["a"]);
        let def = definition(vec![a, b]);

        let run = engine.execute(&def, None, "test").await.unwrap();
        // Run completes because a's failure uses Continue, but b also fails with default Fail policy.
        assert_eq!(run.status, WorkflowRunStatus::Failed);

        let step_runs = storage.get_step_runs_for_run(&run.id).await.unwrap();
        let a_run = step_runs.iter().find(|s| s.step_id == "a").unwrap();
        assert_eq!(a_run.status, StepRunStatus::Failed);
    }

    #[tokio::test]
    async fn engine_error_policy_skip() {
        let storage = Arc::new(InMemoryWorkflowStorage::new());
        let executor = Arc::new(FailExecutor);
        let engine =
            WorkflowEngine::new(storage.clone(), executor, WorkflowEngineConfig::default());

        let mut a = step("a", &[]);
        a.error_policy = ErrorPolicy::Skip;
        let def = definition(vec![a]);

        let run = engine.execute(&def, None, "test").await.unwrap();
        assert_eq!(run.status, WorkflowRunStatus::Completed);
    }

    #[tokio::test]
    async fn engine_context_propagation() {
        /// Executor that reads upstream output from context.
        struct ContextExecutor;

        #[async_trait]
        impl StepExecutor for ContextExecutor {
            async fn execute(
                &self,
                step: &WorkflowStep,
                context: &WorkflowContext,
            ) -> Result<serde_json::Value, String> {
                if step.id == "b" {
                    let a_output = context
                        .step_results
                        .get("a")
                        .and_then(|r| r.output.clone())
                        .unwrap_or(serde_json::Value::Null);
                    Ok(serde_json::json!({"upstream": a_output}))
                } else {
                    Ok(serde_json::json!(42))
                }
            }
        }

        let storage = Arc::new(InMemoryWorkflowStorage::new());
        let executor = Arc::new(ContextExecutor);
        let engine = WorkflowEngine::new(storage, executor, WorkflowEngineConfig::default());

        let def = definition(vec![step("a", &[]), step("b", &["a"])]);
        let run = engine.execute(&def, None, "test").await.unwrap();

        let output = run.output.unwrap();
        assert_eq!(output["b"]["upstream"], 42);
    }

    // --- InMemoryWorkflowStorage retention ---

    #[tokio::test]
    async fn in_memory_evict_older_than() {
        let storage = InMemoryWorkflowStorage::new();
        let now_ms = chrono::Utc::now().timestamp_millis();

        // Insert a completed run with an old completed_at.
        let old_run = WorkflowRun {
            id: "old-run".into(),
            workflow_id: "wf-1".into(),
            workflow_name: "test".into(),
            status: WorkflowRunStatus::Completed,
            input: None,
            output: None,
            error: None,
            triggered_by: "test".into(),
            created_at: now_ms - 7_200_000,
            started_at: Some(now_ms - 7_200_000),
            completed_at: Some(now_ms - 7_200_000), // 2 hours ago
        };
        storage.create_run(&old_run).await.unwrap();

        // Insert associated step run.
        let sr = StepRun {
            id: "old-step".into(),
            run_id: "old-run".into(),
            step_id: "a".into(),
            step_name: "a".into(),
            status: StepRunStatus::Completed,
            input: None,
            output: None,
            error: None,
            attempt: 1,
            started_at: None,
            completed_at: None,
            duration_ms: None,
        };
        storage.create_step_run(&sr).await.unwrap();

        // Insert a recent run.
        let new_run = WorkflowRun {
            id: "new-run".into(),
            workflow_id: "wf-1".into(),
            workflow_name: "test".into(),
            status: WorkflowRunStatus::Completed,
            input: None,
            output: None,
            error: None,
            triggered_by: "test".into(),
            created_at: now_ms,
            started_at: Some(now_ms),
            completed_at: Some(now_ms),
        };
        storage.create_run(&new_run).await.unwrap();

        // Insert a running run (should not be evicted regardless of age).
        let running_run = WorkflowRun {
            id: "running-run".into(),
            workflow_id: "wf-1".into(),
            workflow_name: "test".into(),
            status: WorkflowRunStatus::Running,
            input: None,
            output: None,
            error: None,
            triggered_by: "test".into(),
            created_at: now_ms - 7_200_000,
            started_at: Some(now_ms - 7_200_000),
            completed_at: None,
        };
        storage.create_run(&running_run).await.unwrap();

        assert_eq!(storage.run_count(), 3);
        assert_eq!(storage.step_run_count(), 1);

        // Evict runs older than 1 hour.
        let evicted = storage.evict_older_than(Duration::from_secs(3600));
        assert_eq!(evicted, 1); // only old_run
        assert_eq!(storage.run_count(), 2);
        assert_eq!(storage.step_run_count(), 0); // step run cleaned up
        assert!(storage.get_run("new-run").await.unwrap().is_some());
        assert!(storage.get_run("running-run").await.unwrap().is_some());
        assert!(storage.get_run("old-run").await.unwrap().is_none());
    }

    // --- SQLite storage tests ---

    #[cfg(feature = "sqlite")]
    mod sqlite_tests {
        use super::*;

        #[tokio::test]
        async fn sqlite_definition_roundtrip() {
            let storage = SqliteWorkflowStorage::in_memory().unwrap();
            let def = definition(vec![step("a", &[]), step("b", &["a"])]);

            storage.create_definition(&def).await.unwrap();
            let loaded = storage.get_definition("wf-1").await.unwrap().unwrap();
            assert_eq!(loaded.name, "test-workflow");
            assert_eq!(loaded.steps.len(), 2);

            let list = storage.list_definitions(10, 0).await.unwrap();
            assert_eq!(list.len(), 1);

            assert!(storage.delete_definition("wf-1").await.unwrap());
            assert!(storage.get_definition("wf-1").await.unwrap().is_none());
        }

        #[tokio::test]
        async fn sqlite_run_roundtrip() {
            let storage = SqliteWorkflowStorage::in_memory().unwrap();
            let run = WorkflowRun {
                id: "run-1".into(),
                workflow_id: "wf-1".into(),
                workflow_name: "test".into(),
                status: WorkflowRunStatus::Completed,
                input: Some(serde_json::json!({"k": "v"})),
                output: Some(serde_json::json!({"result": 42})),
                error: None,
                triggered_by: "test".into(),
                created_at: 1000,
                started_at: Some(1001),
                completed_at: Some(1010),
            };

            storage.create_run(&run).await.unwrap();
            let loaded = storage.get_run("run-1").await.unwrap().unwrap();
            assert_eq!(loaded.status, WorkflowRunStatus::Completed);
            assert_eq!(loaded.output.unwrap()["result"], 42);

            let runs = storage.list_runs(Some("wf-1"), 10, 0).await.unwrap();
            assert_eq!(runs.len(), 1);
        }

        #[tokio::test]
        async fn sqlite_step_run_roundtrip() {
            let storage = SqliteWorkflowStorage::in_memory().unwrap();
            let sr = StepRun {
                id: "sr-1".into(),
                run_id: "run-1".into(),
                step_id: "step-a".into(),
                step_name: "a".into(),
                status: StepRunStatus::Completed,
                input: None,
                output: Some(serde_json::json!("hello")),
                error: None,
                attempt: 2,
                started_at: Some(100),
                completed_at: Some(200),
                duration_ms: Some(100),
            };

            storage.create_step_run(&sr).await.unwrap();
            let loaded = storage.get_step_runs_for_run("run-1").await.unwrap();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].attempt, 2);
            assert_eq!(loaded[0].output, Some(serde_json::json!("hello")));
        }

        #[tokio::test]
        async fn sqlite_engine_full_workflow() {
            let storage = Arc::new(SqliteWorkflowStorage::in_memory().unwrap());
            let executor = Arc::new(NoopExecutor);
            let engine =
                WorkflowEngine::new(storage.clone(), executor, WorkflowEngineConfig::default());

            let mut a = step("a", &[]);
            a.config = serde_json::json!("step-a-output");
            let def = definition(vec![a, step("b", &["a"])]);

            let run = engine.execute(&def, None, "test").await.unwrap();
            assert_eq!(run.status, WorkflowRunStatus::Completed);

            // Verify persisted run.
            let loaded = storage.get_run(&run.id).await.unwrap().unwrap();
            assert_eq!(loaded.status, WorkflowRunStatus::Completed);

            // Verify step runs persisted.
            let step_runs = storage.get_step_runs_for_run(&run.id).await.unwrap();
            assert_eq!(step_runs.len(), 2);
        }
    }
}
