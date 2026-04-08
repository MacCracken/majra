use criterion::{Criterion, criterion_group, criterion_main};

// ---------------------------------------------------------------------------
// IPC sustained throughput — measures throughput over many sequential messages
// ---------------------------------------------------------------------------

#[cfg(feature = "ipc")]
fn bench_ipc_sustained(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let mut group = c.benchmark_group("ipc_sustained");
    group.sample_size(20);
    group.measurement_time(std::time::Duration::from_secs(10));

    group.bench_function("1000_roundtrips_small", |b| {
        let path = std::env::temp_dir().join(format!("majra-bench-{}.sock", uuid::Uuid::new_v4()));

        let path_clone = path.clone();
        let server_handle = rt.spawn(async move {
            let server = majra::ipc::IpcServer::bind(&path_clone).unwrap();
            while let Ok(mut conn) = server.accept().await {
                tokio::spawn(async move {
                    while let Ok(msg) = conn.recv().await {
                        if conn.send(&msg).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });

        // Give the server a moment to bind.
        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut client =
            rt.block_on(async { majra::ipc::IpcConnection::connect(&path).await.unwrap() });

        let payload = serde_json::json!({"type": "heartbeat", "node": "gpu-0", "seq": 0});

        b.iter(|| {
            rt.block_on(async {
                for _ in 0..1000 {
                    client.send(&payload).await.unwrap();
                    let _resp = client.recv().await.unwrap();
                }
            });
        });

        server_handle.abort();
        let _ = std::fs::remove_file(&path);
    });

    group.bench_function("1000_roundtrips_large", |b| {
        let path = std::env::temp_dir().join(format!("majra-bench-{}.sock", uuid::Uuid::new_v4()));

        let path_clone = path.clone();
        let server_handle = rt.spawn(async move {
            let server = majra::ipc::IpcServer::bind(&path_clone).unwrap();
            while let Ok(mut conn) = server.accept().await {
                tokio::spawn(async move {
                    while let Ok(msg) = conn.recv().await {
                        if conn.send(&msg).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });

        std::thread::sleep(std::time::Duration::from_millis(50));

        let mut client =
            rt.block_on(async { majra::ipc::IpcConnection::connect(&path).await.unwrap() });

        let payload = serde_json::json!({
            "data": "x".repeat(4096),
            "metadata": {"type": "bulk_transfer", "batch_size": 1000},
            "telemetry": {
                "gpu_util": 87.5,
                "vram_used_mb": 6144,
                "temperature_c": 72.0
            }
        });

        b.iter(|| {
            rt.block_on(async {
                for _ in 0..1000 {
                    client.send(&payload).await.unwrap();
                    let _resp = client.recv().await.unwrap();
                }
            });
        });

        server_handle.abort();
        let _ = std::fs::remove_file(&path);
    });

    group.finish();
}

#[cfg(not(feature = "ipc"))]
fn bench_ipc_sustained(_c: &mut Criterion) {}

// ---------------------------------------------------------------------------
// SQLite on-disk — real file I/O benchmarks for persistence
// ---------------------------------------------------------------------------

#[cfg(feature = "sqlite")]
fn bench_sqlite_disk(c: &mut Criterion) {
    use majra::queue::persistence::SqliteBackend;
    use majra::queue::{JobState, ManagedItem, Priority, ResourceReq};

    let mut group = c.benchmark_group("sqlite_disk");
    group.sample_size(20);

    group.bench_function("persist_100_jobs", |b| {
        let dir = std::env::temp_dir().join(format!("majra-bench-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("bench.db");

        let backend = SqliteBackend::open(&db_path).unwrap();

        b.iter(|| {
            for i in 0..100u32 {
                let item: ManagedItem<serde_json::Value> = ManagedItem {
                    id: uuid::Uuid::new_v4(),
                    priority: Priority::Normal,
                    payload: serde_json::json!({"job": i, "model": "llama-70b"}),
                    resource_req: Some(ResourceReq {
                        gpu_count: 2,
                        vram_mb: 16000,
                    }),
                    state: JobState::Queued,
                    enqueued_at: Some(std::time::Instant::now()),
                    started_at: None,
                    finished_at: None,
                };
                backend.persist(&item).unwrap();
            }
        });

        let _ = std::fs::remove_dir_all(&dir);
    });

    group.bench_function("persist_and_load_100_jobs", |b| {
        let dir = std::env::temp_dir().join(format!("majra-bench-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("bench.db");

        b.iter(|| {
            // Fresh DB each iteration to measure full cycle.
            let backend = SqliteBackend::open(&db_path).unwrap();
            let mut ids = Vec::with_capacity(100);
            for i in 0..100u32 {
                let item: ManagedItem<serde_json::Value> = ManagedItem {
                    id: uuid::Uuid::new_v4(),
                    priority: Priority::Normal,
                    payload: serde_json::json!({"job": i}),
                    resource_req: None,
                    state: JobState::Queued,
                    enqueued_at: Some(std::time::Instant::now()),
                    started_at: None,
                    finished_at: None,
                };
                ids.push(item.id);
                backend.persist(&item).unwrap();
            }
            let loaded: Vec<ManagedItem<serde_json::Value>> = backend.load_all().unwrap();
            assert_eq!(loaded.len(), 100);

            // Cleanup for next iteration.
            backend.evict(&ids).unwrap();
        });

        let _ = std::fs::remove_dir_all(&dir);
    });

    group.bench_function("update_state_100_jobs", |b| {
        let dir = std::env::temp_dir().join(format!("majra-bench-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("bench.db");

        let backend = SqliteBackend::open(&db_path).unwrap();

        // Pre-populate.
        let mut ids = Vec::with_capacity(100);
        for i in 0..100u32 {
            let item: ManagedItem<serde_json::Value> = ManagedItem {
                id: uuid::Uuid::new_v4(),
                priority: Priority::Normal,
                payload: serde_json::json!({"job": i}),
                resource_req: None,
                state: JobState::Queued,
                enqueued_at: Some(std::time::Instant::now()),
                started_at: None,
                finished_at: None,
            };
            ids.push(item.id);
            backend.persist(&item).unwrap();
        }

        b.iter(|| {
            for id in &ids {
                backend.update_state(*id, JobState::Running).unwrap();
            }
            for id in &ids {
                backend.update_state(*id, JobState::Completed).unwrap();
            }
        });

        let _ = std::fs::remove_dir_all(&dir);
    });

    group.finish();
}

#[cfg(not(feature = "sqlite"))]
fn bench_sqlite_disk(_c: &mut Criterion) {}

// ---------------------------------------------------------------------------
// DAG SQLite on-disk — workflow storage benchmarks
// ---------------------------------------------------------------------------

#[cfg(all(feature = "dag", feature = "sqlite"))]
fn bench_dag_sqlite_disk(c: &mut Criterion) {
    use majra::dag::{
        ErrorPolicy, RetryPolicy, SqliteWorkflowStorage, TriggerMode, WorkflowDefinition,
        WorkflowStep, WorkflowStorage,
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("dag_sqlite_disk");
    group.sample_size(20);

    group.bench_function("save_and_load_workflow", |b| {
        let dir = std::env::temp_dir().join(format!("majra-bench-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("dag.db");

        let storage = rt.block_on(async { SqliteWorkflowStorage::open(&db_path).unwrap() });

        let now = chrono::Utc::now().timestamp_millis();
        let def = WorkflowDefinition {
            id: "bench-workflow".into(),
            name: "Benchmark Pipeline".into(),
            description: None,
            steps: vec![
                WorkflowStep {
                    id: "ingest".into(),
                    name: "Ingest Data".into(),
                    depends_on: vec![],
                    trigger_mode: TriggerMode::All,
                    config: serde_json::Value::Null,
                    error_policy: ErrorPolicy::Fail,
                    retry_policy: RetryPolicy {
                        max_attempts: 3,
                        backoff: std::time::Duration::from_millis(100),
                    },
                },
                WorkflowStep {
                    id: "train".into(),
                    name: "Train Model".into(),
                    depends_on: vec!["ingest".into()],
                    trigger_mode: TriggerMode::All,
                    config: serde_json::Value::Null,
                    error_policy: ErrorPolicy::Fail,
                    retry_policy: RetryPolicy::default(),
                },
                WorkflowStep {
                    id: "eval".into(),
                    name: "Evaluate".into(),
                    depends_on: vec!["train".into()],
                    trigger_mode: TriggerMode::All,
                    config: serde_json::Value::Null,
                    error_policy: ErrorPolicy::Continue,
                    retry_policy: RetryPolicy::default(),
                },
            ],
            enabled: true,
            version: 1,
            created_by: "bench".into(),
            created_at: now,
            updated_at: now,
        };

        b.iter(|| {
            rt.block_on(async {
                storage.create_definition(&def).await.unwrap();
                let loaded = storage.get_definition("bench-workflow").await.unwrap();
                assert!(loaded.is_some());
            });
        });

        let _ = std::fs::remove_dir_all(&dir);
    });

    group.finish();
}

#[cfg(not(all(feature = "dag", feature = "sqlite")))]
fn bench_dag_sqlite_disk(_c: &mut Criterion) {}

criterion_group!(
    benches,
    bench_ipc_sustained,
    bench_sqlite_disk,
    bench_dag_sqlite_disk
);
criterion_main!(benches);
