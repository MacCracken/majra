# Migrating to majra 2.9.0

2.9.0 breaks two wire formats to close confirmed security defects, and changes
one return value so that an unsafe call fails closed. Everything else is
additive.

> **If you use encrypted IPC or signed envelopes, both ends upgrade together.**
> A 2.9.0 peer cannot talk to a 2.8.x peer, and 2.8.x signatures do not verify
> on 2.9.0. There is no compatibility mode — see
> [`semver.md`](../development/semver.md) § Security fixes that require a wire
> change for why.

## 1. Encrypted IPC: session handshake (wire break)

**What was wrong.** Every connection under one pre-shared key derived the same
AES key and restarted its nonce counter, so a frame captured on one connection
decrypted on any later one. 2.8.1 stopped the keystream reuse with a per-handle
nonce salt but could not stop the replay, which needs a handshake.

**What 2.9.0 does.** Each handle writes a 32-byte hello at construction
(`MJEIPCv1`, its role, a 16-byte random salt) and reads the peer's before its
first frame. The AES key is then `HKDF(PSK, initiator_salt || responder_salt,
"majra-eipc-session-v1")` — unique per connection, so a captured frame cannot
authenticate anywhere else.

**What you change.** Nothing in your code:
`encrypted_ipc_new(fd, key, role)` keeps its signature and does the exchange
for you. What changes is deployment:

| | |
|---|---|
| **Both ends** | must be 2.9.0. A 2.8.x peer reads the hello as a frame header and fails. |
| **Roles** | still mandatory and still opposite. A role clash is now caught at the handshake, on the first send, and reports the new `MAJRA_ERR_IPC_ROLE` instead of a generic `MAJRA_ERR_IPC`. |
| **First send/recv** | performs the exchange, so it can block where it did not before — the peer must be constructing its own handle. Construction itself still does not block. |
| **`encrypted_ipc_rekey`** | unchanged for callers. It installs the new PSK and re-derives over the salts already exchanged: no second handshake, and a reader parked in `recv` is not disturbed. |

## 2. Signed envelopes: domain prefix (wire break)

**What was wrong.** The signing input was the canonical envelope only, so a
signature over a majra envelope could be replayed as a signature over another
message signed with the same Ed25519 key.

**What 2.9.0 does.** The signed bytes begin with the 24 bytes
`majra/signed-envelope/v1`. The canonical length grows by 24.

**What you change.** Re-sign anything you stored. A 2.8.x signature returns 3
(bad signature) on 2.9.0, and vice versa. Verifier and signer upgrade together.

## 3. `signed_envelope_verify(se, 0)` now returns 4, not 0

**What was wrong.** With no `expected_pk`, a valid-looking envelope returned 0
— the same answer as an anchored pass. An attacker who can modify a message can
re-sign it with their own key and swap `signer_pk`, and it still returned 0.

**What 2.9.0 does.** That case returns `SIGNED_ENV_SELF_CONSISTENT` (4), which
is non-zero, so an unchanged `if (verify(se, 0) == 0)` now **fails closed**
rather than trusting a forgery.

**What you change.** Pick one:

```
# Anchored — what you want whenever the result gates an action.
if (signed_envelope_verify_anchored(se, expected_pk) == 0) { ... }

# Unanchored, deliberately: check the signer against your own trust store.
if (signed_envelope_verify(se, 0) == SIGNED_ENV_SELF_CONSISTENT) {
    if (my_trust_store_has(signed_envelope_signer_pk(se)) == 1) { ... }
}
```

`signed_envelope_verify_anchored(se, 0)` returns 2 (pk mismatch) rather than
passing, so the unanchored mode cannot be reached by accident.

## 4. Additive: things you can now free

Results majra handed back used to come off the bump allocator, which never
reclaims. They are freelist-backed now, and these release them. Calling them is
optional — memory that was leaking before simply leaks less — but a long-lived
process should.

| Call | Release with |
|---|---|
| `pg_query` / `pg_exec` rows | `pg_rows_free(rows)` — frees every cell too |
| Redis array replies (`redis_keys`, `redis_zpopmin`, string-valued EVAL) | `redis_array_free(arr)` |
| Redis arrays of integers or nested arrays | `redis_array_free_shallow(arr)` — the elements are values, not pointers |
| `hb_update_statuses` / `chb_update_statuses` | `hb_transitions_free(tr)` — the id pointers inside stay yours |

## 5. Additive: relay subscriber and message ownership

- `relay_unsubscribe(r, ch)` removes a channel from the fan-out; drain what is
  queued and `chan_close` it yourself. `relay_subscriber_count(r)` reports how
  many remain.
- A delivered `RelayMessage` is **shared** by every subscriber that received
  it. It carries a refcount now: `relay_msg_release(msg)` when done, and the
  last release frees it. `relay_msg_retain` / `relay_msg_refcount` are there for
  handing one on. A message no subscriber accepted is still freed by the relay,
  exactly as before.
- The struct grew from 64 to 72 bytes, with the refcount appended. Offsets 0-56
  are unchanged.

## 6. Additive: WebSocket Origin policy

`_ws_accept_upgrade` answers any Origin by default, exactly as through 2.8.2. A
server reachable from a browser should refuse cross-site handshakes:

```
fn my_origin_policy(origin) {
    if (origin == 0) { return 0; }                          # no Origin header
    if (streq(origin, "https://app.example") == 1) { return 1; }
    return 0;
}
ws_set_origin_check(&my_origin_policy);
```

A refused handshake gets `403 Forbidden` and never switches protocols.
`ws_set_origin_check(0)` clears the policy. The hook is process-wide; install it
once at startup.
