//! PostgreSQL-backed workflow storage and queue persistence.
//!
//! ## Feature flag
//!
//! Requires the `postgres` feature.
//!
//! ## Setup
//!
//! ```ignore
//! use majra::postgres_backend::PostgresWorkflowStorage;
//!
//! let storage = PostgresWorkflowStorage::connect("postgresql://user:pass@localhost/majra").await?;
//! ```

use async_trait::async_trait;
use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
use tokio_postgres::NoTls;
use tracing::debug;

use crate::dag::{
    StepRun, StepRunStatus, WorkflowDefinition, WorkflowRun, WorkflowRunStatus, WorkflowStep,
    WorkflowStorage,
};
use crate::error::MajraError;

fn pg_err(e: impl std::fmt::Display) -> MajraError {
    MajraError::WorkflowStorage(e.to_string())
}

/// PostgreSQL-backed workflow storage using a connection pool.
///
/// Tables are created automatically on [`connect`](PostgresWorkflowStorage::connect).
/// Uses `deadpool-postgres` for async connection pooling.
pub struct PostgresWorkflowStorage {
    pool: Pool,
}

impl PostgresWorkflowStorage {
    /// Connect to PostgreSQL and create tables if they don't exist.
    ///
    /// Uses a default pool size of 16 connections. For custom pool sizing,
    /// use [`from_pool`](Self::from_pool).
    pub async fn connect(connection_string: &str) -> crate::error::Result<Self> {
        Self::connect_with_pool_size(connection_string, 16).await
    }

    /// Connect with a custom connection pool size.
    pub async fn connect_with_pool_size(
        connection_string: &str,
        max_pool_size: usize,
    ) -> crate::error::Result<Self> {
        let pg_config: tokio_postgres::Config = connection_string.parse().map_err(pg_err)?;
        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        };
        let mgr = Manager::from_config(pg_config, NoTls, mgr_config);
        let pool = Pool::builder(mgr)
            .max_size(max_pool_size)
            .build()
            .map_err(pg_err)?;

        let storage = Self { pool };
        storage.init_tables().await?;
        debug!("postgres: workflow storage initialised");
        Ok(storage)
    }

    /// Create a storage backed by an existing connection pool.
    pub async fn from_pool(pool: Pool) -> crate::error::Result<Self> {
        let storage = Self { pool };
        storage.init_tables().await?;
        Ok(storage)
    }

    async fn init_tables(&self) -> crate::error::Result<()> {
        let client = self.pool.get().await.map_err(pg_err)?;
        client
            .batch_execute(
                "CREATE TABLE IF NOT EXISTS majra_workflow_definitions (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT,
                    steps_json TEXT NOT NULL DEFAULT '[]',
                    enabled BOOLEAN NOT NULL DEFAULT TRUE,
                    version INTEGER NOT NULL DEFAULT 1,
                    created_by TEXT NOT NULL DEFAULT 'system',
                    created_at BIGINT NOT NULL,
                    updated_at BIGINT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS majra_workflow_runs (
                    id TEXT PRIMARY KEY,
                    workflow_id TEXT NOT NULL,
                    workflow_name TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    input_json TEXT,
                    output_json TEXT,
                    error TEXT,
                    triggered_by TEXT NOT NULL DEFAULT 'manual',
                    created_at BIGINT NOT NULL,
                    started_at BIGINT,
                    completed_at BIGINT
                );

                CREATE TABLE IF NOT EXISTS majra_workflow_step_runs (
                    id TEXT PRIMARY KEY,
                    run_id TEXT NOT NULL,
                    step_id TEXT NOT NULL,
                    step_name TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'pending',
                    input_json TEXT,
                    output_json TEXT,
                    error TEXT,
                    attempt INTEGER NOT NULL DEFAULT 1,
                    started_at BIGINT,
                    completed_at BIGINT,
                    duration_ms BIGINT
                );

                CREATE INDEX IF NOT EXISTS idx_majra_wf_runs_workflow
                    ON majra_workflow_runs(workflow_id);
                CREATE INDEX IF NOT EXISTS idx_majra_wf_step_runs_run
                    ON majra_workflow_step_runs(run_id);",
            )
            .await
            .map_err(pg_err)?;
        Ok(())
    }
}

