# Testing Guide

## Running Tests

```bash
# All tests (default features)
cargo test

# All tests (all features including sqlite)
cargo test --all-features

# No default features (envelope + error + metrics only)
cargo test --no-default-features

# Single module
cargo test --all-features pubsub
cargo test --all-features queue
cargo test --all-features heartbeat
```

## Test Categories

| Category | Location | Count |
|----------|----------|-------|
| Unit tests | `src/*.rs` under `#[cfg(test)]` | ~100 |
| Integration tests | `tests/integration.rs` | 5 |
| Doc tests | `src/pubsub.rs` (pattern matching example) | 2 |
| Concurrent tests | In each module's test section | ~15 |

## Coverage

```bash
make coverage
# Opens coverage/html/index.html
```

Target: 80% project, 75% patch (configured in `codecov.yml`).

## Fuzzing

Requires nightly Rust and `cargo-fuzz`:

```bash
# Run all fuzz targets (30 seconds each)
make fuzz

# Run a specific target longer
cargo +nightly fuzz run fuzz_queue -- -max_total_time=300
```

Fuzz targets:
- `fuzz_queue` — priority queue enqueue/dequeue with random data
- `fuzz_pubsub` — pattern matching and pub/sub with random strings
- `fuzz_heartbeat` — heartbeat operations with random sequences

## Benchmarks

```bash
cargo bench --all-features
```

Benchmarks in `benches/`:
- `pubsub.rs` — pattern matching performance
- `queue.rs` — priority queue throughput
- `concurrent.rs` — concurrent queue, heartbeat, rate limiter, managed queue

## Local CI

```bash
make check   # fmt + clippy + test + audit
```

This mirrors the GitHub Actions CI pipeline.
