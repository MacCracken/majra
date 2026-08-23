# Migrating Ifran to majra

> ⚠ **FROZEN at this consumer's adoption point — not current guidance.**
>
> This guide is kept as a record of how ifran migrated, not as a recipe to
> follow verbatim today. majra has had breaking changes since; the inline ⚠
> notes below flag the snippets a reader would otherwise copy and be bitten by.
> For the current contract see
> [`migration-2.6.9.md`](migration-2.6.9.md) and the module headers in `src/`.



> Ifran manages model lifecycle (download, conversion, quantisation, indexing).
> This guide covers replacing Ifran's internal queue, pub/sub, and heartbeat
> with majra Cyrius modules.

## Modules to adopt

| Ifran component | majra module | Function prefix |
|----------------|-------------|-----------------|
| Job queue (training, indexing) | `queue.cyr` | `mq_*` |
| Model event bus | `pubsub.cyr` | `pubsub_*` |
| Node health tracking | `heartbeat.cyr` | `chb_*` |
| GPU resource filtering | `fleet.cyr` | `fleet_*` |

## Step 1: Replace the job queue

```cyrius
include "src/queue.cyr"

# Create queue with max 4 concurrent jobs
var mq = mq_new("gpu-training", 4);

# Enqueue a training job (priority + payload pointer)
var job = mq_enqueue(mq, PRIORITY_HIGH, job_data);

# Dequeue highest priority
var next = mq_dequeue(mq);
# ⚠ mq_dequeue returns 0 in two ordinary cases: the queue is empty, AND the
# queue is at its concurrency cap — which this very example sets to 4.
# mq_complete has no null guard, so check before using it.
if (next == 0) { return 0; }
# ... process ...
mq_complete(mq, next);
```

## Step 2: Replace the model event bus

```cyrius
include "src/pubsub.cyr"

var events = pubsub_new();

# Subscribe to all events for a model
var ch = pubsub_subscribe_pattern(events, "models/llama-70b/#");

# Publish
pubsub_publish(events, "models/llama-70b/training/started", event_data);

# Receive (blocking)
var msg = chan_recv(ch);
```

## Step 3: Replace node health tracking

```cyrius
include "src/heartbeat.cyr"

var tracker = chb_tracker_new(heartbeat_config_new(30000000000, 90000000000, 0));

# Register GPU node with telemetry
var gpus = vec_new();
vec_push(gpus, gpu_telemetry_new(0, 0, 80000, 3500));
chb_register_with_telemetry(tracker, "gpu-node-0", 0, gpus);

# Periodic sweep
chb_update_statuses(tracker);
var stats = chb_fleet_stats(tracker);
```

## Step 4: Fleet scheduling (multi-node GPU clusters)

```cyrius
include "src/fleet.cyr"

var cfg = fleet_config_new(10, 5, 4);
var fleet = fleet_new(cfg);

fleet_register_node(fleet, "gpu-0");
fleet_register_node(fleet, "gpu-1");

# Submit — routed to least-loaded node
fleet_submit(fleet, PRIORITY_HIGH, job_data);

# Periodic rebalancing
fleet_rebalance(fleet);
```

## Step 5: PostgreSQL persistence

```cyrius
include "src/postgres_backend.cyr"

# ⚠ LOOPBACK ONLY — plaintext protocol, cleartext-password auth, no TLS.
# See src/postgres_backend.cyr's header before using this off-host.
var pg = pg_connect("127.0.0.1", 5432, "postgres", "ifran", "password");
pg_init_workflow_tables(pg);
pg_save_workflow_def(pg, "training-pipeline", "GPU training", "[]");
```
