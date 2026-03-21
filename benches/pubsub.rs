use criterion::{Criterion, criterion_group, criterion_main};
use majra::pubsub::matches_pattern;

fn bench_pattern_matching(c: &mut Criterion) {
    c.bench_function("matches_pattern exact", |b| {
        b.iter(|| matches_pattern("crew/abc/status", "crew/abc/status"))
    });

    c.bench_function("matches_pattern wildcard *", |b| {
        b.iter(|| matches_pattern("crew/*/status", "crew/abc/status"))
    });

    c.bench_function("matches_pattern wildcard #", |b| {
        b.iter(|| matches_pattern("crew/#", "crew/abc/tasks/1/status"))
    });
}

criterion_group!(benches, bench_pattern_matching);
criterion_main!(benches);
