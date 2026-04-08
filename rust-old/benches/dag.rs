use criterion::{Criterion, criterion_group, criterion_main};
use std::sync::Arc;

use majra::dag::{
    ErrorPolicy, InMemoryWorkflowStorage, RetryPolicy, StepExecutor, TriggerMode, WorkflowContext,
    WorkflowDefinition, WorkflowEngine, WorkflowEngineConfig, WorkflowStep,
};

fn make_step(id: &str, deps: &[&str]) -> WorkflowStep {
    WorkflowStep {
        id: id.into(),
        name: id.into(),
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        trigger_mode: TriggerMode::All,
        config: serde_json::json!(id),
        error_policy: ErrorPolicy::default(),
        retry_policy: RetryPolicy::default(),
    }
}

fn make_definition(steps: Vec<WorkflowStep>) -> WorkflowDefinition {
    WorkflowDefinition {
        id: "bench-wf".into(),
        name: "bench".into(),
        description: None,
        steps,
        enabled: true,
        version: 1,
        created_by: "bench".into(),
        created_at: 0,
        updated_at: 0,
    }
}

struct NoopExecutor;

#[async_trait::async_trait]
impl StepExecutor for NoopExecutor {
    async fn execute(
        &self,
        step: &WorkflowStep,
        _ctx: &WorkflowContext,
    ) -> Result<serde_json::Value, String> {
        Ok(step.config.clone())
    }
}

fn bench_topological_sort(c: &mut Criterion) {
    // Linear chain of 100 steps.
    let linear: Vec<WorkflowStep> = (0..100)
        .map(|i| {
            if i == 0 {
                make_step(&format!("s{i}"), &[])
            } else {
                make_step(&format!("s{i}"), &[&format!("s{}", i - 1)])
            }
        })
        .collect();

    c.bench_function("dag_tier_sort_linear_100", |b| {
        b.iter(|| {
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::topological_sort_tiers(&linear)
                .unwrap()
        });
    });

    // Wide DAG: 1 root → 50 parallel → 1 sink.
    let mut wide = vec![make_step("root", &[])];
    for i in 0..50 {
        wide.push(make_step(&format!("p{i}"), &["root"]));
    }
    let dep_ids: Vec<String> = (0..50).map(|i| format!("p{i}")).collect();
    let dep_refs: Vec<&str> = dep_ids.iter().map(String::as_str).collect();
    wide.push(make_step("sink", &dep_refs));

    c.bench_function("dag_tier_sort_wide_52", |b| {
        b.iter(|| {
            WorkflowEngine::<InMemoryWorkflowStorage, NoopExecutor>::topological_sort_tiers(&wide)
                .unwrap()
        });
    });
}

fn bench_engine_execution(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    // 10-step linear chain with noop executor.
    let steps: Vec<WorkflowStep> = (0..10)
        .map(|i| {
            if i == 0 {
                make_step(&format!("s{i}"), &[])
            } else {
                make_step(&format!("s{i}"), &[&format!("s{}", i - 1)])
            }
        })
        .collect();
    let def = make_definition(steps);

    c.bench_function("dag_engine_execute_linear_10", |b| {
        b.iter(|| {
            rt.block_on(async {
                let storage = Arc::new(InMemoryWorkflowStorage::new());
                let executor = Arc::new(NoopExecutor);
                let engine =
                    WorkflowEngine::new(storage, executor, WorkflowEngineConfig::default());
                engine.execute(&def, None, "bench").await.unwrap()
            })
        });
    });

    // Diamond: root → {a,b,c,d,e} → sink
    let mut diamond_steps = vec![make_step("root", &[])];
    for i in 0..5 {
        diamond_steps.push(make_step(&format!("m{i}"), &["root"]));
    }
    diamond_steps.push(make_step("sink", &["m0", "m1", "m2", "m3", "m4"]));
    let diamond_def = make_definition(diamond_steps);

    c.bench_function("dag_engine_execute_diamond_7", |b| {
        b.iter(|| {
            rt.block_on(async {
                let storage = Arc::new(InMemoryWorkflowStorage::new());
                let executor = Arc::new(NoopExecutor);
                let engine =
                    WorkflowEngine::new(storage, executor, WorkflowEngineConfig::default());
                engine.execute(&diamond_def, None, "bench").await.unwrap()
            })
        });
    });
}

criterion_group!(benches, bench_topological_sort, bench_engine_execution);
criterion_main!(benches);
