---
name: Migrating to majra 2.6.9 / 2.6.10
description: What changed for consumers across the two hardening releases — three signature changes, three behavioural changes, and one on-disk format change.
type: guide
---

# Migrating to 2.6.9 / 2.6.10

The 2.6.9 and 2.6.10 hardening releases changed a small number of public
signatures and behaviours. This is the complete list; nothing else in the
consumer-facing surface moved.

Both releases are covered together because 2.6.10 repairs 2.6.9 — **if you are
on 2.6.8 or earlier, go straight to 2.6.10.** 2.6.9 on its own shipped a
critical rate-limiter bypass, a socket-permission fix that did nothing, and a
possible self-deadlock in encrypted IPC. See
[`../audit/2026-08-22-audit-pass2.md`](../audit/2026-08-22-audit-pass2.md).

---

## 1. `encrypted_ipc_new` requires a role

```
# before
var e = encrypted_ipc_new(fd, key);

# after — one endpoint INITIATOR, the other RESPONDER
var e = encrypted_ipc_new(fd, key, ENCRYPTED_IPC_INITIATOR);
var e = encrypted_ipc_new(fd, key, ENCRYPTED_IPC_RESPONDER);
```

**Why it is not optional.** Both endpoints share one pre-shared key, and each
previously derived nonces from its own counter starting at 0 with nothing in
the nonce distinguishing direction — so every message pair at the same counter
reused `(key, nonce)`. Under AES-GCM that is keystream reuse *and* GHASH subkey
leakage: confidentiality and integrity both fall. The role now occupies nonce
byte 0, giving the two directions disjoint nonce spaces.

**Pick roles deterministically.** Whichever side accepts is conventionally the
responder. If both ends pass the same role the channel is no safer than before,
and majra cannot detect it.

Also new on this type:

- **Replay protection.** The receiver requires the peer's counter to strictly
  increase and the nonce to carry the *peer's* role. A captured frame no longer
  replays.
- `encrypted_ipc_rekey` now returns a `Result` and **refuses a key identical to
  the installed one** — resetting the counter is only safe because the key
  changed.
- `encrypted_ipc_free(e)` releases the handle. Call it after
  `encrypted_ipc_close(e)`.
- `encrypted_ipc_payload_free(r)` releases what `encrypted_ipc_recv` returns.

## 2. `majra_admin_serve` takes a dotted-quad string

```
# before — and this did NOT bind to localhost
majra_admin_serve(admin, "127.0.0.1", 9090);

# after — same call, now actually parsed
majra_admin_serve(admin, "127.0.0.1", 9090);   # returns -1 on a bad address
```

The signature is unchanged; the behaviour is not. The address was previously
forwarded to `sockaddr_in`, which wants a **packed integer**, so passing the
documented `"127.0.0.1"` stored the low 32 bits of a `char*` as the bind
address. For an endpoint with no authentication of its own, that binding is the
whole security boundary. It now parses the string and returns `-1` rather than
binding somewhere unintended.

**Check the return value.** A misspelled address is now a failure instead of a
silent bind to an arbitrary interface.

## 3. `transport_send` / `transport_recv` forward all three arguments

If you implement the transport vtable, your `send_fn` and `recv_fn` are now
called with **three** arguments, as the vtable contract has always documented:

```
fn my_send(self_ptr, data, len)      # len now actually arrives
fn my_recv(self_ptr, buf, buf_len)   # buf_len now actually arrives
```

Previously only two were forwarded, so an implementation written against the
documented signature read a garbage length off the stack. If you wrote your
implementation to work around that — ignoring the third parameter and deriving
the length some other way — remove the workaround.

## 4. `namespace_new` validates its prefix

```
var ns = namespace_new("acme");        # ok
var ns = namespace_new("acme/eu");     # now returns 0
```

Rejected: `/`, `:`, `#`, `+`, and control bytes.

**Why.** Namespaces are a tenant-isolation boundary. A `#` or `+` in a prefix
turned that tenant's own `namespace_wildcard` into a cross-tenant subscription
(`"a/#"` yields the wildcard `"a/#/#"`, whose embedded `#` matches every other
tenant under `a`). A `/` or `:` made prefixes non-prefix-free, so `acme` and
`acme/eu` collided and `acme`'s wildcard matched `acme/eu`'s entire topic space.

