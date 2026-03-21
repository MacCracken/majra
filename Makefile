.PHONY: check fmt clippy test audit deny fuzz coverage build doc clean

# Run all CI checks locally
check: fmt clippy test audit

# Format check
fmt:
	cargo fmt --all -- --check

# Lint (zero warnings)
clippy:
	cargo clippy --all-features --all-targets -- -D warnings

# Run test suite
test:
	cargo test --all-features

# Security audit
audit:
	cargo audit

# Supply-chain checks (cargo-deny)
deny:
	cargo deny check

# Run fuzz targets (30 seconds each)
fuzz:
	cargo +nightly fuzz run fuzz_queue -- -max_total_time=30
	cargo +nightly fuzz run fuzz_pubsub -- -max_total_time=30
	cargo +nightly fuzz run fuzz_heartbeat -- -max_total_time=30

# Generate coverage report
coverage:
	cargo llvm-cov --all-features --html --output-dir coverage/
	@echo "Coverage report: coverage/html/index.html"

# Build release
build:
	cargo build --release --all-features

# Generate documentation
doc:
	RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

# Clean build artifacts
clean:
	cargo clean
	rm -rf coverage/