#[async_trait]
impl WorkflowStorage for PostgresWorkflowStorage {
    async fn create_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let steps_json = serde_json::to_string(&def.steps).map_err(pg_err)?;
        client
            .execute(
                "INSERT INTO majra_workflow_definitions
                 (id, name, description, steps_json, enabled, version, created_by, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
                 ON CONFLICT (id) DO UPDATE SET
                    name = EXCLUDED.name,
                    description = EXCLUDED.description,
                    steps_json = EXCLUDED.steps_json,
                    enabled = EXCLUDED.enabled,
                    version = EXCLUDED.version,
                    created_by = EXCLUDED.created_by,
                    updated_at = EXCLUDED.updated_at",
                &[
                    &def.id,
                    &def.name,
                    &def.description,
                    &steps_json,
                    &def.enabled,
                    &(def.version as i32),
                    &def.created_by,
                    &def.created_at,
                    &def.updated_at,
                ],
            )
            .await
            .map_err(pg_err)?;
        Ok(())
    }

    async fn get_definition(&self, id: &str) -> crate::error::Result<Option<WorkflowDefinition>> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let row = client
            .query_opt(
                "SELECT id, name, description, steps_json, enabled, version, created_by, created_at, updated_at
                 FROM majra_workflow_definitions WHERE id = $1",
                &[&id],
            )
            .await
            .map_err(pg_err)?;

        match row {
            Some(row) => {
                let steps_json: String = row.get(3);
                let steps: Vec<WorkflowStep> = serde_json::from_str(&steps_json).map_err(pg_err)?;
                Ok(Some(WorkflowDefinition {
                    id: row.get(0),
                    name: row.get(1),
                    description: row.get(2),
                    steps,
                    enabled: row.get(4),
                    version: row.get::<_, i32>(5) as u32,
                    created_by: row.get(6),
                    created_at: row.get(7),
                    updated_at: row.get(8),
                }))
            }
            None => Ok(None),
        }
    }

    async fn update_definition(&self, def: &WorkflowDefinition) -> crate::error::Result<()> {
        self.create_definition(def).await
    }

    async fn delete_definition(&self, id: &str) -> crate::error::Result<bool> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let count = client
            .execute(
                "DELETE FROM majra_workflow_definitions WHERE id = $1",
                &[&id],
            )
            .await
            .map_err(pg_err)?;
        Ok(count > 0)
    }

    async fn list_definitions(
        &self,
        limit: usize,
        offset: usize,
    ) -> crate::error::Result<Vec<WorkflowDefinition>> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let rows = client
            .query(
                "SELECT id, name, description, steps_json, enabled, version, created_by, created_at, updated_at
                 FROM majra_workflow_definitions ORDER BY created_at DESC LIMIT $1 OFFSET $2",
                &[&(limit as i64), &(offset as i64)],
            )
            .await
            .map_err(pg_err)?;

        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let steps_json: String = row.get(3);
            let steps: Vec<WorkflowStep> = serde_json::from_str(&steps_json).map_err(pg_err)?;
            result.push(WorkflowDefinition {
                id: row.get(0),
                name: row.get(1),
                description: row.get(2),
                steps,
                enabled: row.get(4),
                version: row.get::<_, i32>(5) as u32,
                created_by: row.get(6),
                created_at: row.get(7),
                updated_at: row.get(8),
            });
        }
        Ok(result)
    }

    async fn create_run(&self, run: &WorkflowRun) -> crate::error::Result<()> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let input_json = run.input.as_ref().map(|v| v.to_string());
        let output_json = run.output.as_ref().map(|v| v.to_string());
        client
            .execute(
                "INSERT INTO majra_workflow_runs
                 (id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                 ON CONFLICT (id) DO UPDATE SET
                    status = EXCLUDED.status,
                    output_json = EXCLUDED.output_json,
                    error = EXCLUDED.error,
                    started_at = EXCLUDED.started_at,
                    completed_at = EXCLUDED.completed_at",
                &[
                    &run.id,
                    &run.workflow_id,
                    &run.workflow_name,
                    &run.status.to_string(),
                    &input_json,
                    &output_json,
                    &run.error,
                    &run.triggered_by,
                    &run.created_at,
                    &run.started_at,
                    &run.completed_at,
                ],
            )
            .await
            .map_err(pg_err)?;
        Ok(())
    }

    async fn get_run(&self, id: &str) -> crate::error::Result<Option<WorkflowRun>> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let row = client
            .query_opt(
                "SELECT id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at
                 FROM majra_workflow_runs WHERE id = $1",
                &[&id],
            )
            .await
            .map_err(pg_err)?;

        match row {
            Some(row) => Ok(Some(parse_run_row(&row)?)),
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
        let client = self.pool.get().await.map_err(pg_err)?;
        let rows = match workflow_id {
            Some(wid) => {
                client
                    .query(
                        "SELECT id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at
                         FROM majra_workflow_runs WHERE workflow_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
                        &[&wid, &(limit as i64), &(offset as i64)],
                    )
                    .await
                    .map_err(pg_err)?
            }
            None => {
                client
                    .query(
                        "SELECT id, workflow_id, workflow_name, status, input_json, output_json, error, triggered_by, created_at, started_at, completed_at
                         FROM majra_workflow_runs ORDER BY created_at DESC LIMIT $1 OFFSET $2",
                        &[&(limit as i64), &(offset as i64)],
                    )
                    .await
                    .map_err(pg_err)?
            }
        };

        rows.iter().map(parse_run_row).collect()
    }

    async fn create_step_run(&self, sr: &StepRun) -> crate::error::Result<()> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let input_json = sr.input.as_ref().map(|v| v.to_string());
        let output_json = sr.output.as_ref().map(|v| v.to_string());
        client
            .execute(
                "INSERT INTO majra_workflow_step_runs
                 (id, run_id, step_id, step_name, status, input_json, output_json, error, attempt, started_at, completed_at, duration_ms)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
                 ON CONFLICT (id) DO UPDATE SET
                    status = EXCLUDED.status,
                    output_json = EXCLUDED.output_json,
                    error = EXCLUDED.error,
                    attempt = EXCLUDED.attempt,
                    completed_at = EXCLUDED.completed_at,
                    duration_ms = EXCLUDED.duration_ms",
                &[
                    &sr.id,
                    &sr.run_id,
                    &sr.step_id,
                    &sr.step_name,
                    &sr.status.to_string(),
                    &input_json,
                    &output_json,
                    &sr.error,
                    &(sr.attempt as i32),
                    &sr.started_at,
                    &sr.completed_at,
                    &sr.duration_ms,
                ],
            )
            .await
            .map_err(pg_err)?;
        Ok(())
    }

    async fn update_step_run(&self, sr: &StepRun) -> crate::error::Result<()> {
        self.create_step_run(sr).await
    }

    async fn get_step_runs_for_run(&self, run_id: &str) -> crate::error::Result<Vec<StepRun>> {
        let client = self.pool.get().await.map_err(pg_err)?;
        let rows = client
            .query(
                "SELECT id, run_id, step_id, step_name, status, input_json, output_json, error, attempt, started_at, completed_at, duration_ms
                 FROM majra_workflow_step_runs WHERE run_id = $1 ORDER BY started_at",
                &[&run_id],
            )
            .await
            .map_err(pg_err)?;

        rows.iter().map(parse_step_run_row).collect()
    }
}

