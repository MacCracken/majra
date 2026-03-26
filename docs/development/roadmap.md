# Majra Roadmap

Completed items are in [CHANGELOG.md](../../CHANGELOG.md).

No open items.

## Engineering Backlog

- Shared-memory IPC transport (30-40x faster than UDS; deferred — requires unsafe for mmap)

## Non-goals

- **Application-level business logic** — majra provides primitives, consumers define semantics
- **Message broker replacement** — in-process library, use Redis backend for cross-process
