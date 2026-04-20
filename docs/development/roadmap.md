# Majra Roadmap

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

## Open Items

- **QUIC transport** — waiting on sigil crypto port (TLS 1.3: X25519, HKDF — AES-GCM is in since sigil 2.8.4)
- **AES-NI hardware acceleration** — sigil currently ships software-only AES. AES-NI (x86_64) + pmull (aarch64) paths pending Cyrius inline-asm support. Majra will benefit transparently when sigil upgrades.

## Engineering Backlog

- Shared-memory IPC transport (mmap-based, deferred)
- **Soak tests** (50k-100k ops) — Cyrius 5.4.x has `lib/thread.cyr` + futex support; viable now
- `lib/patra.cyr` integration for persistent job queues (Cyrius 5.4.8 stdlib ships patra 1.1.1 vendored)
- Evaluate `lib/http_server.cyr` (new in 5.4.x) for an admin/metrics endpoint

## Non-goals

- **Application-level business logic** — majra provides primitives, consumers define semantics
- **Message broker replacement** — in-process library, use Redis backend for cross-process
- **LLVM/Cargo dependency** — Cyrius compiles directly to machine code
