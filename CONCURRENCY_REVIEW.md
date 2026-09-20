# Concurrency Review — Phase 14C

Date: 2026-09-20

## Executive Summary

The `ObservationPipeline` previously used three global `Arc<Mutex<T>>` fields
(tracker, zones, rules) that **serialized all camera frame processing**.  A
frame from Camera B could not begin tracking while Camera A's `update()` was
running — even though their track stores are independent.

Three targeted refactors remove the bottlenecks:

| Component | Before | After |
|---|---|---|
| Tracker | Single global `Mutex<Tracker>` | Per-camera `HashMap<camera_id, Arc<Mutex<Tracker>>>` |
| ZoneEvaluator | Single global `Mutex<ZoneEvaluator>` | Per-camera `HashMap<camera_id, Arc<Mutex<ZoneEvaluator>>>` |
| RuleEngine | Global lock held across DB queries | Lock held only during `evaluate()`; DB ops moved outside |

`ModelManager` and `ObservationEngine` were already lock-free or well-structured
(RwLock + per-session Mutex) and required no changes.

---

## Bottleneck Analysis

### 1. Tracker — Global Mutex

**Before:** `tracker: Arc<Mutex<Tracker>>`

All cameras shared one `Tracker` with a flat `HashMap<Uuid, Track>`.  Even after
the Phase 14B camera_id scoping fix, the single mutex meant:

```
Camera A frame ──► lock(tracker) ──► update() ──► unlock
Camera B frame ──► wait............. lock(tracker) ──► update() ──► unlock
```

Camera B's frame was blocked for the entire duration of Camera A's tracker
update (~1-5 ms depending on track count).

**After:** Per-camera instances

```rust
trackers: Arc<RwLock<HashMap<Uuid, Arc<Mutex<Tracker>>>>>
```

On each `process_frame()` call, the pipeline:
1. Acquires the outer `RwLock` briefly to clone the per-camera `Arc<Mutex<Tracker>>`.
2. Acquires only that camera's mutex for `update()`.

Different cameras never contend on the same mutex.

**Contension eliminated:** 100% for independent cameras.

---

### 2. ZoneEvaluator — Global Mutex

**Before:** `zones: Arc<Mutex<ZoneEvaluator>>`

Same pattern as Tracker.  `ZoneEvaluator.presence` is a
`HashMap<(zone_id, track_id), Presence>` — already keyed by camera-scoped
track IDs — but the single mutex serialized all cameras.

**After:** Per-camera instances

```rust
zones: Arc<RwLock<HashMap<Uuid, Arc<Mutex<ZoneEvaluator>>>>>
```

Identical pattern to Tracker.  Per-camera lock acquisition only.

**Contension eliminated:** 100% for independent cameras.

---

### 3. RuleEngine — Lock Held Across DB Queries (Most Severe)

**Before:** The rule lock was acquired and held for the **entire** observation
loop:

```rust
let mut rule_engine = self.rules.lock().await;          // ← lock acquired
for observation in &observations {
    let identity = self.database.assign_identity(...).await?;  // DB query
    for event in rule_engine.evaluate(...) {
        self.database.insert_event(&event).await?;             // DB write
        self.database.insert_notification(&event).await?;      // DB write
        self.database.get_camera(...).await?;                  // DB query
        self.database.actions_for_rule(...).await?;            // DB query
        self.database.insert_execution(...).await?;            // DB write
    }
}                                                        // ← lock released
```

With 5 observations and 3 DB round-trips each, the lock was held for
**15 sequential DB queries** — blocking all other cameras for potentially
50-200 ms.

**After:** Two-phase approach

```rust
// Phase 1: Short lock — evaluate rules only (no DB)
{
    let mut rule_engine = self.rules.lock().await;
    for observation in &observations {
        let matched = rule_engine.evaluate(&rules, context, now);
        // Collect events, don't do DB work
    }
}  // ← lock released (microseconds)

// Phase 2: DB persistence (no lock held)
for event in events {
    self.database.insert_event(&event).await?;
    self.database.insert_notification(&event).await?;
    // ... all DB operations here, no lock held
}
```

**Contension reduced:** From ~100 ms to <1 ms of lock hold time.

---

### 4. ModelManager — Already Optimal

`ModelManager` uses:
- `Arc<tokio::sync::RwLock<HashMap<Uuid, Arc<LoadedModel>>>>` for model storage.
- `Mutex<Session>` **per loaded model** for ONNX inference.

Different models can infer simultaneously.  Same-model inferences serialize on
the per-session mutex, which is correct (ONNX sessions are not thread-safe).

**No changes required.**

---

### 5. Other Components

| Component | Lock Type | Assessment |
|---|---|---|
| `ActionDispatcher` | None | Stateless; safe |
| `ObservationEngine` | None | Stateless; safe |
| `BehaviourAnalyzer` | None | Stateless; safe |
| `FusionEngine` | None | Stateless; safe |
| `Database` | Connection pool | SQLx pool handles concurrency internally |

---

## Scalability Analysis

### Frame Processing Pipeline Stages

Each camera frame goes through these stages:

```
1. Database query (model assignments)     ~1-5 ms
2. ONNX inference (per model)             ~20-100 ms (GPU), ~200-500 ms (CPU)
3. Fusion                                ~0.1 ms
4. Tracker update                         ~0.5-2 ms
5. Database writes (detections, tracks)   ~2-5 ms
6. Behaviour analysis                     ~0.1 ms
7. Zone evaluation                        ~0.5-1 ms
8. Observation generation                 ~0.1 ms
9. Rule evaluation                        ~0.1 ms
10. Database writes (events, etc.)        ~3-10 ms
11. Action dispatch (async, non-blocking)  ~0 ms (spawned)
12. Clip/snapshot (async, non-blocking)    ~0 ms (spawned)
```

**Total per-frame:** ~30-120 ms (GPU) or ~230-620 ms (CPU)

### Concurrent Camera Capacity

| Cameras | Bottleneck | Estimated FPS/Camera |
|---|---|---|
| 3 | ONNX inference (GPU) | 5-10 fps |
| 3 | ONNX inference (CPU) | 0.3-0.5 fps |
| 5 | GPU memory / DB pool | 5-8 fps (GPU) |
| 5 | CPU inference queue | 0.1-0.3 fps |
| 10 | GPU memory contention | 3-5 fps (GPU, batch) |
| 10 | DB connection pool | 2-4 fps (GPU, limited by writes) |

**GPU inference** is the primary bottleneck for ≤5 cameras.  Each camera's
ONNX session runs independently on the GPU, but GPU compute is shared.

**DB connection pool** becomes the bottleneck at 10+ cameras.  SQLx defaults
to `max_connections = 10` — with 10 cameras each doing 3-5 DB round-trips per
frame, the pool saturates.

### Recommendations for Scale

| Scale | Recommendation |
|---|---|
| ≤5 cameras | Current architecture is sufficient |
| 5-10 cameras | Increase `sqlx::Pool::max_connections` to 20-30 |
| 10-20 cameras | Batch DB writes (multi-row inserts); use a write-ahead queue |
| 20+ cameras | Separate inference service; use dedicated DB writer task |

---

## Files Changed

| File | Change |
|---|---|
| `crates/pipeline/src/lib.rs` | Per-camera tracker/zone instances; RuleEngine lock minimization |
| `crates/tracker/src/lib.rs` | Camera-scoped track matching (Phase 14B) |

---

## Test Coverage

All existing tests continue to pass (11 total across pipeline, tracker, rules,
zones).  Phase 14B added 5 cross-camera isolation tests for the tracker.
