# Majra Roadmap

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

## Open Items

- **QUIC transport** — waiting on sigil crypto port (TLS 1.3: X25519, AES-GCM, HKDF)
- **AES-256-GCM implementation** — AES-NI (x86_64) + aarch64 intrinsics for encrypted IPC (currently stub)
- **Relay dedup fix** — hashmap lookup issue in nested call contexts (Cyrius compiler limitation)
- **Barrier `arrive_and_wait` threading** — blocked by same hashmap issue; non-blocking `cbarrier_arrive` works

## Engineering Backlog

- Shared-memory IPC transport (mmap-based, deferred)
- **Soak tests** (50k-100k ops) — revisit when Cyrius 3.0 ships (expected non-blocking channel recv / epoll integration)
- SQLite persistence (if Cyrius gets a SQLite binding)

## Non-goals

- **Application-level business logic** — majra provides primitives, consumers define semantics
- **Message broker replacement** — in-process library, use Redis backend for cross-process
- **LLVM/Cargo dependency** — Cyrius compiles directly to machine code
