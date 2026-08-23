# Migrating SecureYeoman to majra

> ⚠ **FROZEN at this consumer's adoption point — not current guidance.**
>
> This guide is kept as a record of how secureyeoman migrated, not as a recipe to
> follow verbatim today. majra has had breaking changes since; the inline ⚠
> notes below flag the snippets a reader would otherwise copy and be bitten by.
> For the current contract see
> [`migration-2.6.9.md`](migration-2.6.9.md) and the module headers in `src/`.



> This guide covers integrating SecureYeoman with majra's Cyrius modules.
> Since majra is now a Cyrius library, integration is via `include` directives
> in the consumer's Cyrius source.

## Modules to adopt

| SY component | majra module | Function prefix |
|-------------|-------------|-----------------|
| EventDispatcher | `pubsub.cyr` | `pubsub_*` |
| Rate limiter | `ratelimit.cyr` | `ratelimit_*` |
| HeartbeatManager | `heartbeat.cyr` | `chb_*` |
| Webhook retry queue | `queue.cyr` | `mq_*` |
| Workflow engine | `dag.cyr` | `workflow_*` |
| Multi-tenant scoping | `namespace.cyr` | `namespace_*` |
| A2A relay | `relay.cyr` | `relay_*` |
| Encrypted IPC | `ipc_encrypted.cyr` | `encrypted_ipc_*` |

## Phase 1: Pub/Sub + Rate Limiting

```cyrius
include "src/pubsub.cyr"
include "src/ratelimit.cyr"
include "src/namespace.cyr"

var ns = namespace_new(tenant_id);
# ⚠ SINCE 2.6.9: namespace_new VALIDATES and returns 0 for a prefix containing
# '/', ':', '#', '+' or a control byte — and a tenant id is exactly the value
# likely to carry one. Check it; namespace_topic/_key return 0 on a 0 namespace
# rather than dereferencing, but you will silently get no topic.
if (ns == 0) { return 1; }
var ps = pubsub_new();
var rl = ratelimit_new(100000, 50);

# Scoped event dispatch
pubsub_publish(ps, str_data(namespace_topic(ns, "events/created")), payload);

# Scoped rate limiting
if (ratelimit_check(rl, str_data(namespace_key(ns, "api"))) == 0) {
    # rate limited
}
```

## Phase 2: Heartbeat + Queue

```cyrius
include "src/heartbeat.cyr"
include "src/queue.cyr"

var tracker = chb_tracker_new(heartbeat_config_new(15000000000, 45000000000, 3));
chb_register(tracker, "node-1", 0);

var mq = mq_new("webhook-delivery", 10);
mq_enqueue(mq, PRIORITY_NORMAL, webhook_payload);
var job = mq_dequeue(mq);
mq_complete(mq, job);
```

## Phase 3: DAG Workflow Engine

```cyrius
include "src/dag.cyr"

var def = workflow_def_new("sy-workflow", "SecureYeoman pipeline");
var s1 = workflow_step_new("validate", "validate input");
var s2 = workflow_step_new("process", "process request");
step_add_dep(s2, "validate");
workflow_def_add_step(def, s1);
workflow_def_add_step(def, s2);

var status = workflow_execute(def, &sy_step_executor, input_data);
```

## Phase 4: Distributed Backends

```cyrius
include "src/redis_backend.cyr"
include "src/postgres_backend.cyr"

# ⚠ MISLEADING: majra has NO Redis-backed rate limiter, and never has.
# ratelimit_check is purely in-process — a lookup in the limiter's own hashmap.
# The redis_* calls below are a key-prefixing example, not distributed limiting.
# Cross-process limiting would have to be built on redis_eval + a Lua script.
# Redis key prefixing
var rc = redis_connect_default();
redis_set_prefix(rc, "sy:rl:");

# PostgreSQL workflow storage
# ⚠ LOOPBACK ONLY. majra's PostgreSQL client speaks PLAINTEXT: no SSLRequest,
# no TLS, and cleartext-password auth only — the password and every query and
# result row cross the wire unencrypted. It fails closed on SCRAM rather than
# downgrading. Under a "Distributed Backends" heading this is the one shape the
# module forbids: do not point it across an untrusted network.
var pg = pg_connect("127.0.0.1", 5432, "postgres", "sydb", "password");
pg_init_workflow_tables(pg);
```
