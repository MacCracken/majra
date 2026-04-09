# Dependency Watch

majra v2.0.0 (Cyrius) has **zero external dependencies**. All functionality is built
on the Cyrius standard library (vendored in `lib/`).

## Cyrius Standard Library Modules Used

| Module | Purpose |
|--------|---------|
| `string.cyr` | C string operations (strlen, streq, memcpy, memset) |
| `fmt.cyr` | Integer formatting (print_num, fmt_int) |
| `alloc.cyr` | Bump allocator (alloc, alloc_reset) |
| `freelist.cyr` | Free-list allocator with individual free (fl_alloc, fl_free) |
| `vec.cyr` | Dynamic i64 array (vec_new, vec_push, vec_get) |
| `str.cyr` | Fat string type (str_from, str_len, str_eq, str_builder) |
| `hashmap.cyr` | Hash table — string keys, i64 values (FNV-1a) |
| `syscalls.cyr` | Linux x86_64 syscall wrappers |
| `tagged.cyr` | Option/Result tagged unions (Ok, Err, Some, None) |
| `fnptr.cyr` | Function pointer dispatch (fncall0, fncall1, fncall2) |
| `thread.cyr` | Threads (clone), mutexes (futex), MPSC channels |
| `assert.cyr` | Test assertions (assert, assert_eq, assert_summary) |
| `bench.cyr` | Benchmarking (bench_new, bench_batch_start/stop, bench_report) |
| `net.cyr` | TCP/UDP sockets (sock_connect, sock_send, sock_recv) |

## Upgrade Considerations

- **Cyrius compiler upgrades**: when `cc2` is updated, recompile and re-run tests.
- **Stdlib changes**: if vendored `lib/` modules are updated from Cyrius, verify no
  function signatures changed.
- **sigil crypto port**: when sigil is ported to Cyrius, it will provide AES-256-GCM
  and TLS 1.3 primitives, enabling:
  - Full encrypted IPC (currently stub)
  - QUIC transport (currently deferred)
