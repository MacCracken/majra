# Majra Roadmap

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

## Open Items

- **QUIC transport** — waiting on sigil crypto port (TLS 1.3: X25519, AES-GCM, HKDF)
- **AES-256-GCM implementation** — AES-NI (x86_64) + aarch64 intrinsics for encrypted IPC (currently stub)
- **Relay dedup revisit** — original issue was cc3-era `map_get` after `map_set` in nested calls. cc5 is expected to handle this; re-enable the dedup count assertions in `test_relay` and confirm.
- **Barrier `arrive_and_wait` threading** — same historical root cause as relay dedup; revisit under cc5.

## Engineering Backlog

- Shared-memory IPC transport (mmap-based, deferred)
- **Soak tests** (50k-100k ops) — Cyrius 5.4.x has `lib/thread.cyr` + futex support; viable now
- `lib/patra.cyr` integration for persistent job queues (Cyrius 5.4.8 stdlib ships patra 1.1.1 vendored)
- Evaluate `lib/http_server.cyr` (new in 5.4.x) for an admin/metrics endpoint

## Non-goals

- **Application-level business logic** — majra provides primitives, consumers define semantics
- **Message broker replacement** — in-process library, use Redis backend for cross-process
- **LLVM/Cargo dependency** — Cyrius compiles directly to machine code
