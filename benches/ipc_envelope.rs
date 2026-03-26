use criterion::{Criterion, criterion_group, criterion_main};

use majra::envelope::{Envelope, Target};

fn bench_envelope(c: &mut Criterion) {
    c.bench_function("envelope_new", |b| {
        b.iter(|| {
            Envelope::new(
                "node-1",
                Target::Topic("crew/events".into()),
                serde_json::json!({"status": "ok", "progress": 42}),
            )
        });
    });

    c.bench_function("envelope_serialize_roundtrip", |b| {
        let env = Envelope::new(
            "node-1",
            Target::Topic("crew/events/status".into()),
            serde_json::json!({"status": "ok", "progress": 42, "data": [1,2,3]}),
        );
        b.iter(|| {
            let json = serde_json::to_vec(&env).unwrap();
            let _decoded: Envelope = serde_json::from_slice(&json).unwrap();
        });
    });
}

#[cfg(feature = "ipc")]
fn bench_ipc(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    c.bench_function("ipc_roundtrip_small_payload", |b| {
        let path = std::env::temp_dir().join(format!("majra-bench-{}.sock", uuid::Uuid::new_v4()));

        let server = rt.block_on(async {
            majra::ipc::IpcServer::bind(&path).unwrap()
        });

        // Spawn server echo loop.
        let path_clone = path.clone();
        let server_handle = rt.spawn(async move {
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

        // Create client once.
        let client_path = path_clone;
        let mut client = rt.block_on(async {
            majra::ipc::IpcConnection::connect(&client_path)
                .await
                .unwrap()
        });

        let payload = serde_json::json!({"hello": "world", "n": 42});

        b.iter(|| {
            rt.block_on(async {
                client.send(&payload).await.unwrap();
                let _resp = client.recv().await.unwrap();
            });
        });

        server_handle.abort();
        let _ = std::fs::remove_file(&path);
    });

    c.bench_function("ipc_roundtrip_large_payload", |b| {
        let path = std::env::temp_dir().join(format!("majra-bench-{}.sock", uuid::Uuid::new_v4()));

        let server = rt.block_on(async {
            majra::ipc::IpcServer::bind(&path).unwrap()
        });

        let path_clone = path.clone();
        let server_handle = rt.spawn(async move {
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

        let mut client = rt.block_on(async {
            majra::ipc::IpcConnection::connect(&path_clone)
                .await
                .unwrap()
        });

        let payload = serde_json::json!({
            "data": "x".repeat(4096),
            "metadata": {"type": "bulk", "count": 1000}
        });

        b.iter(|| {
            rt.block_on(async {
                client.send(&payload).await.unwrap();
                let _resp = client.recv().await.unwrap();
            });
        });

        server_handle.abort();
        let _ = std::fs::remove_file(&path);
    });
}

#[cfg(not(feature = "ipc"))]
fn bench_ipc(_c: &mut Criterion) {}

criterion_group!(benches, bench_envelope, bench_ipc);
criterion_main!(benches);