**Check for 0.** If your tenant identifiers can contain any of those bytes, map
them to a safe form before calling — percent-encoding or a hash — rather than
passing them through.

## 5. `mq_job_count` counts live jobs

It now returns jobs that have **not** reached a terminal state, because
`mq_complete` / `mq_fail` release the tracking record. It previously only ever
grew, since nothing removed entries — a long-lived queue grew until the process
died.

If you used it as a lifetime total, use `mq_total_completed` or your own
counter.

Related, from 2.6.10: terminating a job that is still **queued** is now a no-op.
Only a job that actually reached RUNNING is completed or failed.

## 6. `patra_queue`'s payload column is TEXT

The schema changed from `STR` (which patra caps at 255 bytes) to `TEXT`
(chain-page backed, uncapped). Enqueue previously accepted any length and
discarded patra's rejection, so a payload over 255 bytes was **silently dropped
while enqueue reported success**.

**Existing `.patra` files keep their old `STR` column.** 2.6.10 reads both, so
an old file still works — but it still has the 255-byte cap. To lift it:

```
# simplest: drain, delete, recreate
#   1. drain the old queue with patra_queue_dequeue until it returns 0
#   2. delete the .patra file
#   3. re-enqueue into a fresh queue
```

If you are on 2.6.9 specifically, **do not upgrade an existing file in place** —
2.6.9 misread the old column and every job dequeued with an empty payload while
still being marked running. 2.6.10 fixes that; go there directly.

## 7. Signed envelopes: signatures do not verify across the boundary

`_envelope_canonicalize` now encodes an absent field with a distinct length tag
(`0xFFFFFFFF`) rather than a zero length, because a null field and an empty
field previously canonicalised identically — so a broadcast envelope and one
addressed to `""` produced the same signing input and one signature valid for
both.

**Consequence**: a signature produced before 2.6.9 will not verify after it, and
vice versa. If you persist signed envelopes or verify across a version boundary,
re-sign them. In-flight envelopes between a 2.6.8 and a 2.6.9+ peer will fail
verification.

## New teardown functions

2.6.9 and 2.6.10 closed a large number of leaks, which required giving callers
a way to release what majra hands them. None are mandatory — not calling them
leaks, exactly as before — but they are available now:

| Function | Releases |
|---|---|
| `queue_item_free(item)` | a `QueueItem` you own outright |
| `patra_job_free(job)` | a dequeued patra job and its payload |
| `ipc_frame_free(f)` | a frame from `ipc_recv_frame` |
| `ws_frame_free(f)` | a frame from `ws_recv_frame` |
| `encrypted_ipc_payload_free(r)` | a payload from `encrypted_ipc_recv` |
| `encrypted_ipc_free(e)` | the `EncryptedIpc` handle, after close |
| `incoming_msg_free(inc)` | an `IncomingMessage` from `relay_receive_ex` |
| `signed_envelope_free(se)` | a `SignedEnvelope` (not the inner envelope) |
| `chb_node_free(ns)` | a `NodeState` copy from `chb_get` |
| `resp_reply_free(reply)` | a Str reply from the RESP parser |
| `ws_bridge_free(b)` | a `WsBridge` |
| `sliding_window_evict_stale(sw, ns)` | idle sliding-window entries |

## One ownership change worth knowing

`chb_get` now returns a **caller-owned copy** of the node state rather than the
tracker's internal pointer. The signature is unchanged. It had to change:
2.6.9 began freeing `NodeState` on eviction and deregistration, which turned a
merely-racy read into a use-after-free.

Release it with `chb_node_free`. Reading it without freeing leaks 40 bytes per
call.

## Nothing else moved

The pub/sub, queue, relay, barrier, DAG, fleet, metrics and counter APIs are
signature-compatible. `dist/majra.cyr` (the core profile) gained no breaking
change at all — every item above is in the `signed`, `admin` or `backends`
surface, except `namespace_new`, `mq_job_count` and the transport vtable.