fn parse_status(s: &str) -> WorkflowRunStatus {
    match s {
        "pending" => WorkflowRunStatus::Pending,
        "running" => WorkflowRunStatus::Running,
        "completed" => WorkflowRunStatus::Completed,
        "failed" => WorkflowRunStatus::Failed,
        "cancelled" => WorkflowRunStatus::Cancelled,
        _ => WorkflowRunStatus::Pending,
    }
}

fn parse_step_status(s: &str) -> StepRunStatus {
    match s {
        "pending" => StepRunStatus::Pending,
        "running" => StepRunStatus::Running,
        "completed" => StepRunStatus::Completed,
        "failed" => StepRunStatus::Failed,
        "skipped" => StepRunStatus::Skipped,
        _ => StepRunStatus::Pending,
    }
}

fn parse_json_opt(s: &Option<String>) -> Option<serde_json::Value> {
    s.as_ref().and_then(|s| serde_json::from_str(s).ok())
}

fn parse_run_row(row: &tokio_postgres::Row) -> crate::error::Result<WorkflowRun> {
    let status_str: String = row.get(3);
    let input_json: Option<String> = row.get(4);
    let output_json: Option<String> = row.get(5);
    Ok(WorkflowRun {
        id: row.get(0),
        workflow_id: row.get(1),
        workflow_name: row.get(2),
        status: parse_status(&status_str),
        input: parse_json_opt(&input_json),
        output: parse_json_opt(&output_json),
        error: row.get(6),
        triggered_by: row.get(7),
        created_at: row.get(8),
        started_at: row.get(9),
        completed_at: row.get(10),
    })
}

fn parse_step_run_row(row: &tokio_postgres::Row) -> crate::error::Result<StepRun> {
    let status_str: String = row.get(4);
    let input_json: Option<String> = row.get(5);
    let output_json: Option<String> = row.get(6);
    Ok(StepRun {
        id: row.get(0),
        run_id: row.get(1),
        step_id: row.get(2),
        step_name: row.get(3),
        status: parse_step_status(&status_str),
        input: parse_json_opt(&input_json),
        output: parse_json_opt(&output_json),
        error: row.get(7),
        attempt: row.get::<_, i32>(8) as u32,
        started_at: row.get(9),
        completed_at: row.get(10),
        duration_ms: row.get(11),
    })
}
